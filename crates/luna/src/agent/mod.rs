use std::{collections::HashSet, path::Path, time::Instant};

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use tokio::sync::{mpsc, watch};

use crate::{
    config::RunnerConfig,
    error::{LunaError, Result},
    model::{Comment, Issue},
    prompt::{build_continuation_prompt, render_issue_prompt},
    tracker::build_tracker,
    workflow::LoadedWorkflow,
    workspace::WorkspaceManager,
};

mod acp;
mod angel_runtime;
pub(crate) mod command_line;

pub use acp::AcpSession;
pub use angel_runtime::AngelRuntimeSession;

pub type CodexSession = AngelRuntimeSession;

#[derive(Clone, Debug)]
pub enum StopReason {
    NonActive,
    Terminal,
    Stalled,
    Shutdown,
}

#[derive(Clone, Debug)]
pub struct UsageUpdate {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Clone, Debug)]
pub struct SessionUpdate {
    pub issue_id: String,
    pub issue_identifier: String,
    pub event: String,
    pub timestamp: chrono::DateTime<Utc>,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub agent_pid: Option<u32>,
    pub message: Option<String>,
    pub usage: Option<UsageUpdate>,
    pub rate_limits: Option<Value>,
    pub turn_count: Option<u32>,
}

#[derive(Clone, Debug)]
pub enum WorkerOutcome {
    Normal,
    Failed(String),
    TimedOut,
    Stalled,
    CanceledByReconciliation,
}

