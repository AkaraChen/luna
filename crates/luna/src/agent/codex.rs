use std::{path::Path, process::Stdio};

use chrono::Utc;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::mpsc,
    task::JoinHandle,
    time::Duration,
};
use tracing::{debug, info, warn};

use crate::{
    agent::{SessionUpdate, StopReason, TurnExit, UsageUpdate, WorkerEvent},
    config::CodexRunner,
    error::{LunaError, Result},
    paths::absolutize_path,
};

const MAX_JSON_LINE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum CompletedItemLog {
    CommandExecution {
        command: String,
        cwd: Option<String>,
        duration_ms: Option<i64>,
        exit_code: Option<i64>,
    },
    DynamicToolCall {
        tool: String,
        namespace: Option<String>,
        duration_ms: Option<i64>,
        success: Option<bool>,
    },
    McpToolCall {
        tool: String,
        server: String,
        duration_ms: Option<i64>,
    },
    CollabAgentToolCall {
        tool: String,
        duration_ms: Option<i64>,
        receiver_thread_count: usize,
    },
}

pub struct CodexSession {
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
    config: CodexRunner,
}

impl CodexSession {
    pub async fn launch(
        config: &CodexRunner,
        workspace_path: &Path,
        issue_id: String,
        issue_identifier: String,
        events: mpsc::UnboundedSender<WorkerEvent>,
    ) -> Result<Self> {
        let workspace_path =
            std::fs::canonicalize(workspace_path).or_else(|_| absolutize_path(workspace_path))?;
        let mut child = Command::new("bash")
            .arg("-lc")
            .arg(&config.command)
            .current_dir(&workspace_path)
            .env("LUNA_ISSUE_ID", &issue_id)
            .env("LUNA_ISSUE_IDENTIFIER", &issue_identifier)
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
            config: config.clone(),
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

    async fn start_thread(&mut self) -> Result<()> {
        let response = self
            .request(
                "thread/start",
                json!({
                    "cwd": self.workspace_path.clone(),
                    "serviceName": "luna",
                    "sessionStartSource": "startup",
                    "approvalPolicy": self.config.approval_policy.clone(),
                    "sandbox": self.config.thread_sandbox.clone(),
                }),
                self.config.read_timeout_ms,
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

    async fn run_turn_inner(
        &mut self,
        prompt: &str,
        turn_number: u32,
        stop_rx: &mut tokio::sync::watch::Receiver<Option<StopReason>>,
    ) -> Result<TurnExit> {
        self.turn_terminal_status = None;
        let response = self
            .request(
                "turn/start",
                json!({
                    "threadId": self.thread_id.clone().ok_or_else(|| LunaError::Agent("missing thread id".to_string()))?,
                    "cwd": self.workspace_path.clone(),
                    "approvalPolicy": self.config.approval_policy.clone(),
                    "sandboxPolicy": self.config.turn_sandbox_policy.clone(),
                    "input": [
                        {
                            "type": "text",
                            "text": prompt,
                        }
                    ]
                }),
                self.config.read_timeout_ms,
            )
            .await?;

        self.turn_id = extract_string(&response, &["/turn/id"]);
        if let (Some(thread_id), Some(turn_id)) = (&self.thread_id, &self.turn_id) {
            self.session_id = Some(format!("{thread_id}-{turn_id}"));
            self.emit("session_started", None, None, Some(turn_number));
        }

        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(self.config.turn_timeout_ms);
        loop {
            if let Some(status) = self.turn_terminal_status.take() {
                return Ok(match status.as_str() {
                    "completed" => TurnExit::Completed,
                    "interrupted" => TurnExit::Failed("turn interrupted".to_string()),
                    "failed" => TurnExit::Failed("turn failed".to_string()),
                    other => TurnExit::Failed(format!("unexpected turn status: {other}")),
                });
            }

            let stop_deadline = tokio::time::sleep_until(deadline);
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
            "item/completed" => {
                self.log_completed_item(&params);
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

    fn log_completed_item(&self, params: &Value) {
        let Some(item_log) = extract_completed_item_log(params) else {
            return;
        };

        match item_log {
            CompletedItemLog::CommandExecution {
                command,
                cwd,
                duration_ms,
                exit_code,
            } => {
                info!(
                    issue_id = %self.issue_id,
                    issue_identifier = %self.issue_identifier,
                    session_id = self.session_id.as_deref().unwrap_or("unknown"),
                    command = %command,
                    cwd = cwd.as_deref().unwrap_or("unknown"),
                    duration_ms,
                    exit_code,
                    "codex command execution completed"
                );
            }
            CompletedItemLog::DynamicToolCall {
                tool,
                namespace,
                duration_ms,
                success,
            } => {
                info!(
                    issue_id = %self.issue_id,
                    issue_identifier = %self.issue_identifier,
                    session_id = self.session_id.as_deref().unwrap_or("unknown"),
                    tool = %tool,
                    namespace = namespace.as_deref().unwrap_or(""),
                    duration_ms,
                    success,
                    "codex dynamic tool call completed"
                );
            }
            CompletedItemLog::McpToolCall {
                tool,
                server,
                duration_ms,
            } => {
                info!(
                    issue_id = %self.issue_id,
                    issue_identifier = %self.issue_identifier,
                    session_id = self.session_id.as_deref().unwrap_or("unknown"),
                    tool = %tool,
                    server = %server,
                    duration_ms,
                    "codex mcp tool call completed"
                );
            }
            CompletedItemLog::CollabAgentToolCall {
                tool,
                duration_ms,
                receiver_thread_count,
            } => {
                info!(
                    issue_id = %self.issue_id,
                    issue_identifier = %self.issue_identifier,
                    session_id = self.session_id.as_deref().unwrap_or("unknown"),
                    tool = %tool,
                    duration_ms,
                    receiver_thread_count,
                    "codex collab tool call completed"
                );
            }
        }
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
            agent_pid: self.pid,
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
            agent_pid: self.pid,
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
}

#[async_trait::async_trait]
impl crate::agent::AgentSession for CodexSession {
    async fn start(&mut self) -> Result<()> {
        self.start_thread().await
    }

    async fn run_turn(
        &mut self,
        prompt: &str,
        turn_number: u32,
        stop_rx: &mut tokio::sync::watch::Receiver<Option<StopReason>>,
    ) -> Result<TurnExit> {
        self.run_turn_inner(prompt, turn_number, stop_rx).await
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

fn extract_completed_item_log(params: &Value) -> Option<CompletedItemLog> {
    let item = params.get("item")?;
    let item_type = item.get("type")?.as_str()?;
    let status = item.get("status").and_then(Value::as_str)?;

    match item_type {
        "commandExecution" if status == "completed" => {
            let exit_code = item.get("exitCode").and_then(Value::as_i64);
            if !matches!(exit_code, None | Some(0)) {
                return None;
            }

            Some(CompletedItemLog::CommandExecution {
                command: item
                    .get("command")
                    .and_then(Value::as_str)
                    .map(truncate_message)?,
                cwd: item.get("cwd").and_then(Value::as_str).map(str::to_string),
                duration_ms: item.get("durationMs").and_then(Value::as_i64),
                exit_code,
            })
        }
        "dynamicToolCall" if status == "completed" => {
            let success = item.get("success").and_then(Value::as_bool);
            if success == Some(false) {
                return None;
            }

            Some(CompletedItemLog::DynamicToolCall {
                tool: item.get("tool").and_then(Value::as_str)?.to_string(),
                namespace: item
                    .get("namespace")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                duration_ms: item.get("durationMs").and_then(Value::as_i64),
                success,
            })
        }
        "mcpToolCall" if status == "completed" => Some(CompletedItemLog::McpToolCall {
            tool: item.get("tool").and_then(Value::as_str)?.to_string(),
            server: item.get("server").and_then(Value::as_str)?.to_string(),
            duration_ms: item.get("durationMs").and_then(Value::as_i64),
        }),
        "collabAgentToolCall" if status == "completed" => {
            Some(CompletedItemLog::CollabAgentToolCall {
                tool: item.get("tool").and_then(Value::as_str)?.to_string(),
                duration_ms: item.get("durationMs").and_then(Value::as_i64),
                receiver_thread_count: item
                    .get("receiverThreadIds")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            })
        }
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CompletedItemLog, extract_completed_item_log};

    #[test]
    fn extracts_successful_command_execution_log() {
        let params = json!({
            "item": {
                "type": "commandExecution",
                "status": "completed",
                "command": "gh issue view 31 -R AkaraChen/fama",
                "cwd": "/tmp/workspace",
                "durationMs": 1250,
                "exitCode": 0
            }
        });

        assert_eq!(
            extract_completed_item_log(&params),
            Some(CompletedItemLog::CommandExecution {
                command: "gh issue view 31 -R AkaraChen/fama".to_string(),
                cwd: Some("/tmp/workspace".to_string()),
                duration_ms: Some(1250),
                exit_code: Some(0),
            })
        );
    }

    #[test]
    fn ignores_unsuccessful_dynamic_tool_call() {
        let params = json!({
            "item": {
                "type": "dynamicToolCall",
                "status": "completed",
                "tool": "linear_graphql",
                "namespace": "luna",
                "durationMs": 42,
                "success": false
            }
        });

        assert_eq!(extract_completed_item_log(&params), None);
    }
}
