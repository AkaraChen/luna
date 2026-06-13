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

#[derive(Clone, Debug, Default)]
struct AngelWorkerEventState {
    thread_id: Option<String>,
    turn_id: Option<String>,
    session_id: Option<String>,
}

struct AngelEventContext<'a> {
    issue_id: &'a str,
    issue_identifier: &'a str,
    events: &'a mpsc::UnboundedSender<WorkerEvent>,
    workspace_path: &'a str,
}

enum ProjectedAngelEvent {
    Handled,
    ResolveElicitation(String),
}

struct AngelWorker {
    session: AngelSession,
    issue_id: String,
    issue_identifier: String,
    events: mpsc::UnboundedSender<WorkerEvent>,
    workspace_path: String,
    pending_comments: Vec<String>,
    event_state: AngelWorkerEventState,
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
            event_state: AngelWorkerEventState::default(),
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
        let projected = project_turn_event(
            event,
            turn_number,
            &mut self.event_state,
            AngelEventContext {
                issue_id: &self.issue_id,
                issue_identifier: &self.issue_identifier,
                events: &self.events,
                workspace_path: &self.workspace_path,
            },
        );
        match projected {
            ProjectedAngelEvent::Handled => {}
            ProjectedAngelEvent::ResolveElicitation(elicitation_id) => {
                let events = self
                    .session
                    .resolve_elicitation(
                        elicitation_id,
                        angel_engine_client::ElicitationResponse::AllowForSession,
                    )
                    .map_err(angel_error)?;
                for event in events {
                    self.handle_turn_event(event, turn_number)?;
                }
            }
        }
        Ok(())
    }
}

fn project_turn_event(
    event: TurnRunEvent,
    turn_number: u32,
    state: &mut AngelWorkerEventState,
    context: AngelEventContext<'_>,
) -> ProjectedAngelEvent {
    match event {
        TurnRunEvent::Delta { text, turn_id, .. } => {
            if let Some(turn_id) = turn_id {
                set_projected_turn_id(turn_id, turn_number, state, &context);
            }
            emit_projected_session_update(
                "item/agentMessage/delta",
                Some(truncate_message(text)),
                None,
                Some(turn_number),
                state,
                &context,
            );
            ProjectedAngelEvent::Handled
        }
        TurnRunEvent::ActionObserved { action, .. }
        | TurnRunEvent::ActionUpdated { action, .. } => {
            set_projected_turn_id(action.turn_id.clone(), turn_number, state, &context);
            if action.kind == "command" && action.phase == "completed" && action.error.is_none() {
                if let Some(command) = action.input_summary.or(action.raw_input) {
                    let _ =
                        context
                            .events
                            .send(WorkerEvent::CommandExecuted(CommandExecutionEvent {
                                issue_id: context.issue_id.to_string(),
                                issue_identifier: context.issue_identifier.to_string(),
                                command,
                                cwd: Some(context.workspace_path.to_string()),
                                duration_ms: None,
                                exit_code: None,
                            }));
                }
            }
            ProjectedAngelEvent::Handled
        }
        TurnRunEvent::ActionOutputDelta { turn_id, .. } => {
            set_projected_turn_id(turn_id, turn_number, state, &context);
            ProjectedAngelEvent::Handled
        }
        TurnRunEvent::Elicitation { elicitation, .. } => {
            set_projected_turn_id(
                elicitation.turn_id.unwrap_or_default(),
                turn_number,
                state,
                &context,
            );
            ProjectedAngelEvent::ResolveElicitation(elicitation.id)
        }
        TurnRunEvent::PlanUpdated { turn_id, .. } => {
            if let Some(turn_id) = turn_id {
                set_projected_turn_id(turn_id, turn_number, state, &context);
            }
            ProjectedAngelEvent::Handled
        }
        TurnRunEvent::Result { result } => {
            if let Some(remote_thread_id) = result.remote_thread_id.clone() {
                state.thread_id = Some(remote_thread_id);
            }
            if let Some(turn_id) = result.turn_id.clone() {
                set_projected_turn_id(turn_id, turn_number, state, &context);
            }
            if let Some(conversation) = result.conversation {
                state.thread_id = conversation.remote_id.or(Some(conversation.id));
                if let Some(usage) = conversation.usage {
                    emit_projected_session_update(
                        "thread/tokenUsage/updated",
                        None,
                        Some(UsageUpdate {
                            input_tokens: usage.used,
                            output_tokens: 0,
                            total_tokens: usage.used,
                        }),
                        Some(turn_number),
                        state,
                        &context,
                    );
                }
            }
            emit_projected_session_update(
                "turn/completed",
                None,
                None,
                Some(turn_number),
                state,
                &context,
            );
            ProjectedAngelEvent::Handled
        }
    }
}

