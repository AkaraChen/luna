use std::{path::Path, process::Stdio, time::Instant};

use chrono::Utc;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Duration, sleep_until},
};
use tracing::{debug, warn};

use crate::{
    config::CodexConfig,
    error::{LunaError, Result},
    model::Issue,
    prompt::{build_continuation_prompt, render_issue_prompt},
    tracker::build_tracker,
    workflow::LoadedWorkflow,
    workspace::WorkspaceManager,
};

const MAX_JSON_LINE_BYTES: usize = 10 * 1024 * 1024;

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
    pub codex_app_server_pid: Option<u32>,
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
pub enum WorkerEvent {
    Session(SessionUpdate),
    Exited(WorkerExit),
    RetryDue(String),
}

pub async fn run_agent_attempt(
    issue: Issue,
    attempt: Option<u32>,
    workflow: LoadedWorkflow,
    events: mpsc::UnboundedSender<WorkerEvent>,
    stop_rx: watch::Receiver<Option<StopReason>>,
) {
    let started = Instant::now();
    let outcome =
        run_agent_attempt_inner(issue.clone(), attempt, workflow, events.clone(), stop_rx).await;

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
) -> Result<WorkerOutcome> {
    let workspace_manager = WorkspaceManager::new(
        workflow.config.workspace.root.clone(),
        workflow.config.hooks.clone(),
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

    let tracker = build_tracker(&workflow.config.tracker)?;
    let mut session = AppServerSession::launch(
        &workflow.config.codex,
        &workspace.path,
        issue.id.clone(),
        issue.identifier.clone(),
        events.clone(),
    )
    .await?;

    let prompt = render_issue_prompt(&workflow.definition.prompt_template, &issue, attempt)?;
    session.start_thread(&workflow.config.codex).await?;

    let mut turn_number = 1_u32;
    loop {
        let prompt = if turn_number == 1 {
            prompt.clone()
        } else {
            build_continuation_prompt(&issue, turn_number, workflow.config.agent.max_turns)
        };

        match session
            .run_turn(prompt, turn_number, &workflow.config.codex, &mut stop_rx)
            .await?
        {
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

        let refreshed = tracker.fetch_issue_states_by_ids(&[issue.id.clone()]).await?;
        issue = refreshed.into_iter().next().ok_or_else(|| {
            LunaError::Tracker("issue state refresh error: issue missing after turn".to_string())
        })?;

        if !workflow.config.tracker.is_active_state(&issue.state) {
            break;
        }
        if turn_number >= workflow.config.agent.max_turns {
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

enum TurnExit {
    Completed,
    Failed(String),
    TimedOut,
    Stopped(StopReason),
}

struct AppServerSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr_task: JoinHandle<()>,
    next_request_id: u64,
    issue_id: String,
    issue_identifier: String,
    events: mpsc::UnboundedSender<WorkerEvent>,
    pid: Option<u32>,
    workspace_path: String,
    thread_id: Option<String>,
    turn_id: Option<String>,
    session_id: Option<String>,
    turn_terminal_status: Option<String>,
}

impl AppServerSession {
    async fn launch(
        config: &CodexConfig,
        workspace_path: &Path,
        issue_id: String,
        issue_identifier: String,
        events: mpsc::UnboundedSender<WorkerEvent>,
    ) -> Result<Self> {
        let mut child = Command::new("bash")
            .arg("-lc")
            .arg(&config.command)
            .current_dir(workspace_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let pid = child.id();
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LunaError::Agent("failed to capture codex stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LunaError::Agent("failed to capture codex stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| LunaError::Agent("failed to capture codex stderr".to_string()))?;

        let stderr_task = tokio::spawn(log_stderr(
            stderr,
            issue_id.clone(),
            issue_identifier.clone(),
        ));
        let mut session = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stderr_task,
            next_request_id: 1,
            issue_id,
            issue_identifier,
            events,
            pid,
            workspace_path: workspace_path.to_string_lossy().to_string(),
            thread_id: None,
            turn_id: None,
            session_id: None,
            turn_terminal_status: None,
        };

        session.initialize(config.read_timeout_ms).await?;
        Ok(session)
    }

    async fn initialize(&mut self, read_timeout_ms: u64) -> Result<()> {
        let params = json!({
            "clientInfo": {
                "name": "luna",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "experimentalApi": true,
            }
        });
        self.request("initialize", params, read_timeout_ms).await?;
        Ok(())
    }

    async fn start_thread(&mut self, config: &CodexConfig) -> Result<()> {
        let response = self
            .request(
                "thread/start",
                json!({
                    "cwd": self.workspace_path.clone(),
                    "serviceName": "luna",
                    "sessionStartSource": "startup",
                    "approvalPolicy": config.approval_policy.clone(),
                    "sandbox": config.thread_sandbox.clone(),
                }),
                config.read_timeout_ms,
            )
            .await?;
        self.thread_id = extract_string(&response, &["/thread/id"]);
        if self.thread_id.is_none() {
            return Err(LunaError::Agent(
                "thread/start did not return thread.id".to_string(),
            ));
        }
        Ok(())
    }

    async fn run_turn(
        &mut self,
        prompt: String,
        turn_number: u32,
        config: &CodexConfig,
        stop_rx: &mut watch::Receiver<Option<StopReason>>,
    ) -> Result<TurnExit> {
        self.turn_terminal_status = None;
        let response = self
            .request(
                "turn/start",
                json!({
                    "threadId": self.thread_id.clone().ok_or_else(|| LunaError::Agent("missing thread id".to_string()))?,
                    "cwd": self.workspace_path.clone(),
                    "approvalPolicy": config.approval_policy.clone(),
                    "sandboxPolicy": config.turn_sandbox_policy.clone(),
                    "input": [
                        {
                            "type": "text",
                            "text": prompt,
                        }
                    ]
                }),
                config.read_timeout_ms,
            )
            .await?;

        self.turn_id = extract_string(&response, &["/turn/id"]);
        if let (Some(thread_id), Some(turn_id)) = (&self.thread_id, &self.turn_id) {
            self.session_id = Some(format!("{thread_id}-{turn_id}"));
            self.emit("session_started", None, None, Some(turn_number));
        }

        let deadline = tokio::time::Instant::now() + Duration::from_millis(config.turn_timeout_ms);
        loop {
            if let Some(status) = self.turn_terminal_status.take() {
                return Ok(match status.as_str() {
                    "completed" => TurnExit::Completed,
                    "interrupted" => TurnExit::Failed("turn interrupted".to_string()),
                    "failed" => TurnExit::Failed("turn failed".to_string()),
                    other => TurnExit::Failed(format!("unexpected turn status: {other}")),
                });
            }

            let stop_deadline = sleep_until(deadline);
            tokio::pin!(stop_deadline);

            tokio::select! {
                changed = stop_rx.changed() => {
                    if changed.is_ok() {
                        let reason = stop_rx.borrow().clone().unwrap_or(StopReason::Shutdown);
                        self.kill_process().await;
                        return Ok(TurnExit::Stopped(reason));
                    }
                }
                _ = &mut stop_deadline => {
                    self.kill_process().await;
                    return Ok(TurnExit::TimedOut);
                }
                message = self.read_message(None) => {
                    let message = message?;
                    if message.get("method").is_none() && message.get("id").is_some() {
                        if let Some(response_id) = message.get("id") {
                            debug!(issue_id = %self.issue_id, response_id = %response_id, "ignoring unexpected in-flight response");
                        }
                        continue;
                    }

                    if let Some(method) = message.get("method").and_then(Value::as_str) {
                        if let Some(id) = message.get("id").cloned() {
                            let fatal = self
                                .handle_server_request(
                                    method,
                                    id,
                                    message.get("params").cloned().unwrap_or(Value::Null),
                                )
                                .await?;
                            if let Some(err) = fatal {
                                return Ok(TurnExit::Failed(err));
                            }
                            continue;
                        }
                        self.handle_notification(
                            method,
                            message.get("params").cloned().unwrap_or(Value::Null),
                            turn_number,
                        )
                        .await?;
                    }
                }
            }
        }
    }

    async fn request(
        &mut self,
        method: &str,
        params: Value,
        read_timeout_ms: u64,
    ) -> Result<Value> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await?;

        loop {
            let message = self
                .read_message(Some(Duration::from_millis(read_timeout_ms)))
                .await?;

            if let Some(response_id) = message.get("id").and_then(Value::as_u64) {
                if response_id == id {
                    if let Some(error) = message.get("error") {
                        return Err(LunaError::Agent(format!(
                            "response_error for {method}: {}",
                            error
                        )));
                    }
                    return Ok(message.get("result").cloned().unwrap_or(Value::Null));
                }
            }

            if message.get("method").and_then(Value::as_str).is_some()
                && message.get("id").is_some()
            {
                let method = message
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let fatal = self
                    .handle_server_request(
                        method,
                        message.get("id").cloned().unwrap_or(Value::Null),
                        message.get("params").cloned().unwrap_or(Value::Null),
                    )
                    .await?;
                if let Some(err) = fatal {
                    return Err(LunaError::Agent(err));
                }
                continue;
            }

            if let Some(method) = message.get("method").and_then(Value::as_str) {
                self.handle_notification(
                    method,
                    message.get("params").cloned().unwrap_or(Value::Null),
                    0,
                )
                .await?;
            }
        }
    }

    async fn handle_server_request(
        &mut self,
        method: &str,
        id: Value,
        params: Value,
    ) -> Result<Option<String>> {
        let response = match method {
            "item/commandExecution/requestApproval" | "execCommandApproval" => {
                Some(json!({"decision": "acceptForSession"}))
            }
            "item/fileChange/requestApproval" | "applyPatchApproval" => {
                Some(json!({"decision": "acceptForSession"}))
            }
            "item/permissions/requestApproval" => Some(json!({
                "permissions": params.get("permissions").cloned().unwrap_or(Value::Null),
                "scope": "turn"
            })),
            "item/tool/call" => {
                let tool_name = params
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                Some(json!({
                    "success": false,
                    "contentItems": [
                        {
                            "type": "inputText",
                            "text": format!("unsupported tool call in luna: {tool_name}")
                        }
                    ]
                }))
            }
            "item/tool/requestUserInput" | "mcpServer/elicitation/request" => {
                self.write_json(&json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32000,
                        "message": "luna is configured to reject interactive input requests"
                    }
                }))
                .await?;
                return Ok(Some("turn_input_required".to_string()));
            }
            _ => {
                self.write_json(&json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32000,
                        "message": format!("unsupported server request: {method}")
                    }
                }))
                .await?;
                return Ok(None);
            }
        };

        if let Some(result) = response {
            self.write_json(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            }))
            .await?;
        }

        Ok(None)
    }

    async fn handle_notification(
        &mut self,
        method: &str,
        params: Value,
        turn_number: u32,
    ) -> Result<bool> {
        match method {
            "thread/started" => {
                if self.thread_id.is_none() {
                    self.thread_id = extract_string(&params, &["/thread/id"]);
                }
                self.emit(method, None, None, Some(turn_number));
            }
            "turn/started" => {
                if self.turn_id.is_none() {
                    self.turn_id = extract_string(&params, &["/turn/id"]);
                }
                if let (Some(thread_id), Some(turn_id)) = (&self.thread_id, &self.turn_id) {
                    self.session_id = Some(format!("{thread_id}-{turn_id}"));
                }
                self.emit(method, None, None, Some(turn_number));
            }
            "thread/tokenUsage/updated" => {
                let usage = extract_usage(&params);
                self.emit(method, None, usage, Some(turn_number));
            }
            "item/agentMessage/delta" => {
                let message = params
                    .get("delta")
                    .and_then(Value::as_str)
                    .map(truncate_message);
                self.emit(method, message, None, Some(turn_number));
            }
            "turn/completed" => {
                self.turn_id =
                    extract_string(&params, &["/turn/id"]).or_else(|| self.turn_id.clone());
                self.turn_terminal_status = extract_string(&params, &["/turn/status"]);
                let message = extract_string(&params, &["/turn/error/message"]);
                self.emit(method, message, None, Some(turn_number));
            }
            "account/rateLimits/updated" => {
                self.emit_with_rate_limits(method, params, Some(turn_number));
            }
            "error" => {
                self.emit(method, Some(params.to_string()), None, Some(turn_number));
            }
            _ => {}
        }
        Ok(true)
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
            codex_app_server_pid: self.pid,
            message,
            usage,
            rate_limits: None,
            turn_count,
        }));
    }

    fn emit_with_rate_limits(&self, event: &str, rate_limits: Value, turn_count: Option<u32>) {
        let _ = self.events.send(WorkerEvent::Session(SessionUpdate {
            issue_id: self.issue_id.clone(),
            issue_identifier: self.issue_identifier.clone(),
            event: event.to_string(),
            timestamp: Utc::now(),
            session_id: self.session_id.clone(),
            thread_id: self.thread_id.clone(),
            turn_id: self.turn_id.clone(),
            codex_app_server_pid: self.pid,
            message: None,
            usage: None,
            rate_limits: Some(rate_limits),
            turn_count,
        }));
    }

    async fn read_message(&mut self, timeout_duration: Option<Duration>) -> Result<Value> {
        let mut buf = Vec::new();
        let read = async { self.stdout.read_until(b'\n', &mut buf).await };
        let bytes = match timeout_duration {
            Some(duration) => tokio::time::timeout(duration, read)
                .await
                .map_err(|_| LunaError::Agent("response_timeout".to_string()))??,
            None => read.await?,
        };

        if bytes == 0 {
            return Err(LunaError::Agent("port_exit".to_string()));
        }
        if buf.len() > MAX_JSON_LINE_BYTES {
            return Err(LunaError::Agent("protocol line exceeded 10MB".to_string()));
        }
        let message = String::from_utf8_lossy(&buf).trim().to_string();
        serde_json::from_str(&message).map_err(Into::into)
    }

    async fn write_json(&mut self, value: &Value) -> Result<()> {
        let serialized = serde_json::to_vec(value)?;
        self.stdin.write_all(&serialized).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn kill_process(&mut self) {
        if let Err(err) = self.child.kill().await {
            warn!(issue_id = %self.issue_id, error = %err, "failed to kill codex process");
        }
    }

    async fn shutdown(&mut self) {
        self.kill_process().await;
        self.stderr_task.abort();
    }
}

async fn log_stderr(
    stderr: tokio::process::ChildStderr,
    issue_id: String,
    issue_identifier: String,
) {
    let mut reader = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        warn!(
            issue_id = %issue_id,
            issue_identifier = %issue_identifier,
            codex_stderr = %truncate_message(line),
            "codex stderr"
        );
    }
}

fn extract_string(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|pointer| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn extract_usage(value: &Value) -> Option<UsageUpdate> {
    let input_tokens = value
        .pointer("/tokenUsage/total/inputTokens")
        .and_then(Value::as_u64)?;
    let output_tokens = value
        .pointer("/tokenUsage/total/outputTokens")
        .and_then(Value::as_u64)?;
    let total_tokens = value
        .pointer("/tokenUsage/total/totalTokens")
        .and_then(Value::as_u64)?;
    Some(UsageUpdate {
        input_tokens,
        output_tokens,
        total_tokens,
    })
}

fn truncate_message(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    const LIMIT: usize = 400;
    if value.len() <= LIMIT {
        value.to_string()
    } else {
        format!("{}...", &value[..LIMIT])
    }
}
