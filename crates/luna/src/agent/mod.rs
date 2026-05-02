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
mod codex;

pub use acp::AcpSession;
pub use codex::CodexSession;

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

pub async fn run_agent_attempt(
    issue: Issue,
    attempt: Option<u32>,
    workflow: LoadedWorkflow,
    events: mpsc::UnboundedSender<WorkerEvent>,
    stop_rx: watch::Receiver<Option<StopReason>>,
    comment_rx: mpsc::Receiver<String>,
) {
    let started = Instant::now();
    let outcome =
        run_agent_attempt_inner(issue.clone(), attempt, workflow, events.clone(), stop_rx, comment_rx).await;

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

    let mut session = build_agent_session(
        &workflow.config.runner,
        &workspace.path,
        issue.id.clone(),
        issue.identifier.clone(),
        events.clone(),
    )
    .await?;

    let prompt = render_issue_prompt(&workflow.definition.prompt_template, &issue, attempt)?;
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
            build_continuation_prompt(&issue, turn_number, workflow.config.scheduler.max_turns, &new_comments)
        };

        match session.run_turn(&prompt, turn_number, &mut stop_rx).await? {
            TurnExit::Completed => {}
            TurnExit::Failed(reason) => {
                session.shutdown().await;
                workspace_manager.after_run_best_effort(&workspace).await;
                return Ok(WorkerOutcome::Failed(reason));
            }
            TurnExit::TimedOut => {
                session.shutdown().await;
                workspace_manager.after_run_best_effort(&workspace).await;
                return Ok(WorkerOutcome::TimedOut);
            }
            TurnExit::Stopped(reason) => {
                session.shutdown().await;
                workspace_manager.after_run_best_effort(&workspace).await;
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

    session.shutdown().await;
    workspace_manager.after_run_best_effort(&workspace).await;
    Ok(WorkerOutcome::Normal)
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
            CodexSession::launch(c, workspace_path, issue_id, issue_identifier, events).await?,
        )),
        RunnerConfig::Acp(c) => Ok(Box::new(
            AcpSession::launch(c, workspace_path, issue_id, issue_identifier, events).await?,
        )),
    }
}
