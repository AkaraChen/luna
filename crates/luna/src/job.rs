use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use angel_engine_client::{
    AngelSession, ElicitationResponse, RuntimeOptionsOverrides, SendTextRequest, TurnRunEvent,
    create_runtime_options,
};
use chrono::Utc;
use clap::ValueEnum;

use crate::{
    agent::command_line::split_command,
    config::{CodexRunner, OpencodeRunner, RunnerConfig},
    error::{LunaError, Result},
    paths::absolutize_path,
    workflow::WorkflowStore,
    workspace::{WorkspaceManager, sanitize_workspace_key},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum JobWorkspaceMode {
    None,
    Repo,
    Worktree,
}

#[derive(Debug)]
pub struct JobOptions {
    pub workflow_path: PathBuf,
    pub prompt: String,
    pub workspace: JobWorkspaceMode,
}

pub async fn run_job(options: JobOptions) -> Result<()> {
    let store = WorkflowStore::load(options.workflow_path.clone())?;
    let workflow = store.current();
    let workspace_dir = resolve_job_workspace(&options, workflow).await?;

    let result = run_job_in_workspace(
        &workflow.config.runner,
        &workspace_dir.path,
        &options.prompt,
    )
    .await;

    if let Some(manager) = &workspace_dir.manager {
        manager
            .after_run_best_effort(&workspace_dir.assignment)
            .await;
        if let Err(err) = manager
            .cleanup(&workspace_dir.assignment.workspace_key)
            .await
        {
            eprintln!("failed to cleanup job workspace: {err}");
        }
    }

    if let Some(path) = &workspace_dir.temp_path {
        if let Err(err) = tokio::fs::remove_dir_all(path).await {
            eprintln!(
                "failed to remove temporary job workspace {}: {err}",
                path.display()
            );
        }
    }

    result
}

struct ResolvedJobWorkspace {
    path: PathBuf,
    temp_path: Option<PathBuf>,
    manager: Option<WorkspaceManager>,
    assignment: crate::model::WorkspaceAssignment,
}

async fn resolve_job_workspace(
    options: &JobOptions,
    workflow: &crate::workflow::LoadedWorkflow,
) -> Result<ResolvedJobWorkspace> {
    match options.workspace {
        JobWorkspaceMode::None => {
            let path = create_tmp_job_dir().await?;
            Ok(ResolvedJobWorkspace {
                path: path.clone(),
                temp_path: Some(path.clone()),
                manager: None,
                assignment: crate::model::WorkspaceAssignment {
                    path,
                    workspace_key: "none".to_string(),
                    created_now: true,
                },
            })
        }
        JobWorkspaceMode::Repo => Ok(ResolvedJobWorkspace {
            path: workflow.config.workflow_dir.clone(),
            temp_path: None,
            manager: None,
            assignment: crate::model::WorkspaceAssignment {
                path: workflow.config.workflow_dir.clone(),
                workspace_key: "repo".to_string(),
                created_now: false,
            },
        }),
        JobWorkspaceMode::Worktree => {
            let key = job_workspace_key(&options.prompt);
            let manager = WorkspaceManager::new(
                workflow.config.workspace.root.clone(),
                workflow.config.hooks.clone(),
                Some(workflow.config.workflow_dir.clone()),
            );
            let assignment = manager.prepare(&key).await?;
            if let Err(err) = manager.before_run(&assignment).await {
                if let Err(cleanup_err) = manager.cleanup(&assignment.workspace_key).await {
                    eprintln!(
                        "failed to cleanup job workspace after before_run error: {cleanup_err}"
                    );
                }
                return Err(err);
            }
            Ok(ResolvedJobWorkspace {
                path: assignment.path.clone(),
                temp_path: None,
                manager: Some(manager),
                assignment,
            })
        }
    }
}

async fn run_job_in_workspace(
    runner: &RunnerConfig,
    workspace_path: &Path,
    prompt: &str,
) -> Result<()> {
    match runner {
        RunnerConfig::Codex(config) => run_angel_job("codex", config, workspace_path, prompt).await,
        RunnerConfig::Opencode(config) => {
            run_angel_job("opencode", config, workspace_path, prompt).await
        }
        RunnerConfig::Acp(_) => Err(LunaError::Agent(
            "luna job only supports angel-engine runners: codex, opencode".to_string(),
        )),
    }
}

trait AngelJobRunnerConfig {
    fn command(&self) -> &str;
    fn args(&self) -> &[String];
    fn turn_timeout_ms(&self) -> u64;
}

impl AngelJobRunnerConfig for CodexRunner {
    fn command(&self) -> &str {
        &self.command
    }
    fn args(&self) -> &[String] {
        &self.args
    }
    fn turn_timeout_ms(&self) -> u64 {
        self.turn_timeout_ms
    }
}

impl AngelJobRunnerConfig for OpencodeRunner {
    fn command(&self) -> &str {
        &self.command
    }
    fn args(&self) -> &[String] {
        &self.args
    }
    fn turn_timeout_ms(&self) -> u64 {
        self.turn_timeout_ms
    }
}

trait AngelJobSession {
    fn start_text_turn(&mut self, request: SendTextRequest) -> Result<Vec<TurnRunEvent>>;
    fn next_turn_event(&mut self, timeout: Duration) -> Result<Option<TurnRunEvent>>;
    fn resolve_elicitation(
        &mut self,
        elicitation_id: String,
        response: ElicitationResponse,
    ) -> Result<Vec<TurnRunEvent>>;
    fn close(&mut self);
}

impl AngelJobSession for AngelSession {
    fn start_text_turn(&mut self, request: SendTextRequest) -> Result<Vec<TurnRunEvent>> {
        AngelSession::start_text_turn(self, request).map_err(angel_error)
    }

    fn next_turn_event(&mut self, timeout: Duration) -> Result<Option<TurnRunEvent>> {
        AngelSession::next_turn_event(self, timeout).map_err(angel_error)
    }

    fn resolve_elicitation(
        &mut self,
        elicitation_id: String,
        response: ElicitationResponse,
    ) -> Result<Vec<TurnRunEvent>> {
        AngelSession::resolve_elicitation(self, elicitation_id, response).map_err(angel_error)
    }

    fn close(&mut self) {
        AngelSession::close(self);
    }
}

async fn run_angel_job(
    runtime_name: &str,
    config: &impl AngelJobRunnerConfig,
    workspace_path: &Path,
    prompt: &str,
) -> Result<()> {
    let workspace_path =
        std::fs::canonicalize(workspace_path).or_else(|_| absolutize_path(workspace_path))?;
    let workspace_path = workspace_path.to_string_lossy().to_string();
    let (command, args) = split_command(config.command(), config.args())?;
    let timeout_ms = config.turn_timeout_ms();
    let prompt = prompt.to_string();
    let runtime_name = runtime_name.to_string();

    tokio::task::spawn_blocking(move || {
        let options = create_runtime_options(
            Some(&runtime_name),
            RuntimeOptionsOverrides {
                command: Some(command),
                args: Some(args),
                cwd: Some(workspace_path.clone()),
                process_label: Some(format!("luna:job:{runtime_name}")),
                client_name: Some("luna".to_string()),
                client_title: Some("Luna".to_string()),
                ..RuntimeOptionsOverrides::default()
            },
        );
        let mut session = AngelSession::new(options).map_err(angel_error)?;
        let stdout = io::stdout();
        let mut output = stdout.lock();
        run_angel_job_session(
            &mut session,
            &runtime_name,
            workspace_path,
            prompt,
            timeout_ms,
            &mut output,
        )
    })
    .await
    .map_err(|err| LunaError::Agent(format!("job task failed: {err}")))?
}

fn run_angel_job_session(
    session: &mut impl AngelJobSession,
    runtime_name: &str,
    workspace_path: String,
    prompt: String,
    timeout_ms: u64,
    output: &mut impl Write,
) -> Result<()> {
    let request = SendTextRequest {
        text: prompt,
        cwd: Some(workspace_path),
        permission_mode: Some(default_permission_mode(runtime_name).to_string()),
        ..SendTextRequest::default()
    };
    let events = session.start_text_turn(request)?;
    for event in events {
        write_turn_event_to(output, &event)?;
    }

    let started = Instant::now();
    loop {
        if started.elapsed() > Duration::from_millis(timeout_ms) {
            session.close();
            return Err(LunaError::Agent("job turn timed out".to_string()));
        }
        match session.next_turn_event(Duration::from_millis(250))? {
            Some(event) => {
                let done = matches!(event, TurnRunEvent::Result { .. });
                let approval_id = permission_elicitation_id(&event);
                write_turn_event_to(output, &event)?;
                if let Some(elicitation_id) = approval_id {
                    let events = session.resolve_elicitation(
                        elicitation_id,
                        ElicitationResponse::AllowForSession,
                    )?;
                    for event in events {
                        write_turn_event_to(output, &event)?;
                    }
                }
                if done {
                    session.close();
                    return Ok(());
                }
            }
            None => {}
        }
    }
}

fn default_permission_mode(runtime_name: &str) -> &'static str {
    match runtime_name {
        "codex" => "never",
        "opencode" => "bypassPermissions",
        _ => "never",
    }
}

