use std::{path::Path, time::Duration};

use angel_engine_client::{
    AngelSession, RuntimeOptionsOverrides, SendTextRequest, TurnRunEvent, create_runtime_options,
};
use chrono::Utc;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tracing::warn;

use crate::{
    agent::{CommandExecutionEvent, SessionUpdate, StopReason, TurnExit, UsageUpdate, WorkerEvent},
    config::{CodexRunner, OpencodeRunner},
    error::{LunaError, Result},
    paths::absolutize_path,
};

use super::command_line::split_command;

#[derive(Clone, Copy, Debug)]
enum AngelRuntimeKind {
    Codex,
    Opencode,
}

impl AngelRuntimeKind {
    fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }
}

#[derive(Clone, Debug)]
struct AngelRuntimeLaunchConfig {
    kind: AngelRuntimeKind,
    command: String,
    args: Vec<String>,
    turn_timeout_ms: u64,
    default_reasoning_effort: Option<String>,
    default_permission_mode: String,
}

impl AngelRuntimeLaunchConfig {
    fn codex(config: &CodexRunner) -> Self {
        Self {
            kind: AngelRuntimeKind::Codex,
            command: config.command.clone(),
            args: config.args.clone(),
            turn_timeout_ms: config.turn_timeout_ms,
            default_reasoning_effort: None,
            default_permission_mode: "never".to_string(),
        }
    }

    fn opencode(config: &OpencodeRunner) -> Self {
        Self {
            kind: AngelRuntimeKind::Opencode,
            command: config.command.clone(),
            args: config.args.clone(),
            turn_timeout_ms: config.turn_timeout_ms,
            default_reasoning_effort: None,
            default_permission_mode: "bypassPermissions".to_string(),
        }
    }
}

pub struct AngelRuntimeSession {
    command_tx: mpsc::UnboundedSender<AngelCommand>,
    worker: JoinHandle<()>,
    issue_id: String,
    issue_identifier: String,
    events: mpsc::UnboundedSender<WorkerEvent>,
    config: AngelRuntimeLaunchConfig,
    session_id: Option<String>,
    thread_id: Option<String>,
    turn_id: Option<String>,
}

enum AngelCommand {
    Start {
        respond: oneshot::Sender<Result<()>>,
    },
    RunTurn {
        prompt: String,
        turn_number: u32,
        respond: oneshot::Sender<Result<TurnExit>>,
    },
    SendComment {
        body: String,
        respond: oneshot::Sender<Result<()>>,
    },
    Cancel,
    Shutdown,
}

struct AngelWorker {
    session: AngelSession,
    issue_id: String,
    issue_identifier: String,
    events: mpsc::UnboundedSender<WorkerEvent>,
    workspace_path: String,
    pending_comments: Vec<String>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    session_id: Option<String>,
    default_permission_mode: String,
}

impl AngelRuntimeSession {
    pub async fn launch(
        config: &CodexRunner,
        workspace_path: &Path,
        issue_id: String,
        issue_identifier: String,
        events: mpsc::UnboundedSender<WorkerEvent>,
    ) -> Result<Self> {
        Self::launch_codex(config, workspace_path, issue_id, issue_identifier, events).await
    }

    pub async fn launch_codex(
        config: &CodexRunner,
        workspace_path: &Path,
        issue_id: String,
        issue_identifier: String,
        events: mpsc::UnboundedSender<WorkerEvent>,
    ) -> Result<Self> {
        Self::launch_runtime(
            AngelRuntimeLaunchConfig::codex(config),
            workspace_path,
            issue_id,
            issue_identifier,
            events,
        )
        .await
    }

    pub async fn launch_opencode(
        config: &OpencodeRunner,
        workspace_path: &Path,
        issue_id: String,
        issue_identifier: String,
        events: mpsc::UnboundedSender<WorkerEvent>,
    ) -> Result<Self> {
        Self::launch_runtime(
            AngelRuntimeLaunchConfig::opencode(config),
            workspace_path,
            issue_id,
            issue_identifier,
            events,
        )
        .await
    }