#[derive(Clone, Debug)]
pub struct WorkerExit {
    pub issue_id: String,
    pub issue_identifier: String,
    pub outcome: WorkerOutcome,
    pub runtime_seconds: f64,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CommandExecutionEvent {
    pub issue_id: String,
    pub issue_identifier: String,
    pub command: String,
    pub cwd: Option<String>,
    pub duration_ms: Option<i64>,
    pub exit_code: Option<i64>,
}

#[derive(Clone, Debug)]
pub enum WorkerEvent {
    Session(SessionUpdate),
    Exited(WorkerExit),
    RetryDue(String),
    CommandExecuted(CommandExecutionEvent),
}

pub enum TurnExit {
    Completed,
    Failed(String),
    TimedOut,
    Stopped(StopReason),
}

#[async_trait]
pub trait AgentSession: Send {
    async fn start(&mut self) -> Result<()>;
    async fn run_turn(
        &mut self,
        prompt: &str,
        turn_number: u32,
        stop_rx: &mut watch::Receiver<Option<StopReason>>,
    ) -> Result<TurnExit>;
    async fn send_comment(&mut self, body: &str) -> Result<()>;
    async fn shutdown(&mut self);
}

#[async_trait]
trait AgentSessionFactory: Send + Sync {
    async fn build_session(
        &self,
        config: &RunnerConfig,
        workspace_path: &Path,
        issue_id: String,
        issue_identifier: String,
        events: mpsc::UnboundedSender<WorkerEvent>,
    ) -> Result<Box<dyn AgentSession>>;
}

struct DefaultAgentSessionFactory;

#[async_trait]
impl AgentSessionFactory for DefaultAgentSessionFactory {
    async fn build_session(
        &self,
        config: &RunnerConfig,
        workspace_path: &Path,
        issue_id: String,
        issue_identifier: String,
        events: mpsc::UnboundedSender<WorkerEvent>,
    ) -> Result<Box<dyn AgentSession>> {
        build_agent_session(config, workspace_path, issue_id, issue_identifier, events).await
    }
}

pub async fn run_agent_attempt(
    issue: Issue,
    attempt: Option<u32>,
    workflow: LoadedWorkflow,
    events: mpsc::UnboundedSender<WorkerEvent>,
    stop_rx: watch::Receiver<Option<StopReason>>,
    comment_rx: mpsc::Receiver<String>,
) {
    let started = Instant::now();
    let session_factory = DefaultAgentSessionFactory;
    let outcome = run_agent_attempt_inner(
        issue.clone(),
        attempt,
        workflow,
        events.clone(),
        stop_rx,
        comment_rx,
        &session_factory,
    )
    .await;

    let (worker_outcome, error) = match outcome {
        Ok(WorkerOutcome::Normal) => (WorkerOutcome::Normal, None),
        Ok(other) => {
            let reason = match &other {
                WorkerOutcome::Failed(reason) => Some(reason.clone()),
                WorkerOutcome::TimedOut => Some("turn_timeout".to_string()),
                WorkerOutcome::Stalled => Some("stalled".to_string()),
                WorkerOutcome::CanceledByReconciliation => {
                    Some("canceled_by_reconciliation".to_string())
                }
                WorkerOutcome::Normal => None,
            };
            (other, reason)
        }
        Err(err) => (
            WorkerOutcome::Failed(err.to_string()),
            Some(err.to_string()),
        ),
    };

    let _ = events.send(WorkerEvent::Exited(WorkerExit {
        issue_id: issue.id,
        issue_identifier: issue.identifier,
        outcome: worker_outcome,
        runtime_seconds: started.elapsed().as_secs_f64(),
        error,
    }));
}

async fn run_agent_attempt_inner(
    mut issue: Issue,
    attempt: Option<u32>,
    workflow: LoadedWorkflow,
    events: mpsc::UnboundedSender<WorkerEvent>,
    mut stop_rx: watch::Receiver<Option<StopReason>>,
    mut comment_rx: mpsc::Receiver<String>,
    session_factory: &dyn AgentSessionFactory,
) -> Result<WorkerOutcome> {
    tracing::info!(
        issue_id = %issue.id,
        identifier = %issue.identifier,
        attempt = attempt.unwrap_or(0),
        "agent attempt starting"
    );

    let tracker = build_tracker(&workflow.config.tracker)?;
    if let Err(err) = tracker
        .create_activity(
            &issue,
            "agent_started",
            &format!("Agent started on {}", issue.identifier),
            Some(&format!("Working on: {}", issue.title)),
        )
        .await
    {
        tracing::warn!(error = %err, "failed to create agent_started activity");
    }

    let workspace_manager = WorkspaceManager::new(
        workflow.config.workspace.root.clone(),
        workflow.config.hooks.clone(),
        Some(workflow.config.workflow_dir.clone()),
    );
    let workspace = workspace_manager.prepare(&issue.identifier).await?;
    workspace_manager.before_run(&workspace).await?;

    if matches!(
        stop_rx.borrow().clone(),
        Some(
            StopReason::Shutdown
                | StopReason::NonActive
                | StopReason::Terminal
                | StopReason::Stalled
        )
    ) {
        workspace_manager.after_run_best_effort(&workspace).await;
        return Ok(map_stop_reason(stop_rx.borrow().clone()));
    }

    let prompt = match render_issue_prompt(&workflow.definition.prompt_template, &issue, attempt) {
        Ok(prompt) => prompt,
        Err(err) => {
            workspace_manager.after_run_best_effort(&workspace).await;
            return Err(err);
        }
    };

    let mut session = match session_factory
        .build_session(
            &workflow.config.runner,
            &workspace.path,
            issue.id.clone(),
            issue.identifier.clone(),
            events.clone(),
        )
        .await
    {
        Ok(session) => session,
        Err(err) => {
            workspace_manager.after_run_best_effort(&workspace).await;
            return Err(err);
        }
    };

    let outcome = async {
        session.start().await?;

        let mut turn_number = 1_u32;
        let mut seen_comment_ids = HashSet::new();
        loop {
            while let Ok(body) = comment_rx.try_recv() {
                if let Err(err) = session.send_comment(&body).await {
                    tracing::warn!(issue_id = %issue.id, error = %err, "failed to send comment to agent session");
                }
            }

            let new_comments = match tracker.fetch_comments(&issue).await {
                Ok(comments) => {
                    let new_ones: Vec<Comment> = comments
                        .into_iter()
                        .filter(|c| !seen_comment_ids.contains(&c.id))
                        .collect();
                    for c in &new_ones {
                        seen_comment_ids.insert(c.id.clone());
                    }
                    new_ones
                }
                Err(err) => {
                    tracing::warn!(issue_id = %issue.id, error = %err, "failed to fetch comments");
                    vec![]
                }
            };

            let prompt = if turn_number == 1 {
                prompt.clone()
            } else {
                build_continuation_prompt(
                    &issue,
                    turn_number,
                    workflow.config.scheduler.max_turns,
                    &new_comments,
                )
            };

            match session.run_turn(&prompt, turn_number, &mut stop_rx).await? {
                TurnExit::Completed => {}
                TurnExit::Failed(reason) => {
                    return Ok(WorkerOutcome::Failed(reason));
                }
                TurnExit::TimedOut => {
                    return Ok(WorkerOutcome::TimedOut);
                }
                TurnExit::Stopped(reason) => {
                    return Ok(map_stop_reason(Some(reason)));
                }
            }

            let refreshed = tracker
                .fetch_issue_states_by_ids(&[issue.id.clone()])
                .await?;
            issue = refreshed.into_iter().next().ok_or_else(|| {
                LunaError::Tracker("issue state refresh error: issue missing after turn".to_string())
            })?;

            if !workflow.config.tracker.is_active_state(&issue.state) {
                break;
            }
            if turn_number >= workflow.config.scheduler.max_turns {
                break;
            }
            turn_number += 1;
        }

        Ok(WorkerOutcome::Normal)
    }
    .await;
    session.shutdown().await;
    workspace_manager.after_run_best_effort(&workspace).await;
    outcome
}

fn map_stop_reason(reason: Option<StopReason>) -> WorkerOutcome {
    match reason {
        Some(StopReason::Stalled) => WorkerOutcome::Stalled,
        Some(StopReason::NonActive | StopReason::Terminal | StopReason::Shutdown) | None => {
            WorkerOutcome::CanceledByReconciliation
        }
    }
}

pub async fn build_agent_session(
    config: &RunnerConfig,
    workspace_path: &Path,
    issue_id: String,
    issue_identifier: String,
    events: mpsc::UnboundedSender<WorkerEvent>,
) -> Result<Box<dyn AgentSession>> {
    match config {
        RunnerConfig::Codex(c) => Ok(Box::new(
            AngelRuntimeSession::launch_codex(
                c,
                workspace_path,
                issue_id,
                issue_identifier,
                events,
            )
            .await?,
        )),
        RunnerConfig::Opencode(c) => Ok(Box::new(
            AngelRuntimeSession::launch_opencode(
                c,
                workspace_path,
                issue_id,
                issue_identifier,
                events,
            )
            .await?,
        )),
        RunnerConfig::Acp(c) => Ok(Box::new(
            AcpSession::launch(c, workspace_path, issue_id, issue_identifier, events).await?,
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    use serde_json::{Value, json};
    use tempfile::tempdir;
    use tokio::sync::{mpsc, watch};

    use crate::{
        agent::{
            AgentSession, StopReason, TurnExit, WorkerEvent, WorkerOutcome, run_agent_attempt,
        },
        config::RunnerConfig,
        error::{LunaError, Result},
        model::Issue,
        test_support::{MockHttpServer, MockResponse, issue_json},
        workflow::{LoadedWorkflow, WorkflowStore},
    };

    use super::{AgentSessionFactory, run_agent_attempt_inner};

    async fn write_codex_asahi_workflow(endpoint: &str) -> (tempfile::TempDir, LoadedWorkflow) {
        let temp = tempdir().expect("tempdir");
        let workflow_path = temp.path().join("WORKFLOW.md");
        tokio::fs::write(
            &workflow_path,
            format!(
                r#"---
tracker:
  kind: asahi
  endpoint: "{endpoint}"
workspace:
  root: ./workspaces
hooks:
  timeout_ms: 1000
scheduler:
  max_turns: 1
runner:
  kind: codex
  command: '"codex'
---
Issue {{{{ issue.identifier }}}}: {{{{ issue.title }}}}
"#
            ),
        )
        .await
        .expect("write workflow");
        let store = WorkflowStore::load(workflow_path).expect("workflow");
        (temp, store.current().clone())
    }

    async fn write_codex_asahi_workflow_with_max_turns(
        endpoint: &str,
        max_turns: u32,
    ) -> (tempfile::TempDir, LoadedWorkflow) {
        let temp = tempdir().expect("tempdir");
        let workflow_path = temp.path().join("WORKFLOW.md");
        tokio::fs::write(
            &workflow_path,
            format!(
                r#"---
tracker:
  kind: asahi
  endpoint: "{endpoint}"
workspace:
  root: ./workspaces
hooks:
  timeout_ms: 1000
scheduler:
  max_turns: {max_turns}
runner:
  kind: codex
  command: codex app-server
---
Issue {{{{ issue.identifier }}}}: {{{{ issue.title }}}}
"#
            ),
        )
        .await
        .expect("write workflow");
        let store = WorkflowStore::load(workflow_path).expect("workflow");
        (temp, store.current().clone())
    }

    async fn write_codex_asahi_workflow_with_after_run(
        endpoint: &str,
    ) -> (tempfile::TempDir, LoadedWorkflow) {
        let temp = tempdir().expect("tempdir");
        let workflow_path = temp.path().join("WORKFLOW.md");
        tokio::fs::write(
            &workflow_path,
            format!(
                r#"---
tracker:
  kind: asahi
  endpoint: "{endpoint}"
workspace:
  root: ./workspaces
hooks:
  timeout_ms: 1000
  after_run: "printf after > after_run.txt"
scheduler:
  max_turns: 2
runner:
  kind: codex
  command: codex app-server
---
Issue {{{{ issue.identifier }}}}: {{{{ issue.title }}}}
"#
            ),
        )
        .await
        .expect("write workflow");
        let store = WorkflowStore::load(workflow_path).expect("workflow");
        (temp, store.current().clone())
    }

    fn test_issue() -> Issue {
        serde_json::from_value(issue_json("1", "ASAHI-1", "Todo", None)).expect("issue")
    }

    fn comment_json(id: &str, body: &str, created_at: &str) -> Value {
        json!({
            "id": id,
            "issue_id": "1",
            "body": body,
            "created_at": created_at
        })
    }

    #[derive(Clone, Debug, Default)]
    struct FakeSessionState {
        starts: u32,
        build_workspace_path: Option<PathBuf>,
        build_issue_id: Option<String>,
        build_issue_identifier: Option<String>,
        prompts: Vec<(u32, String)>,
        comments: Vec<String>,
        shutdowns: u32,
    }

    struct FakeSessionFactory {
        state: Arc<Mutex<FakeSessionState>>,
        start_error: Option<String>,
        outcomes: Mutex<VecDeque<Result<TurnExit>>>,
    }

    impl FakeSessionFactory {
        fn new(outcomes: Vec<TurnExit>) -> Self {
            Self::new_results(outcomes.into_iter().map(Ok).collect(), None)
        }

        fn with_start_error(message: &str) -> Self {
            Self::new_results(Vec::new(), Some(message.to_string()))
        }

        fn with_turn_error(message: &str) -> Self {
            Self::new_results(vec![Err(LunaError::Agent(message.to_string()))], None)
        }

        fn new_results(outcomes: Vec<Result<TurnExit>>, start_error: Option<String>) -> Self {
            Self {
                state: Arc::new(Mutex::new(FakeSessionState::default())),
                start_error,
                outcomes: Mutex::new(outcomes.into_iter().collect()),
            }
        }

        fn state(&self) -> FakeSessionState {
            self.state.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl AgentSessionFactory for FakeSessionFactory {
        async fn build_session(
            &self,
            config: &RunnerConfig,
            workspace_path: &Path,
            issue_id: String,
            issue_identifier: String,
            _events: mpsc::UnboundedSender<WorkerEvent>,
        ) -> Result<Box<dyn AgentSession>> {
            assert!(matches!(config, RunnerConfig::Codex(_)));
            {
                let mut state = self.state.lock().unwrap();
                state.build_workspace_path = Some(workspace_path.to_path_buf());
                state.build_issue_id = Some(issue_id);
                state.build_issue_identifier = Some(issue_identifier);
            }
            let outcomes = {
                let mut outcomes = self.outcomes.lock().unwrap();
                std::mem::take(&mut *outcomes)
            };
            Ok(Box::new(FakeSession {
                state: Arc::clone(&self.state),
                start_error: self.start_error.clone(),
                outcomes,
            }))
        }
    }

    struct FakeSession {
        state: Arc<Mutex<FakeSessionState>>,
        start_error: Option<String>,
        outcomes: VecDeque<Result<TurnExit>>,
    }

    #[async_trait::async_trait]
    impl AgentSession for FakeSession {
        async fn start(&mut self) -> Result<()> {
            self.state.lock().unwrap().starts += 1;
            if let Some(message) = self.start_error.take() {
                return Err(LunaError::Agent(message));
            }
            Ok(())
        }

        async fn run_turn(
            &mut self,
            prompt: &str,
            turn_number: u32,
            _stop_rx: &mut watch::Receiver<Option<StopReason>>,
        ) -> Result<TurnExit> {
            self.state
                .lock()
                .unwrap()
                .prompts
                .push((turn_number, prompt.to_string()));
            self.outcomes.pop_front().unwrap_or(Ok(TurnExit::Completed))
        }

        async fn send_comment(&mut self, body: &str) -> Result<()> {
            self.state.lock().unwrap().comments.push(body.to_string());
            Ok(())
        }

        async fn shutdown(&mut self) {
            self.state.lock().unwrap().shutdowns += 1;
        }
    }

    #[tokio::test]
    async fn codex_agent_attempt_runs_successful_multi_turn_with_comments_and_refresh() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(200, json!({})),
            MockResponse::json(
                200,
                json!({
                    "comments": [
                        comment_json("c1", "initial tracker comment", "2026-01-01T00:00:00Z")
                    ]
                }),
            ),
            MockResponse::json(
                200,
                json!({"issues": [issue_json("1", "ASAHI-1", "In Progress", None)]}),
            ),
            MockResponse::json(
                200,
                json!({
                    "comments": [
                        comment_json("c1", "initial tracker comment", "2026-01-01T00:00:00Z"),
                        comment_json("c2", "fresh tracker comment", "2026-01-02T00:00:00Z")
                    ]
                }),
            ),
            MockResponse::json(
                200,
                json!({"issues": [issue_json("1", "ASAHI-1", "In Progress", None)]}),
            ),
        ])
        .await;
        let endpoint = server.endpoint.clone();
        let (_temp, workflow) = write_codex_asahi_workflow_with_max_turns(&endpoint, 2).await;
        let factory = FakeSessionFactory::new(vec![TurnExit::Completed, TurnExit::Completed]);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let (comments_tx, comments_rx) = mpsc::channel(4);
        comments_tx
            .send("manual queue comment".to_string())
            .await
            .expect("queue comment");
        drop(comments_tx);
        let (_stop_tx, stop_rx) = watch::channel(None);

        let outcome = run_agent_attempt_inner(
            test_issue(),
            Some(3),
            workflow,
            events_tx,
            stop_rx,
            comments_rx,
            &factory,
        )
        .await
        .expect("agent attempt");

        assert!(matches!(outcome, WorkerOutcome::Normal));
        let state = factory.state();
        assert_eq!(state.starts, 1);
        assert_eq!(state.build_issue_id.as_deref(), Some("1"));
        assert_eq!(state.build_issue_identifier.as_deref(), Some("ASAHI-1"));
        assert!(
            state
                .build_workspace_path
                .as_ref()
                .is_some_and(|path| path.ends_with("ASAHI-1"))
        );
        assert_eq!(state.comments, vec!["manual queue comment"]);
        assert_eq!(state.prompts.len(), 2);
        assert_eq!(state.prompts[0].0, 1);
        assert!(state.prompts[0].1.contains("Issue ASAHI-1"));
        assert!(!state.prompts[0].1.contains("New comments"));
        assert_eq!(state.prompts[1].0, 2);
        assert!(state.prompts[1].1.contains("continuation turn 2/2"));
        assert!(state.prompts[1].1.contains("- fresh tracker comment"));
        assert!(!state.prompts[1].1.contains("initial tracker comment"));
        assert_eq!(state.shutdowns, 1);

        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 5);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].target, "/api/issues/1/activities");
        assert_eq!(requests[1].target, "/api/issues/1/comments");
        assert!(requests[2].target.starts_with("/api/issues?"));
        assert!(requests[2].target.contains("ids=1"));
        assert_eq!(requests[3].target, "/api/issues/1/comments");
        assert!(requests[4].target.starts_with("/api/issues?"));
        assert!(requests[4].target.contains("ids=1"));
    }