fn permission_elicitation_id(event: &TurnRunEvent) -> Option<String> {
    let TurnRunEvent::Elicitation { elicitation, .. } = event else {
        return None;
    };
    matches!(elicitation.kind.as_str(), "approval" | "permissionProfile")
        .then(|| elicitation.id.clone())
}

fn write_turn_event_to(output: &mut impl Write, event: &TurnRunEvent) -> Result<()> {
    serde_json::to_writer(&mut *output, event)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

async fn create_tmp_job_dir() -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "luna-job-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    tokio::fs::create_dir_all(&path).await?;
    Ok(path)
}

fn job_workspace_key(prompt: &str) -> String {
    let first_line = prompt
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("job");
    let slug = first_line
        .chars()
        .take(48)
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() {
        "job".to_string()
    } else {
        slug
    };
    sanitize_workspace_key(&format!(
        "JOB-{}-{slug}",
        Utc::now().format("%Y%m%d-%H%M%S")
    ))
}

fn angel_error(error: angel_engine_client::ClientError) -> LunaError {
    LunaError::Agent(format!("angel-engine client error: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, time::Duration};

    use angel_engine_client::{
        DisplayMessagePartSnapshot, ElicitationResponse, ElicitationSnapshot, SendTextRequest,
        TurnRunEvent,
    };
    use serde_json::{Value, json};
    use tempfile::tempdir;

    use crate::{
        config::{AcpRunner, CodexRunner, RunnerConfig},
        error::Result,
        workflow::WorkflowStore,
    };

    use super::{
        AngelJobSession, JobOptions, JobWorkspaceMode, default_permission_mode, job_workspace_key,
        permission_elicitation_id, resolve_job_workspace, run_angel_job_session, run_job,
        run_job_in_workspace,
    };

    #[test]
    fn job_workspace_key_uses_prompt_slug() {
        let key = job_workspace_key("Fix flaky tests!!!\nmore");
        assert!(key.starts_with("JOB-"));
        assert!(key.ends_with("fix-flaky-tests"));
    }

    async fn write_codex_workflow() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempdir().expect("tempdir");
        let workflow_path = temp.path().join("WORKFLOW.md");
        tokio::fs::write(
            &workflow_path,
            r#"---
tracker:
  kind: asahi
  db: ./asahi.db
workspace:
  root: ./.luna/workspaces
runner:
  kind: codex
  command: codex app-server
---
test prompt
"#,
        )
        .await
        .expect("write workflow");
        (temp, workflow_path)
    }

    #[tokio::test]
    async fn job_workspace_none_creates_temporary_codex_workspace() {
        let (_temp, workflow_path) = write_codex_workflow().await;
        let store = WorkflowStore::load(workflow_path.clone()).expect("workflow");
        let options = JobOptions {
            workflow_path,
            prompt: "Inspect repository".to_string(),
            workspace: JobWorkspaceMode::None,
        };

        let workspace = resolve_job_workspace(&options, store.current())
            .await
            .expect("workspace");

        assert!(workspace.path.exists());
        assert_eq!(workspace.assignment.workspace_key, "none");
        assert!(workspace.manager.is_none());
        let temp_path = workspace.temp_path.expect("temp path");
        tokio::fs::remove_dir_all(temp_path)
            .await
            .expect("cleanup temp");
    }

    #[tokio::test]
    async fn job_workspace_repo_uses_workflow_directory() {
        let (temp, workflow_path) = write_codex_workflow().await;
        let store = WorkflowStore::load(workflow_path.clone()).expect("workflow");
        let options = JobOptions {
            workflow_path,
            prompt: "Inspect repository".to_string(),
            workspace: JobWorkspaceMode::Repo,
        };

        let workspace = resolve_job_workspace(&options, store.current())
            .await
            .expect("workspace");

        assert_eq!(workspace.path, temp.path());
        assert_eq!(workspace.assignment.workspace_key, "repo");
        assert!(workspace.temp_path.is_none());
        assert!(workspace.manager.is_none());
    }

    #[tokio::test]
    async fn job_workspace_worktree_uses_prompt_slug_under_configured_root() {
        let (_temp, workflow_path) = write_codex_workflow().await;
        let store = WorkflowStore::load(workflow_path.clone()).expect("workflow");
        let options = JobOptions {
            workflow_path,
            prompt: "Fix flaky tests!!!\nmore".to_string(),
            workspace: JobWorkspaceMode::Worktree,
        };

        let workspace = resolve_job_workspace(&options, store.current())
            .await
            .expect("workspace");

        assert!(
            workspace
                .assignment
                .workspace_key
                .contains("fix-flaky-tests")
        );
        assert!(
            workspace
                .path
                .ends_with(&workspace.assignment.workspace_key)
        );
        assert!(workspace.path.exists());
        if let Some(manager) = &workspace.manager {
            manager
                .cleanup(&workspace.assignment.workspace_key)
                .await
                .expect("cleanup worktree");
        }
    }

    #[tokio::test]
    async fn codex_job_rejects_invalid_command_before_launching_runtime() {
        let temp = tempdir().expect("tempdir");
        let runner = RunnerConfig::Codex(CodexRunner {
            command: "\"codex".to_string(),
            ..CodexRunner::default()
        });

        let err = run_job_in_workspace(&runner, temp.path(), "hello")
            .await
            .expect_err("invalid command should fail");

        assert!(err.to_string().contains("unterminated"));
    }

    #[tokio::test]
    async fn job_rejects_acp_runner_before_launching_runtime() {
        let temp = tempdir().expect("tempdir");
        let runner = RunnerConfig::Acp(AcpRunner {
            command: "kimi acp".to_string(),
            turn_timeout_ms: 1_000,
            read_timeout_ms: 1_000,
            stall_timeout_ms: 1_000,
        });

        let err = run_job_in_workspace(&runner, temp.path(), "hello")
            .await
            .expect_err("acp runner should fail before launch");

        assert!(
            err.to_string()
                .contains("luna job only supports angel-engine runners")
        );
    }

    #[tokio::test]
    async fn run_job_cleans_worktree_workspace_after_codex_launch_failure() {
        let temp = tempdir().expect("tempdir");
        let workspace_root = temp.path().join("job-workspaces");
        let workflow_path = temp.path().join("WORKFLOW.md");
        let workspace_root = workspace_root
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        tokio::fs::write(
            &workflow_path,
            format!(
                r#"---
tracker:
  kind: asahi
  db: ./asahi.db
workspace:
  root: "{workspace_root}"
runner:
  kind: codex
  command: '"codex'
---
test prompt
"#
            ),
        )
        .await
        .expect("write workflow");

        let err = run_job(JobOptions {
            workflow_path,
            prompt: "Fix launch failure".to_string(),
            workspace: JobWorkspaceMode::Worktree,
        })
        .await
        .expect_err("invalid command should fail");

        assert!(err.to_string().contains("unterminated"));
        let mut entries = tokio::fs::read_dir(temp.path().join("job-workspaces"))
            .await
            .expect("workspace root");
        assert!(
            entries
                .next_entry()
                .await
                .expect("read workspace root")
                .is_none(),
            "failed Codex job should cleanup its prepared worktree workspace"
        );
    }

    #[test]
    fn codex_job_default_permission_mode_is_never() {
        assert_eq!(default_permission_mode("codex"), "never");
    }

    fn empty_message_part() -> DisplayMessagePartSnapshot {
        DisplayMessagePartSnapshot {
            kind: "tool".to_string(),
            text: None,
            data: None,
            mime_type: None,
            name: None,
            action: None,
            plan: None,
        }
    }

    fn elicitation_event(kind: &str) -> TurnRunEvent {
        TurnRunEvent::Elicitation {
            elicitation: ElicitationSnapshot {
                id: format!("{kind}-id"),
                turn_id: Some("turn".to_string()),
                action_id: Some("action".to_string()),
                kind: kind.to_string(),
                phase: "open".to_string(),
                title: None,
                body: None,
                choices: Vec::new(),
                questions: Vec::new(),
            },
            message_part: empty_message_part(),
        }
    }

    fn turn_event(value: Value) -> TurnRunEvent {
        serde_json::from_value(value).expect("turn event")
    }

    fn message_part_json() -> Value {
        json!({"type": "text", "text": ""})
    }

    fn result_event() -> TurnRunEvent {
        turn_event(json!({
            "type": "result",
            "result": {
                "remoteThreadId": "thread-1",
                "turnId": "turn-1",
                "conversation": null
            }
        }))
    }

    fn delta_event(text: &str) -> TurnRunEvent {
        turn_event(json!({
            "type": "delta",
            "part": "text",
            "text": text,
            "turnId": "turn-1",
            "messagePart": message_part_json()
        }))
    }

    #[derive(Default)]
    struct FakeJobSession {
        start_events: Vec<TurnRunEvent>,
        next_events: VecDeque<Option<TurnRunEvent>>,
        resolve_events: Vec<TurnRunEvent>,
        requests: Vec<SendTextRequest>,
        resolved: Vec<(String, ElicitationResponse)>,
        next_timeouts: Vec<Duration>,
        closed: u32,
    }

    impl FakeJobSession {
        fn new(start_events: Vec<TurnRunEvent>, next_events: Vec<Option<TurnRunEvent>>) -> Self {
            Self {
                start_events,
                next_events: next_events.into_iter().collect(),
                ..Self::default()
            }
        }
    }

    impl AngelJobSession for FakeJobSession {
        fn start_text_turn(&mut self, request: SendTextRequest) -> Result<Vec<TurnRunEvent>> {
            self.requests.push(request);
            Ok(self.start_events.clone())
        }

        fn next_turn_event(&mut self, timeout: Duration) -> Result<Option<TurnRunEvent>> {
            self.next_timeouts.push(timeout);
            let event = self.next_events.pop_front().flatten();
            if event.is_none() {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(event)
        }

        fn resolve_elicitation(
            &mut self,
            elicitation_id: String,
            response: ElicitationResponse,
        ) -> Result<Vec<TurnRunEvent>> {
            self.resolved.push((elicitation_id, response));
            Ok(self.resolve_events.clone())
        }

        fn close(&mut self) {
            self.closed += 1;
        }
    }

    #[test]
    fn codex_job_auto_allows_only_permission_elicitations() {
        assert_eq!(
            permission_elicitation_id(&elicitation_event("approval")).as_deref(),
            Some("approval-id")
        );
        assert_eq!(
            permission_elicitation_id(&elicitation_event("permissionProfile")).as_deref(),
            Some("permissionProfile-id")
        );
        assert!(permission_elicitation_id(&elicitation_event("userInput")).is_none());
        assert!(permission_elicitation_id(&elicitation_event("dynamicToolCall")).is_none());
    }

    #[test]
    fn codex_job_session_streams_jsonl_resolves_permission_and_closes_on_result() {
        let mut session = FakeJobSession::new(
            vec![delta_event("started")],
            vec![Some(elicitation_event("approval")), Some(result_event())],
        );
        session.resolve_events = vec![delta_event("approved")];
        let mut output = Vec::new();

        run_angel_job_session(
            &mut session,
            "codex",
            "/tmp/luna-job".to_string(),
            "inspect repo".to_string(),
            1_000,
            &mut output,
        )
        .expect("job session");

        assert_eq!(session.requests.len(), 1);
        assert_eq!(session.requests[0].text, "inspect repo");
        assert_eq!(session.requests[0].cwd.as_deref(), Some("/tmp/luna-job"));
        assert_eq!(
            session.requests[0].permission_mode.as_deref(),
            Some("never")
        );
        assert_eq!(session.resolved.len(), 1);
        assert_eq!(session.resolved[0].0, "approval-id");
        assert!(matches!(
            session.resolved[0].1,
            ElicitationResponse::AllowForSession
        ));
        assert_eq!(session.next_timeouts, vec![Duration::from_millis(250); 2]);
        assert_eq!(session.closed, 1);

        let lines = String::from_utf8(output)
            .expect("utf8 output")
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("json line"))
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0]["type"], "delta");
        assert_eq!(lines[0]["text"], "started");
        assert_eq!(lines[1]["type"], "elicitation");
        assert_eq!(lines[1]["elicitation"]["id"], "approval-id");
        assert_eq!(lines[2]["type"], "delta");
        assert_eq!(lines[2]["text"], "approved");
        assert_eq!(lines[3]["type"], "result");
    }

    #[test]
    fn codex_job_session_does_not_resolve_user_input_and_times_out() {
        let mut session =
            FakeJobSession::new(Vec::new(), vec![Some(elicitation_event("userInput")), None]);
        let mut output = Vec::new();

        let err = run_angel_job_session(
            &mut session,
            "codex",
            "/tmp/luna-job".to_string(),
            "inspect repo".to_string(),
            1,
            &mut output,
        )
        .expect_err("timeout");

        assert!(err.to_string().contains("job turn timed out"));
        assert!(session.resolved.is_empty());
        assert_eq!(session.closed, 1);
        let lines = String::from_utf8(output)
            .expect("utf8 output")
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("json line"))
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["type"], "elicitation");
        assert_eq!(lines[0]["elicitation"]["kind"], "userInput");
    }
}