    async fn launch_runtime(
        config: AngelRuntimeLaunchConfig,
        workspace_path: &Path,
        issue_id: String,
        issue_identifier: String,
        events: mpsc::UnboundedSender<WorkerEvent>,
    ) -> Result<Self> {
        let workspace_path =
            std::fs::canonicalize(workspace_path).or_else(|_| absolutize_path(workspace_path))?;
        let workspace_path = workspace_path.to_string_lossy().to_string();
        let (command, args) = split_command(&config.command, &config.args)?;
        let options = create_runtime_options(
            Some(config.kind.name()),
            RuntimeOptionsOverrides {
                command: Some(command),
                args: Some(args),
                cwd: Some(workspace_path.clone()),
                process_label: Some(format!("luna:{}:{}", config.kind.name(), issue_identifier)),
                client_name: Some("luna".to_string()),
                client_title: Some("Luna".to_string()),
                default_reasoning_effort: config.default_reasoning_effort.clone(),
                ..RuntimeOptionsOverrides::default()
            },
        );
        let session = tokio::task::spawn_blocking(move || AngelSession::new(options))
            .await
            .map_err(|e| LunaError::Agent(format!("angel session spawn task failed: {e}")))?
            .map_err(angel_error)?;

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let worker = AngelWorker {
            session,
            issue_id: issue_id.clone(),
            issue_identifier: issue_identifier.clone(),
            events: events.clone(),
            workspace_path: workspace_path.clone(),
            pending_comments: Vec::new(),
            thread_id: None,
            turn_id: None,
            session_id: None,
            default_permission_mode: config.default_permission_mode.clone(),
        };
        let worker = tokio::task::spawn_blocking(move || worker.run(command_rx));

        Ok(Self {
            command_tx,
            worker,
            issue_id,
            issue_identifier,
            events,
            config,
            session_id: None,
            thread_id: None,
            turn_id: None,
        })
    }

    fn emit(
        &self,
        event: &str,
        message: Option<String>,
        usage: Option<UsageUpdate>,
        turn_count: Option<u32>,
    ) {
        let _ = self.events.send(WorkerEvent::Session(SessionUpdate {
            issue_id: self.issue_id.clone(),
            issue_identifier: self.issue_identifier.clone(),
            event: event.to_string(),
            timestamp: Utc::now(),
            session_id: self.session_id.clone(),
            thread_id: self.thread_id.clone(),
            turn_id: self.turn_id.clone(),
            agent_pid: None,
            message,
            usage,
            rate_limits: None,
            turn_count,
        }));
    }
}

impl AngelWorker {
    fn run(mut self, mut command_rx: mpsc::UnboundedReceiver<AngelCommand>) {
        while let Some(command) = command_rx.blocking_recv() {
            match command {
                AngelCommand::Start { respond } => {
                    let result = self.start();
                    let _ = respond.send(result);
                }
                AngelCommand::RunTurn {
                    prompt,
                    turn_number,
                    respond,
                } => {
                    let result = self.run_turn(prompt, turn_number);
                    let _ = respond.send(result);
                }
                AngelCommand::SendComment { body, respond } => {
                    self.pending_comments.push(body);
                    let _ = respond.send(Ok(()));
                }
                AngelCommand::Cancel => {
                    if let Err(err) = self.session.cancel_turn() {
                        warn!(issue_id = %self.issue_id, error = %err, "failed to cancel angel turn");
                    }
                }
                AngelCommand::Shutdown => break,
            }
        }
        self.session.close();
    }

    fn start(&mut self) -> Result<()> {
        let request = SendTextRequest {
            text: "Initialize this Luna workspace. Do not make changes yet.".to_string(),
            cwd: Some(self.workspace_path.clone()),
            permission_mode: Some(self.default_permission_mode.clone()),
            ..SendTextRequest::default()
        };
        let events = self.session.start_text_turn(request).map_err(angel_error)?;
        for event in events {
            self.handle_turn_event(event, 0)?;
        }
        self.drain_until_result(0).map(|_| ())
    }