fn set_projected_turn_id(
    turn_id: String,
    turn_number: u32,
    state: &mut AngelWorkerEventState,
    context: &AngelEventContext<'_>,
) {
    if turn_id.is_empty() {
        return;
    }
    state.turn_id = Some(turn_id.clone());
    if let Some(thread_id) = &state.thread_id {
        state.session_id = Some(format!("{thread_id}-{turn_id}"));
    }
    emit_projected_session_update(
        "turn/started",
        None,
        None,
        Some(turn_number),
        state,
        context,
    );
}

fn emit_projected_session_update(
    event: &str,
    message: Option<String>,
    usage: Option<UsageUpdate>,
    turn_count: Option<u32>,
    state: &AngelWorkerEventState,
    context: &AngelEventContext<'_>,
) {
    let _ = context.events.send(WorkerEvent::Session(SessionUpdate {
        issue_id: context.issue_id.to_string(),
        issue_identifier: context.issue_identifier.to_string(),
        event: event.to_string(),
        timestamp: Utc::now(),
        session_id: state.session_id.clone(),
        thread_id: state.thread_id.clone(),
        turn_id: state.turn_id.clone(),
        agent_pid: None,
        message,
        usage,
        rate_limits: None,
        turn_count,
    }));
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

#[cfg(test)]
mod tests {
    use angel_engine_client::TurnRunEvent;
    use serde_json::{Value, json};
    use tokio::sync::{mpsc, watch};

    use crate::{
        agent::{AgentSession, StopReason, TurnExit, WorkerEvent},
        config::CodexRunner,
    };

    use super::{
        AngelCommand, AngelEventContext, AngelRuntimeKind, AngelRuntimeLaunchConfig,
        AngelRuntimeSession, AngelWorkerEventState, ProjectedAngelEvent, project_turn_event,
        truncate_message,
    };

    #[test]
    fn codex_launch_config_uses_codex_runtime_defaults() {
        let config = CodexRunner {
            command: "codex app-server".to_string(),
            args: vec!["--experimental".to_string()],
            approval_policy: None,
            thread_sandbox: None,
            turn_sandbox_policy: None,
            turn_timeout_ms: 1234,
            read_timeout_ms: 5000,
            stall_timeout_ms: 300_000,
        };

        let launch = AngelRuntimeLaunchConfig::codex(&config);

        assert!(matches!(launch.kind, AngelRuntimeKind::Codex));
        assert_eq!(launch.kind.name(), "codex");
        assert_eq!(launch.command, "codex app-server");
        assert_eq!(launch.args, vec!["--experimental"]);
        assert_eq!(launch.turn_timeout_ms, 1234);
        assert_eq!(launch.default_permission_mode, "never");
        assert_eq!(launch.default_reasoning_effort, None);
    }

    fn codex_session_with_command_tx(
        command_tx: mpsc::UnboundedSender<AngelCommand>,
        events: mpsc::UnboundedSender<WorkerEvent>,
        turn_timeout_ms: u64,
    ) -> AngelRuntimeSession {
        AngelRuntimeSession {
            command_tx,
            worker: tokio::spawn(async {
                std::future::pending::<()>().await;
            }),
            issue_id: "issue-1".to_string(),
            issue_identifier: "ASAHI-1".to_string(),
            events,
            config: AngelRuntimeLaunchConfig {
                kind: AngelRuntimeKind::Codex,
                command: "codex app-server".to_string(),
                args: Vec::new(),
                turn_timeout_ms,
                default_reasoning_effort: None,
                default_permission_mode: "never".to_string(),
            },
            session_id: Some("session".to_string()),
            thread_id: Some("thread".to_string()),
            turn_id: Some("turn".to_string()),
        }
    }

    fn turn_event(value: Value) -> TurnRunEvent {
        serde_json::from_value(value).expect("turn run event")
    }

    fn message_part_json() -> Value {
        json!({"type": "text", "text": ""})
    }

    fn command_action_json(
        kind: &str,
        phase: &str,
        input_summary: Option<&str>,
        raw_input: Option<&str>,
        error: Option<Value>,
    ) -> Value {
        json!({
            "id": "action-1",
            "turnId": "turn-action",
            "elicitationId": null,
            "kind": kind,
            "phase": phase,
            "title": null,
            "inputSummary": input_summary,
            "rawInput": raw_input,
            "outputText": "",
            "output": [],
            "error": error
        })
    }

    fn conversation_json(id: &str, remote_id: Option<&str>, used_tokens: u64) -> Value {
        json!({
            "id": id,
            "remoteId": remote_id,
            "remoteKind": "known",
            "lifecycle": "active",
            "activeTurnIds": [],
            "focusedTurnId": null,
            "context": {
                "model": null,
                "mode": null,
                "permissionMode": null,
                "cwd": null,
                "additionalDirectories": [],
                "approvalPolicy": null,
                "sandbox": null,
                "permissionProfile": null,
                "raw": {}
            },
            "turns": [],
            "actions": [],
            "messages": [],
            "elicitations": [],
            "history": {
                "hydrated": false,
                "turnCount": 0,
                "replay": []
            },
            "agentState": {
                "currentMode": null,
                "currentPermissionMode": null
            },
            "settings": {
                "reasoningLevel": {
                    "currentLevel": null,
                    "availableLevels": [],
                    "availableOptions": [],
                    "source": "unknown",
                    "configOptionId": null,
                    "canSet": false
                },
                "modelList": {
                    "currentModelId": null,
                    "availableModels": [],
                    "configOptionId": null,
                    "canSet": false
                },
                "availableModes": {
                    "currentModeId": null,
                    "availableModes": [],
                    "configOptionId": null,
                    "canSet": false
                },
                "permissionModes": {
                    "currentModeId": null,
                    "availableModes": [],
                    "configOptionId": null,
                    "canSet": false
                }
            },
            "availableCommands": [],
            "usage": {
                "used": used_tokens,
                "size": 1000,
                "cost": null
            }
        })
    }

    fn project(
        event: TurnRunEvent,
        turn_number: u32,
        state: &mut AngelWorkerEventState,
        events: &mpsc::UnboundedSender<WorkerEvent>,
    ) -> ProjectedAngelEvent {
        project_turn_event(
            event,
            turn_number,
            state,
            AngelEventContext {
                issue_id: "issue-1",
                issue_identifier: "ASAHI-1",
                events,
                workspace_path: "/tmp/luna-workspace",
            },
        )
    }

    fn expect_session_event(
        events: &mut mpsc::UnboundedReceiver<WorkerEvent>,
        expected_event: &str,
    ) -> crate::agent::SessionUpdate {
        match events.try_recv().expect("worker event") {
            WorkerEvent::Session(update) => {
                assert_eq!(update.event, expected_event);
                update
            }
            other => panic!("expected session event, got {other:?}"),
        }
    }

    #[test]
    fn codex_worker_projection_emits_delta_command_usage_and_completion_events() {
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let mut state = AngelWorkerEventState::default();

        let projected = project(
            turn_event(json!({
                "type": "delta",
                "part": "text",
                "text": "hello",
                "turnId": "turn-1",
                "messagePart": message_part_json()
            })),
            4,
            &mut state,
            &events_tx,
        );
        assert!(matches!(projected, ProjectedAngelEvent::Handled));
        let started = expect_session_event(&mut events_rx, "turn/started");
        assert_eq!(started.issue_id, "issue-1");
        assert_eq!(started.issue_identifier, "ASAHI-1");
        assert_eq!(started.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(started.turn_count, Some(4));
        let delta = expect_session_event(&mut events_rx, "item/agentMessage/delta");
        assert_eq!(delta.message.as_deref(), Some("hello"));
        assert_eq!(delta.turn_id.as_deref(), Some("turn-1"));

        let projected = project(
            turn_event(json!({
                "type": "action_observed",
                "action": command_action_json(
                    "command",
                    "completed",
                    Some("gh pr create"),
                    Some("ignored raw input"),
                    None
                ),
                "messagePart": message_part_json()
            })),
            4,
            &mut state,
            &events_tx,
        );
        assert!(matches!(projected, ProjectedAngelEvent::Handled));
        let action_started = expect_session_event(&mut events_rx, "turn/started");
        assert_eq!(action_started.turn_id.as_deref(), Some("turn-action"));
        match events_rx.try_recv().expect("command event") {
            WorkerEvent::CommandExecuted(command) => {
                assert_eq!(command.issue_id, "issue-1");
                assert_eq!(command.issue_identifier, "ASAHI-1");
                assert_eq!(command.command, "gh pr create");
                assert_eq!(command.cwd.as_deref(), Some("/tmp/luna-workspace"));
                assert_eq!(command.duration_ms, None);
                assert_eq!(command.exit_code, None);
            }
            other => panic!("expected command event, got {other:?}"),
        }

        let projected = project(
            turn_event(json!({
                "type": "result",
                "result": {
                    "remoteThreadId": "thread-1",
                    "turnId": "turn-final",
                    "conversation": conversation_json("conversation-1", Some("thread-1"), 77)
                }
            })),
            4,
            &mut state,
            &events_tx,
        );
        assert!(matches!(projected, ProjectedAngelEvent::Handled));
        let result_started = expect_session_event(&mut events_rx, "turn/started");
        assert_eq!(result_started.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(
            result_started.session_id.as_deref(),
            Some("thread-1-turn-final")
        );
        assert_eq!(result_started.turn_id.as_deref(), Some("turn-final"));
        let usage = expect_session_event(&mut events_rx, "thread/tokenUsage/updated");
        let usage_update = usage.usage.expect("usage update");
        assert_eq!(usage_update.input_tokens, 77);
        assert_eq!(usage_update.output_tokens, 0);
        assert_eq!(usage_update.total_tokens, 77);
        assert_eq!(usage.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(usage.turn_id.as_deref(), Some("turn-final"));
        let completed = expect_session_event(&mut events_rx, "turn/completed");
        assert_eq!(completed.session_id.as_deref(), Some("thread-1-turn-final"));
        assert_eq!(completed.turn_count, Some(4));
        assert!(events_rx.try_recv().is_err());
    }

    #[test]
    fn codex_worker_projection_skips_noncompleted_or_failed_command_actions() {
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let mut state = AngelWorkerEventState::default();

        let projected = project(
            turn_event(json!({
                "type": "action_updated",
                "action": command_action_json(
                    "command",
                    "completed",
                    Some("gh pr create"),
                    None,
                    Some(json!({
                        "code": "failed",
                        "message": "command failed",
                        "recoverable": true
                    }))
                ),
                "messagePart": message_part_json()
            })),
            1,
            &mut state,
            &events_tx,
        );
        assert!(matches!(projected, ProjectedAngelEvent::Handled));
        let started = expect_session_event(&mut events_rx, "turn/started");
        assert_eq!(started.turn_id.as_deref(), Some("turn-action"));
        assert!(events_rx.try_recv().is_err());

        let projected = project(
            turn_event(json!({
                "type": "action_updated",
                "action": command_action_json("read", "completed", Some("cat README.md"), None, None),
                "messagePart": message_part_json()
            })),
            1,
            &mut state,
            &events_tx,
        );
        assert!(matches!(projected, ProjectedAngelEvent::Handled));
        let started = expect_session_event(&mut events_rx, "turn/started");
        assert_eq!(started.turn_id.as_deref(), Some("turn-action"));
        assert!(events_rx.try_recv().is_err());
    }

    #[test]
    fn codex_worker_projection_tracks_action_output_and_plan_turn_identity() {
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let mut state = AngelWorkerEventState {
            thread_id: Some("thread-1".to_string()),
            ..AngelWorkerEventState::default()
        };

        let projected = project(
            turn_event(json!({
                "type": "action_output_delta",
                "turnId": "turn-output",
                "actionId": "action-1",
                "content": {
                    "kind": "text",
                    "text": "stdout"
                },
                "messagePart": message_part_json()
            })),
            3,
            &mut state,
            &events_tx,
        );
        assert!(matches!(projected, ProjectedAngelEvent::Handled));
        let started = expect_session_event(&mut events_rx, "turn/started");
        assert_eq!(started.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(started.session_id.as_deref(), Some("thread-1-turn-output"));
        assert_eq!(started.turn_id.as_deref(), Some("turn-output"));
        assert!(events_rx.try_recv().is_err());

        let projected = project(
            turn_event(json!({
                "type": "plan_updated",
                "turnId": "turn-plan",
                "plan": {
                    "entries": [],
                    "text": "- [ ] run CI"
                },
                "messagePart": message_part_json()
            })),
            3,
            &mut state,
            &events_tx,
        );
        assert!(matches!(projected, ProjectedAngelEvent::Handled));
        let started = expect_session_event(&mut events_rx, "turn/started");
        assert_eq!(started.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(started.session_id.as_deref(), Some("thread-1-turn-plan"));
        assert_eq!(started.turn_id.as_deref(), Some("turn-plan"));
        assert!(events_rx.try_recv().is_err());

        let projected = project(
            turn_event(json!({
                "type": "plan_updated",
                "plan": {
                    "entries": [],
                    "text": "- [x] run CI"
                },
                "messagePart": message_part_json()
            })),
            3,
            &mut state,
            &events_tx,
        );
        assert!(matches!(projected, ProjectedAngelEvent::Handled));
        assert!(events_rx.try_recv().is_err());
    }

    #[test]
    fn codex_worker_projection_returns_elicitation_resolution_request() {
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let mut state = AngelWorkerEventState {
            thread_id: Some("thread-1".to_string()),
            ..AngelWorkerEventState::default()
        };

        let projected = project(
            turn_event(json!({
                "type": "elicitation",
                "elicitation": {
                    "id": "approval-1",
                    "turnId": "turn-approval",
                    "actionId": null,
                    "kind": "approval",
                    "phase": "open",
                    "title": null,
                    "body": null,
                    "choices": [],
                    "questions": []
                },
                "messagePart": message_part_json()
            })),
            2,
            &mut state,
            &events_tx,
        );

        match projected {
            ProjectedAngelEvent::ResolveElicitation(id) => assert_eq!(id, "approval-1"),
            ProjectedAngelEvent::Handled => panic!("expected elicitation resolution request"),
        }
        let started = expect_session_event(&mut events_rx, "turn/started");
        assert_eq!(started.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(
            started.session_id.as_deref(),
            Some("thread-1-turn-approval")
        );
        assert_eq!(started.turn_id.as_deref(), Some("turn-approval"));
        assert!(events_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn codex_session_methods_send_commands_and_emit_start_event() {
        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let mut session = codex_session_with_command_tx(command_tx, events_tx, 1_000);

        let responder = tokio::spawn(async move {
            match command_rx.recv().await.expect("start command") {
                AngelCommand::Start { respond } => {
                    respond.send(Ok(())).expect("start response");
                }
                _ => panic!("expected start command"),
            }
            match command_rx.recv().await.expect("comment command") {
                AngelCommand::SendComment { body, respond } => {
                    assert_eq!(body, "ship it");
                    respond.send(Ok(())).expect("comment response");
                }
                _ => panic!("expected comment command"),
            }
            match command_rx.recv().await.expect("run turn command") {
                AngelCommand::RunTurn {
                    prompt,
                    turn_number,
                    respond,
                } => {
                    assert_eq!(prompt, "continue");
                    assert_eq!(turn_number, 2);
                    assert!(respond.send(Ok(TurnExit::Completed)).is_ok());
                }
                _ => panic!("expected run turn command"),
            }
            match command_rx.recv().await.expect("shutdown command") {
                AngelCommand::Shutdown => {}
                _ => panic!("expected shutdown command"),
            }
        });

        session.start().await.expect("start");
        match events_rx.recv().await.expect("session event") {
            WorkerEvent::Session(update) => {
                assert_eq!(update.issue_id, "issue-1");
                assert_eq!(update.issue_identifier, "ASAHI-1");
                assert_eq!(update.event, "session_started");
                assert_eq!(update.session_id.as_deref(), Some("session"));
                assert_eq!(update.thread_id.as_deref(), Some("thread"));
                assert_eq!(update.turn_id.as_deref(), Some("turn"));
                assert_eq!(update.turn_count, Some(0));
            }
            _ => panic!("expected session event"),
        }

        session.send_comment("ship it").await.expect("send comment");
        let (_stop_tx, mut stop_rx) = watch::channel(None);
        let turn = session
            .run_turn("continue", 2, &mut stop_rx)
            .await
            .expect("run turn");
        assert!(matches!(turn, TurnExit::Completed));

        session.shutdown().await;
        responder.await.expect("responder");
    }

    #[tokio::test]
    async fn codex_session_run_turn_cancels_on_stop_and_timeout() {
        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let mut session = codex_session_with_command_tx(command_tx, events_tx, 1_000);
        let (stop_tx, mut stop_rx) = watch::channel(None);
        stop_tx.send(Some(StopReason::NonActive)).unwrap();

        let turn = session
            .run_turn("stop", 1, &mut stop_rx)
            .await
            .expect("run turn");

        assert!(matches!(turn, TurnExit::Stopped(StopReason::NonActive)));
        match command_rx.recv().await.expect("run turn command") {
            AngelCommand::RunTurn {
                prompt,
                turn_number,
                ..
            } => {
                assert_eq!(prompt, "stop");
                assert_eq!(turn_number, 1);
            }
            _ => panic!("expected run turn command"),
        }
        match command_rx.recv().await.expect("cancel command") {
            AngelCommand::Cancel => {}
            _ => panic!("expected cancel command"),
        }
        session.worker.abort();

        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let mut session = codex_session_with_command_tx(command_tx, events_tx, 1);
        let (_stop_tx, mut stop_rx) = watch::channel(None);

        let turn = session
            .run_turn("timeout", 3, &mut stop_rx)
            .await
            .expect("run turn");

        assert!(matches!(turn, TurnExit::TimedOut));
        match command_rx.recv().await.expect("run turn command") {
            AngelCommand::RunTurn {
                prompt,
                turn_number,
                ..
            } => {
                assert_eq!(prompt, "timeout");
                assert_eq!(turn_number, 3);
            }
            _ => panic!("expected run turn command"),
        }
        match command_rx.recv().await.expect("cancel command") {
            AngelCommand::Cancel => {}
            _ => panic!("expected cancel command"),
        }
        session.worker.abort();
    }

    #[tokio::test]
    async fn codex_session_methods_report_closed_worker_channel() {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        drop(command_rx);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let mut session = codex_session_with_command_tx(command_tx, events_tx, 1_000);

        let err = session.start().await.expect_err("closed start channel");
        assert!(err.to_string().contains("angel worker channel closed"));
        let err = session
            .send_comment("comment")
            .await
            .expect_err("closed comment channel");
        assert!(err.to_string().contains("angel worker channel closed"));
        let (_stop_tx, mut stop_rx) = watch::channel(None);
        let err = match session.run_turn("prompt", 1, &mut stop_rx).await {
            Ok(_) => panic!("expected closed turn channel error"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("angel worker channel closed"));
        session.worker.abort();
    }

    #[test]
    fn truncate_message_keeps_short_messages_and_shortens_long_ones() {
        assert_eq!(truncate_message("short".to_string()), "short");

        let long = "好".repeat(513);
        let truncated = truncate_message(long);

        assert_eq!(truncated.chars().count(), 515);
        assert!(truncated.ends_with("..."));
    }
}