    #[tokio::test]
    async fn codex_agent_attempt_stops_after_turn_when_tracker_state_is_no_longer_active() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(200, json!({})),
            MockResponse::json(200, json!({"comments": []})),
            MockResponse::json(
                200,
                json!({"issues": [issue_json("1", "ASAHI-1", "Done", None)]}),
            ),
        ])
        .await;
        let endpoint = server.endpoint.clone();
        let (_temp, workflow) = write_codex_asahi_workflow_with_max_turns(&endpoint, 5).await;
        let factory = FakeSessionFactory::new(vec![TurnExit::Completed]);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let (_comments_tx, comments_rx) = mpsc::channel(1);
        let (_stop_tx, stop_rx) = watch::channel(None);

        let outcome = run_agent_attempt_inner(
            test_issue(),
            None,
            workflow,
            events_tx,
            stop_rx,
            comments_rx,
            &factory,
        )
        .await
        .expect("agent attempt");

        assert!(matches!(outcome, WorkerOutcome::Normal));
        let state = factory.state();
        assert_eq!(state.starts, 1);
        assert_eq!(state.prompts.len(), 1);
        assert_eq!(state.shutdowns, 1);

        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].target, "/api/issues/1/activities");
        assert_eq!(requests[1].target, "/api/issues/1/comments");
        assert!(requests[2].target.starts_with("/api/issues?"));
        assert!(requests[2].target.contains("ids=1"));
    }

    #[tokio::test]
    async fn codex_agent_attempt_maps_session_turn_exits_and_shuts_down() {
        enum ExpectedOutcome {
            Failed,
            TimedOut,
            Stalled,
        }

        let cases = vec![
            (
                "failed",
                TurnExit::Failed("agent rejected task".to_string()),
                ExpectedOutcome::Failed,
            ),
            ("timeout", TurnExit::TimedOut, ExpectedOutcome::TimedOut),
            (
                "stalled",
                TurnExit::Stopped(StopReason::Stalled),
                ExpectedOutcome::Stalled,
            ),
        ];

        for (name, turn_exit, expected) in cases {
            let server = MockHttpServer::spawn(vec![
                MockResponse::json(200, json!({})),
                MockResponse::json(200, json!({"comments": []})),
            ])
            .await;
            let endpoint = server.endpoint.clone();
            let (_temp, workflow) = write_codex_asahi_workflow_with_max_turns(&endpoint, 2).await;
            let factory = FakeSessionFactory::new(vec![turn_exit]);
            let (events_tx, _events_rx) = mpsc::unbounded_channel();
            let (_comments_tx, comments_rx) = mpsc::channel(1);
            let (_stop_tx, stop_rx) = watch::channel(None);

            let outcome = run_agent_attempt_inner(
                test_issue(),
                None,
                workflow,
                events_tx,
                stop_rx,
                comments_rx,
                &factory,
            )
            .await
            .unwrap_or_else(|err| panic!("{name}: agent attempt failed: {err}"));

            match expected {
                ExpectedOutcome::Failed => match outcome {
                    WorkerOutcome::Failed(reason) => assert_eq!(reason, "agent rejected task"),
                    other => panic!("{name}: expected failed outcome, got {other:?}"),
                },
                ExpectedOutcome::TimedOut => {
                    assert!(matches!(outcome, WorkerOutcome::TimedOut), "{name}");
                }
                ExpectedOutcome::Stalled => {
                    assert!(matches!(outcome, WorkerOutcome::Stalled), "{name}");
                }
            }
            let state = factory.state();
            assert_eq!(state.starts, 1, "{name}");
            assert_eq!(state.prompts.len(), 1, "{name}");
            assert_eq!(state.shutdowns, 1, "{name}");

            let requests = server.recorded_requests().await;
            assert_eq!(requests.len(), 2, "{name}");
            assert_eq!(requests[0].target, "/api/issues/1/activities");
            assert_eq!(requests[1].target, "/api/issues/1/comments");
        }
    }

    #[tokio::test]
    async fn codex_agent_attempt_shuts_down_and_runs_after_run_on_session_errors() {
        #[derive(Clone, Copy)]
        enum ErrorCase {
            Start,
            Turn,
        }

        let cases = vec![
            (
                "start_error",
                ErrorCase::Start,
                1,
                0,
                "session refused start",
            ),
            ("turn_error", ErrorCase::Turn, 2, 1, "turn channel closed"),
        ];

        for (name, error_case, expected_requests, expected_prompts, expected_error) in cases {
            let mut responses = vec![MockResponse::json(200, json!({}))];
            if matches!(error_case, ErrorCase::Turn) {
                responses.push(MockResponse::json(200, json!({"comments": []})));
            }
            let server = MockHttpServer::spawn(responses).await;
            let endpoint = server.endpoint.clone();
            let (_temp, workflow) = write_codex_asahi_workflow_with_after_run(&endpoint).await;
            let factory = match error_case {
                ErrorCase::Start => FakeSessionFactory::with_start_error(expected_error),
                ErrorCase::Turn => FakeSessionFactory::with_turn_error(expected_error),
            };
            let (events_tx, _events_rx) = mpsc::unbounded_channel();
            let (_comments_tx, comments_rx) = mpsc::channel(1);
            let (_stop_tx, stop_rx) = watch::channel(None);

            let err = run_agent_attempt_inner(
                test_issue(),
                None,
                workflow,
                events_tx,
                stop_rx,
                comments_rx,
                &factory,
            )
            .await
            .unwrap_err();

            assert!(err.to_string().contains(expected_error), "{name}");
            let state = factory.state();
            assert_eq!(state.starts, 1, "{name}");
            assert_eq!(state.prompts.len(), expected_prompts, "{name}");
            assert_eq!(state.shutdowns, 1, "{name}");
            let workspace_path = state.build_workspace_path.expect("workspace path");
            let marker = tokio::fs::read_to_string(workspace_path.join("after_run.txt"))
                .await
                .unwrap_or_else(|err| panic!("{name}: missing after_run marker: {err}"));
            assert_eq!(marker, "after", "{name}");

            let requests = server.recorded_requests().await;
            assert_eq!(requests.len(), expected_requests, "{name}");
            assert_eq!(requests[0].target, "/api/issues/1/activities");
            if matches!(error_case, ErrorCase::Turn) {
                assert_eq!(requests[1].target, "/api/issues/1/comments");
            }
        }
    }

    #[tokio::test]
    async fn codex_agent_attempt_posts_asahi_activity_and_reports_launch_failure() {
        let server = MockHttpServer::spawn(vec![MockResponse::json(200, json!({}))]).await;
        let endpoint = server.endpoint.clone();
        let (_temp, workflow) = write_codex_asahi_workflow(&endpoint).await;
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let (_comments_tx, comments_rx) = mpsc::channel(1);
        let (_stop_tx, stop_rx) = watch::channel(None);

        run_agent_attempt(
            test_issue(),
            Some(2),
            workflow,
            events_tx,
            stop_rx,
            comments_rx,
        )
        .await;

        let event = events_rx.recv().await.expect("worker exit event");
        match event {
            WorkerEvent::Exited(exit) => {
                assert_eq!(exit.issue_identifier, "ASAHI-1");
                match exit.outcome {
                    WorkerOutcome::Failed(reason) => assert!(reason.contains("unterminated")),
                    other => panic!("expected failed launch, got {other:?}"),
                }
                assert!(exit.error.unwrap().contains("unterminated"));
            }
            other => panic!("expected worker exit, got {other:?}"),
        }
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].target, "/api/issues/1/activities");
        let body = serde_json::from_str::<Value>(&requests[0].body).expect("activity body");
        assert_eq!(body["kind"], "agent_started");
        assert_eq!(body["title"], "Agent started on ASAHI-1");
    }

    #[tokio::test]
    async fn codex_agent_attempt_continues_when_agent_started_activity_fails() {
        let server = MockHttpServer::spawn(vec![MockResponse::json(
            500,
            json!({"error": "activity failed"}),
        )])
        .await;
        let endpoint = server.endpoint.clone();
        let (_temp, workflow) = write_codex_asahi_workflow(&endpoint).await;
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let (_comments_tx, comments_rx) = mpsc::channel(1);
        let (_stop_tx, stop_rx) = watch::channel(None);

        run_agent_attempt(
            test_issue(),
            None,
            workflow,
            events_tx,
            stop_rx,
            comments_rx,
        )
        .await;

        let event = events_rx.recv().await.expect("worker exit event");
        match event {
            WorkerEvent::Exited(exit) => {
                assert_eq!(exit.issue_identifier, "ASAHI-1");
                match exit.outcome {
                    WorkerOutcome::Failed(reason) => assert!(reason.contains("unterminated")),
                    other => panic!("expected failed launch, got {other:?}"),
                }
            }
            other => panic!("expected worker exit, got {other:?}"),
        }
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target, "/api/issues/1/activities");
    }

    #[tokio::test]
    async fn codex_agent_attempt_stop_before_launch_reports_reconciliation_cancel() {
        let server = MockHttpServer::spawn(vec![MockResponse::json(200, json!({}))]).await;
        let endpoint = server.endpoint.clone();
        let (_temp, workflow) = write_codex_asahi_workflow(&endpoint).await;
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let (_comments_tx, comments_rx) = mpsc::channel(1);
        let (_stop_tx, stop_rx) = watch::channel(Some(StopReason::NonActive));

        run_agent_attempt(
            test_issue(),
            None,
            workflow,
            events_tx,
            stop_rx,
            comments_rx,
        )
        .await;

        let event = events_rx.recv().await.expect("worker exit event");
        match event {
            WorkerEvent::Exited(exit) => {
                assert_eq!(exit.issue_identifier, "ASAHI-1");
                assert!(matches!(
                    exit.outcome,
                    WorkerOutcome::CanceledByReconciliation
                ));
                assert_eq!(exit.error.as_deref(), Some("canceled_by_reconciliation"));
            }
            other => panic!("expected worker exit, got {other:?}"),
        }
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target, "/api/issues/1/activities");
    }

    #[tokio::test]
    async fn codex_agent_attempt_stop_before_launch_reports_stalled() {
        let server = MockHttpServer::spawn(vec![MockResponse::json(200, json!({}))]).await;
        let endpoint = server.endpoint.clone();
        let (_temp, workflow) = write_codex_asahi_workflow(&endpoint).await;
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let (_comments_tx, comments_rx) = mpsc::channel(1);
        let (_stop_tx, stop_rx) = watch::channel(Some(StopReason::Stalled));

        run_agent_attempt(
            test_issue(),
            None,
            workflow,
            events_tx,
            stop_rx,
            comments_rx,
        )
        .await;

        let event = events_rx.recv().await.expect("worker exit event");
        match event {
            WorkerEvent::Exited(exit) => {
                assert_eq!(exit.issue_identifier, "ASAHI-1");
                assert!(matches!(exit.outcome, WorkerOutcome::Stalled));
                assert_eq!(exit.error.as_deref(), Some("stalled"));
            }
            other => panic!("expected worker exit, got {other:?}"),
        }
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target, "/api/issues/1/activities");
    }
}
