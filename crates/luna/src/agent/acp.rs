use std::{path::PathBuf, str::FromStr};

use agent_client_protocol::schema::{
    ContentBlock, InitializeRequest, NewSessionRequest, PromptRequest, ProtocolVersion,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, TextContent,
};
use agent_client_protocol::{Agent, Client, ConnectionTo};
use agent_client_protocol_tokio::AcpAgent;
use tokio::{
    process::{Child, Command},
    sync::{mpsc, oneshot, watch},
};
use tracing::{debug, info, warn};

use crate::{
    agent::{SessionUpdate, StopReason, TurnExit, WorkerEvent},
    config::AcpRunner,
    error::{LunaError, Result},
    paths::absolutize_path,
};

pub struct AcpSession {
    _child: Child,
    _events: mpsc::UnboundedSender<WorkerEvent>,
    _issue_id: String,
    _issue_identifier: String,
    prompt_tx: mpsc::Sender<(String, oneshot::Sender<Result<TurnExit>>)>,
    shutdown_tx: mpsc::Sender<()>,
    session_ready_rx: Option<oneshot::Receiver<Result<String>>>,
}

impl AcpSession {
    pub async fn launch(
        config: &AcpRunner,
        workspace_path: &std::path::Path,
        issue_id: String,
        issue_identifier: String,
        events: mpsc::UnboundedSender<WorkerEvent>,
    ) -> Result<Self> {
        let workspace_path =
            std::fs::canonicalize(workspace_path).or_else(|_| absolutize_path(workspace_path))?;

        let child = Command::new("bash")
            .arg("-lc")
            .arg(&config.command)
            .current_dir(&workspace_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let agent_pid = child.id();

        let agent = AcpAgent::from_str(&config.command)
            .map_err(|e| LunaError::Agent(format!("failed to parse acp agent command: {e}")))?;

        let (prompt_tx, mut prompt_rx) = mpsc::channel::<(String, oneshot::Sender<Result<TurnExit>>)>(1);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let (session_ready_tx, session_ready_rx) = oneshot::channel();

        let events_notif = events.clone();
        let events_warn = events.clone();
        let issue_id_notif = issue_id.clone();
        let issue_identifier_notif = issue_identifier.clone();
        let issue_id_conn = issue_id.clone();
        let issue_id_warn = issue_id.clone();
        let issue_identifier_warn = issue_identifier.clone();
        let workspace_path_bg = workspace_path.to_string_lossy().to_string();

        tokio::spawn(async move {
            let result = Client
                .builder()
                .on_receive_notification(
                    async move |notification: SessionNotification, _cx| {
                        match notification.update {
                            agent_client_protocol::schema::SessionUpdate::AgentThoughtChunk(chunk) => {
                                if let agent_client_protocol::schema::ContentBlock::Text(text) = chunk.content {
                                    let _ = events_notif.send(WorkerEvent::Session(SessionUpdate {
                                        issue_id: issue_id_notif.clone(),
                                        issue_identifier: issue_identifier_notif.clone(),
                                        event: "item/agentMessage/delta".to_string(),
                                        timestamp: chrono::Utc::now(),
                                        session_id: None,
                                        thread_id: None,
                                        turn_id: None,
                                        agent_pid,
                                        message: Some(text.text.clone()),
                                        usage: None,
                                        rate_limits: None,
                                        turn_count: None,
                                    }));
                                }
                            }
                            _ => {}
                        }
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .on_receive_request(
                    async move |request: RequestPermissionRequest, responder: agent_client_protocol::Responder<RequestPermissionResponse>, _connection| {
                        let option_id = request.options.first().map(|opt| opt.option_id.clone());
                        if let Some(id) = option_id {
                            responder.respond(RequestPermissionResponse::new(
                                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
                            ))
                        } else {
                            responder.respond(RequestPermissionResponse::new(
                                RequestPermissionOutcome::Cancelled,
                            ))
                        }
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                    info!(issue_id = %issue_id_conn, "acp initializing");
                    let _init = connection
                        .send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await?;

                    let session = connection
                        .send_request(NewSessionRequest::new(PathBuf::from(&workspace_path_bg)))
                        .block_task()
                        .await?;

                    let session_id = session.session_id.to_string();
                    let _ = session_ready_tx.send(Ok(session_id.clone()));
                    info!(issue_id = %issue_id_conn, %session_id, "acp session created");

                    loop {
                        tokio::select! {
                            Some((prompt, response_tx)) = prompt_rx.recv() => {
                                debug!(issue_id = %issue_id_conn, "acp sending prompt");
                                let result = connection
                                    .send_request(PromptRequest::new(
                                        session_id.clone(),
                                        vec![ContentBlock::Text(TextContent::new(prompt))],
                                    ))
                                    .block_task()
                                    .await;

                                match result {
                                    Ok(_response) => {
                                        let _ = response_tx.send(Ok(TurnExit::Completed));
                                    }
                                    Err(e) => {
                                        let _ = response_tx.send(Err(LunaError::Agent(format!(
                                            "acp prompt failed: {e}"
                                        ))));
                                    }
                                }
                            }
                            _ = shutdown_rx.recv() => {
                                info!(issue_id = %issue_id_conn, "acp shutting down");
                                break;
                            }
                        }
                    }

                    Ok(())
                })
                .await;

            if let Err(e) = result {
                warn!(issue_id = %issue_id_warn, error = %e, "acp background task exited with error");
                let _ = events_warn.send(WorkerEvent::Session(SessionUpdate {
                    issue_id: issue_id_warn,
                    issue_identifier: issue_identifier_warn,
                    event: "error".to_string(),
                    timestamp: chrono::Utc::now(),
                    session_id: None,
                    thread_id: None,
                    turn_id: None,
                    agent_pid,
                    message: Some(format!("acp error: {e}")),
                    usage: None,
                    rate_limits: None,
                    turn_count: None,
                }));
            }
        });

        Ok(Self {
            _child: child,
            _events: events,
            _issue_id: issue_id,
            _issue_identifier: issue_identifier,
            prompt_tx,
            shutdown_tx,
            session_ready_rx: Some(session_ready_rx),
        })
    }
}

#[async_trait::async_trait]
impl crate::agent::AgentSession for AcpSession {
    async fn start(&mut self) -> Result<()> {
        let rx = self
            .session_ready_rx
            .take()
            .ok_or_else(|| LunaError::Agent("acp session already started".to_string()))?;
        rx.await
            .map_err(|_| LunaError::Agent("acp session_ready channel closed".to_string()))??;
        Ok(())
    }

    async fn run_turn(
        &mut self,
        prompt: &str,
        _turn_number: u32,
        stop_rx: &mut watch::Receiver<Option<StopReason>>,
    ) -> Result<TurnExit> {
        let (tx, rx) = oneshot::channel();
        self.prompt_tx
            .send((prompt.to_string(), tx))
            .await
            .map_err(|_| LunaError::Agent("acp prompt channel closed".to_string()))?;

        tokio::select! {
            result = rx => {
                result.map_err(|_| LunaError::Agent("acp turn response channel closed".to_string()))?
            }
            changed = stop_rx.changed() => {
                if changed.is_ok() {
                    let reason = stop_rx.borrow().clone().unwrap_or(StopReason::Shutdown);
                    return Ok(TurnExit::Stopped(reason));
                }
                // Channel closed — treat as shutdown
                Ok(TurnExit::Stopped(StopReason::Shutdown))
            }
        }
    }

    async fn shutdown(&mut self) {
        let _ = self.shutdown_tx.send(()).await;
    }
}