    fn run_turn(&mut self, mut prompt: String, turn_number: u32) -> Result<TurnExit> {
        if !self.pending_comments.is_empty() {
            let comments = std::mem::take(&mut self.pending_comments);
            let comments_text = comments
                .iter()
                .map(|c| format!("- {}", c.trim()))
                .collect::<Vec<_>>()
                .join("\n");
            prompt = format!("{prompt}\n\nNew comments on this issue:\n{comments_text}");
        }

        let request = SendTextRequest {
            text: prompt,
            cwd: Some(self.workspace_path.clone()),
            permission_mode: Some(self.default_permission_mode.clone()),
            ..SendTextRequest::default()
        };
        let events = self.session.start_text_turn(request).map_err(angel_error)?;
        for event in events {
            self.handle_turn_event(event, turn_number)?;
        }
        self.drain_until_result(turn_number)
    }

    fn drain_until_result(&mut self, turn_number: u32) -> Result<TurnExit> {
        loop {
            match self
                .session
                .next_turn_event(Duration::from_millis(250))
                .map_err(angel_error)?
            {
                Some(event) => {
                    if matches!(event, TurnRunEvent::Result { .. }) {
                        self.handle_turn_event(event, turn_number)?;
                        return Ok(TurnExit::Completed);
                    }
                    self.handle_turn_event(event, turn_number)?;
                }
                None => continue,
            }
        }
    }

    fn handle_turn_event(&mut self, event: TurnRunEvent, turn_number: u32) -> Result<()> {
        match event {
            TurnRunEvent::Delta { text, turn_id, .. } => {
                if let Some(turn_id) = turn_id {
                    self.set_turn_id(turn_id, turn_number);
                }
                self.emit(
                    "item/agentMessage/delta",
                    Some(truncate_message(text)),
                    None,
                    Some(turn_number),
                );
            }
            TurnRunEvent::ActionObserved { action, .. }
            | TurnRunEvent::ActionUpdated { action, .. } => {
                self.set_turn_id(action.turn_id.clone(), turn_number);
                if action.kind == "command" && action.phase == "completed" && action.error.is_none()
                {
                    if let Some(command) = action.input_summary.or(action.raw_input) {
                        let _ =
                            self.events
                                .send(WorkerEvent::CommandExecuted(CommandExecutionEvent {
                                    issue_id: self.issue_id.clone(),
                                    issue_identifier: self.issue_identifier.clone(),
                                    command,
                                    cwd: Some(self.workspace_path.clone()),
                                    duration_ms: None,
                                    exit_code: None,
                                }));
                    }
                }
            }
            TurnRunEvent::ActionOutputDelta { turn_id, .. } => {
                self.set_turn_id(turn_id, turn_number);
            }
            TurnRunEvent::Elicitation { elicitation, .. } => {
                self.set_turn_id(elicitation.turn_id.unwrap_or_default(), turn_number);
                let events = self
                    .session
                    .resolve_elicitation(
                        elicitation.id,
                        angel_engine_client::ElicitationResponse::AllowForSession,
                    )
                    .map_err(angel_error)?;
                for event in events {
                    self.handle_turn_event(event, turn_number)?;
                }
            }
            TurnRunEvent::PlanUpdated { turn_id, .. } => {
                if let Some(turn_id) = turn_id {
                    self.set_turn_id(turn_id, turn_number);
                }
            }
            TurnRunEvent::Result { result } => {
                if let Some(remote_thread_id) = result.remote_thread_id.clone() {
                    self.thread_id = Some(remote_thread_id);
                }
                if let Some(turn_id) = result.turn_id.clone() {
                    self.set_turn_id(turn_id, turn_number);
                }
                if let Some(conversation) = result.conversation {
                    self.thread_id = conversation.remote_id.or(Some(conversation.id));
                    if let Some(usage) = conversation.usage {
                        self.emit(
                            "thread/tokenUsage/updated",
                            None,
                            Some(UsageUpdate {
                                input_tokens: usage.used,
                                output_tokens: 0,
                                total_tokens: usage.used,
                            }),
                            Some(turn_number),
                        );
                    }
                }
                self.emit("turn/completed", None, None, Some(turn_number));
            }
        }
        Ok(())
    }

