use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use angel_engine_client::{
    AngelSession, RuntimeOptionsOverrides, SendTextRequest, TurnRunEvent, create_runtime_options,
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
        let request = SendTextRequest {
            text: prompt,
            cwd: Some(workspace_path),
            permission_mode: Some(default_permission_mode(&runtime_name).to_string()),
            ..SendTextRequest::default()
        };
        let events = session.start_text_turn(request).map_err(angel_error)?;
        for event in events {
            write_turn_event(&event)?;
        }

        let started = Instant::now();
        loop {
            if started.elapsed() > Duration::from_millis(timeout_ms) {
                session.close();
                return Err(LunaError::Agent("job turn timed out".to_string()));
            }
            match session
                .next_turn_event(Duration::from_millis(250))
                .map_err(angel_error)?
            {
                Some(event) => {
                    let done = matches!(event, TurnRunEvent::Result { .. });
                    let approval_id = permission_elicitation_id(&event);
                    write_turn_event(&event)?;
                    if let Some(elicitation_id) = approval_id {
                        let events = session
                            .resolve_elicitation(
                                elicitation_id,
                                angel_engine_client::ElicitationResponse::AllowForSession,
                            )
                            .map_err(angel_error)?;
                        for event in events {
                            write_turn_event(&event)?;
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
    })
    .await
    .map_err(|err| LunaError::Agent(format!("job task failed: {err}")))?
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

fn write_turn_event(event: &TurnRunEvent) -> Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer(&mut lock, event)?;
    lock.write_all(b"\n")?;
    lock.flush()?;
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
    use super::job_workspace_key;

    #[test]
    fn job_workspace_key_uses_prompt_slug() {
        let key = job_workspace_key("Fix flaky tests!!!\nmore");
        assert!(key.starts_with("JOB-"));
        assert!(key.ends_with("fix-flaky-tests"));
    }
}