    fn set_turn_id(&mut self, turn_id: String, turn_number: u32) {
        if turn_id.is_empty() {
            return;
        }
        self.turn_id = Some(turn_id.clone());
        if let Some(thread_id) = &self.thread_id {
            self.session_id = Some(format!("{thread_id}-{turn_id}"));
        }
        self.emit("turn/started", None, None, Some(turn_number));
    }

    fn emit(
        &self,
        event: &str,
        message: Option<String>,
        usage: Option<UsageUpdate>,
        turn_count: Option<u32>,
    ) {
        let _ = self.events.send(WorkerEvent::Session(SessionUpdate {
            issue_id: self.issue_id.clone(),
            issue_identifier: self.issue_identifier.clone(),
            event: event.to_string(),
            timestamp: Utc::now(),
            session_id: self.session_id.clone(),
            thread_id: self.thread_id.clone(),
            turn_id: self.turn_id.clone(),
            agent_pid: None,
            message,
            usage,
            rate_limits: None,
            turn_count,
        }));
    }
}

#[async_trait::async_trait]
impl crate::agent::AgentSession for AngelRuntimeSession {
    async fn start(&mut self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(AngelCommand::Start { respond: tx })
            .map_err(|_| LunaError::Agent("angel worker channel closed".to_string()))?;
        rx.await
            .map_err(|_| LunaError::Agent("angel start response channel closed".to_string()))??;
        self.emit("session_started", None, None, Some(0));
        Ok(())
    }

    async fn run_turn(
        &mut self,
        prompt: &str,
        turn_number: u32,
        stop_rx: &mut tokio::sync::watch::Receiver<Option<StopReason>>,
    ) -> Result<TurnExit> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(AngelCommand::RunTurn {
                prompt: prompt.to_string(),
                turn_number,
                respond: tx,
            })
            .map_err(|_| LunaError::Agent("angel worker channel closed".to_string()))?;

        tokio::select! {
            result = rx => {
                result.map_err(|_| LunaError::Agent("angel turn response channel closed".to_string()))?
            }
            changed = stop_rx.changed() => {
                if changed.is_ok() {
                    let reason = stop_rx.borrow().clone().unwrap_or(StopReason::Shutdown);
                    let _ = self.command_tx.send(AngelCommand::Cancel);
                    return Ok(TurnExit::Stopped(reason));
                }
                Ok(TurnExit::Stopped(StopReason::Shutdown))
            }
            _ = tokio::time::sleep(Duration::from_millis(self.config.turn_timeout_ms)) => {
                let _ = self.command_tx.send(AngelCommand::Cancel);
                Ok(TurnExit::TimedOut)
            }
        }
    }

    async fn send_comment(&mut self, body: &str) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(AngelCommand::SendComment {
                body: body.to_string(),
                respond: tx,
            })
            .map_err(|_| LunaError::Agent("angel worker channel closed".to_string()))?;
        rx.await
            .map_err(|_| LunaError::Agent("angel comment response channel closed".to_string()))?
    }

    async fn shutdown(&mut self) {
        let _ = self.command_tx.send(AngelCommand::Shutdown);
        self.worker.abort();
    }
}

fn angel_error(error: angel_engine_client::ClientError) -> LunaError {
    LunaError::Agent(format!("angel-engine client error: {error}"))
}

fn truncate_message(message: String) -> String {
    const MAX: usize = 512;
    if message.chars().count() <= MAX {
        return message;
    }
    let mut truncated = message.chars().take(MAX).collect::<String>();
    truncated.push_str("...");
    truncated
}
