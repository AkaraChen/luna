use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tokio::{
    signal,
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Duration, Instant, interval, interval_at},
};
use tracing::{debug, error, info, warn};

use crate::{
    agent::{StopReason, UsageUpdate, WorkerEvent, WorkerExit, WorkerOutcome, run_agent_attempt},
    config::{ServiceConfig, TrackerConfig},
    error::{LunaError, Result},
    model::{Issue, TokenTotals},
    tracker::build_tracker,
    workflow::{LoadedWorkflow, WorkflowStore, parse_workflow_definition},
    workspace::WorkspaceManager,
};

pub async fn run(workflow_path: PathBuf) -> Result<()> {
    let mut store = WorkflowStore::load(workflow_path.clone())?;

    let raw_contents = tokio::fs::read_to_string(&workflow_path).await?;
    let raw_def = parse_workflow_definition(&raw_contents)?;
    let auto_start_asahi = !raw_def.config.contains_key("tracker");

    let mut asahi_shutdown: Option<tokio::sync::oneshot::Sender<()>> = None;

    let should_embed = auto_start_asahi
        || matches!(
            &store.current().config.tracker,
            TrackerConfig::Asahi(cfg) if cfg.db.is_some()
        );

    if should_embed {
        let port = match &store.current().config.tracker {
            TrackerConfig::Asahi(cfg) if cfg.port.is_some() => cfg.port.unwrap(),
            _ => find_available_port().await?,
        };

        let db_path = match &store.current().config.tracker {
            TrackerConfig::Asahi(cfg) => cfg
                .db
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| store.current().config.workflow_dir.join("asahi.db")),
            _ => store.current().config.workflow_dir.join("asahi.db"),
        };

        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let rocket = asahi::rocket_with_database_url_and_port(db_url, port);

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        asahi_shutdown = Some(tx);

        tokio::spawn(async move {
            match rocket.launch().await {
                Ok(orbit) => {
                    let _ = rx.await;
                    orbit.shutdown().notify();
                }
                Err(e) => {
                    error!(error = %e, "asahi server error");
                }
            }
        });

        let endpoint = format!("http://127.0.0.1:{}", port);
        wait_for_asahi(&endpoint).await?;

        if let TrackerConfig::Asahi(ref mut config) = store.current_mut().config.tracker {
            config.endpoint = endpoint;
        }

        info!("embedded asahi started on port {}", port);
    }

    let initial = store.current().clone();

    info!(
        tracker = ?std::mem::discriminant(&initial.config.tracker),
        runner = ?std::mem::discriminant(&initial.config.runner),
        interval_ms = initial.config.polling.interval_ms,
        max_concurrent = initial.config.scheduler.max_concurrent,
        "luna orchestrator started"
    );

    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let mut state = OrchestratorState::new(&initial.config);

    startup_terminal_workspace_cleanup(&initial).await;

    let mut ticker = interval(Duration::from_millis(
        initial.config.polling.interval_ms.max(1),
    ));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("received shutdown signal");
                shutdown_running(&mut state);
                if let Some(tx) = asahi_shutdown.take() {
                    let _ = tx.send(());
                    info!("signaled embedded asahi to stop");
                }
                break;
            }
            _ = ticker.tick() => {
                if let Err(err) = on_tick(&mut store, &mut state, &events_tx).await {
                    error!(error = %err, "poll tick failed");
                }
                let next = Instant::now() + Duration::from_millis(store.current().config.polling.interval_ms.max(1));
                ticker = interval_at(next, Duration::from_millis(store.current().config.polling.interval_ms.max(1)));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            }
            Some(event) = events_rx.recv() => {
                handle_worker_event(event, &mut store, &mut state, &events_tx).await;
            }
        }
    }

    Ok(())
}

async fn on_tick(
    store: &mut WorkflowStore,
    state: &mut OrchestratorState,
    events_tx: &mpsc::UnboundedSender<WorkerEvent>,
) -> Result<()> {
    let current = store.current().clone();
    reconcile_running_issues(state, &current, events_tx).await;

    let dispatch_enabled = match store.reload_if_changed() {
        Ok(true) => {
            info!(workflow = %store.path().display(), "reloaded workflow configuration");
            true
        }
        Ok(false) => true,
        Err(err) => {
            error!(error = %err, "workflow reload failed; keeping last known good configuration");
            false
        }
    };

    let workflow = store.current().clone();
    state.poll_interval_ms = workflow.config.polling.interval_ms;
    state.max_concurrent_agents = workflow.config.scheduler.max_concurrent;

    if !dispatch_enabled {
        return Ok(());
    }

    let tracker = match build_tracker(&workflow.config.tracker) {
        Ok(client) => client,
        Err(err) => {
            error!(error = %err, "tracker client initialization failed");
            return Ok(());
        }
    };

    let candidates = match tracker.fetch_candidate_issues().await {
        Ok(issues) => issues,
        Err(err) => {
            error!(error = %err, "candidate issue fetch failed");
            return Ok(());
        }
    };

    info!(candidate_count = candidates.len(), running = state.running.len(), claimed = state.claimed.len(), "poll tick");

    let mut sorted = candidates;
    sorted.sort_by(sort_issues_for_dispatch);

    let mut dispatched = 0;
    for issue in sorted {
        if available_global_slots(state, &workflow.config) == 0 {
            info!("no available global slots, skipping remaining issues");
            break;
        }
        if should_dispatch(&issue, state, &workflow.config) {
            info!(issue_id = %issue.id, identifier = %issue.identifier, state = %issue.state, "dispatching agent");
            dispatch_issue(issue, None, workflow.clone(), state, events_tx);
            dispatched += 1;
        } else {
            debug!(issue_id = %issue.id, identifier = %issue.identifier, state = %issue.state, "issue skipped");
        }
    }

    if dispatched > 0 {
        info!(dispatched, "tick complete");
    } else {
        info!("tick complete, no issues dispatched");
    }

    Ok(())
}

async fn handle_worker_event(
    event: WorkerEvent,
    store: &mut WorkflowStore,
    state: &mut OrchestratorState,
    events_tx: &mpsc::UnboundedSender<WorkerEvent>,
) {
    match event {
        WorkerEvent::Session(update) => apply_session_update(update, state),
        WorkerEvent::Exited(exit) => {
            handle_worker_exit(exit, store.current().clone(), state, events_tx).await
        }
        WorkerEvent::RetryDue(issue_id) => {
            handle_retry_due(issue_id, store.current().clone(), state, events_tx).await
        }
    }
}

fn apply_session_update(update: crate::agent::SessionUpdate, state: &mut OrchestratorState) {
    let Some(entry) = state.running.get_mut(&update.issue_id) else {
        return;
    };

    entry.last_agent_event = Some(update.event.clone());
    entry.last_agent_timestamp = Some(update.timestamp);
    entry.last_agent_message = update.message.clone();
    entry.session_id = update.session_id.clone();
    entry.thread_id = update.thread_id.clone();
    entry.turn_id = update.turn_id.clone();
    entry.agent_pid = update.agent_pid;
    if let Some(turn_count) = update.turn_count {
        entry.turn_count = turn_count;
    }
    if let Some(rate_limits) = update.rate_limits {
        state.agent_rate_limits = Some(rate_limits);
    }
    if let Some(usage) = update.usage {
        apply_usage_update(state, &update.issue_id, usage);
    }
}

fn apply_usage_update(state: &mut OrchestratorState, issue_id: &str, usage: UsageUpdate) {
    let Some(entry) = state.running.get_mut(issue_id) else {
        return;
    };
    let delta_input = usage
        .input_tokens
        .saturating_sub(entry.last_reported_input_tokens);
    let delta_output = usage
        .output_tokens
        .saturating_sub(entry.last_reported_output_tokens);
    let delta_total = usage
        .total_tokens
        .saturating_sub(entry.last_reported_total_tokens);

    entry.agent_input_tokens = usage.input_tokens;
    entry.agent_output_tokens = usage.output_tokens;
    entry.agent_total_tokens = usage.total_tokens;
    entry.last_reported_input_tokens = usage.input_tokens;
    entry.last_reported_output_tokens = usage.output_tokens;
    entry.last_reported_total_tokens = usage.total_tokens;

    state.agent_totals.input_tokens += delta_input;
    state.agent_totals.output_tokens += delta_output;
    state.agent_totals.total_tokens += delta_total;
}

async fn handle_worker_exit(
    exit: WorkerExit,
    workflow: LoadedWorkflow,
    state: &mut OrchestratorState,
    events_tx: &mpsc::UnboundedSender<WorkerEvent>,
) {
    let Some(entry) = state.running.remove(&exit.issue_id) else {
        return;
    };
    state.agent_totals.seconds_running += exit.runtime_seconds;
    entry.worker.abort();

    let cleanup_workspace = entry.pending_cleanup;
    let identifier = entry.identifier.clone();

    info!(
        issue_id = %exit.issue_id,
        identifier = %identifier,
        outcome = ?exit.outcome,
        runtime_seconds = %exit.runtime_seconds,
        "agent exited"
    );

    if cleanup_workspace {
        let workspace_manager = WorkspaceManager::new(
            workflow.config.workspace.root.clone(),
            workflow.config.hooks.clone(),
            Some(workflow.config.workflow_dir.clone()),
        );
        if let Err(err) = workspace_manager.cleanup(&identifier).await {
            warn!(issue_id = %exit.issue_id, issue_identifier = %identifier, error = %err, "workspace cleanup failed");
        }
    }

    match exit.outcome {
        WorkerOutcome::Normal => {
            state.completed.insert(exit.issue_id.clone());
            schedule_retry(
                state,
                workflow.config.clone(),
                exit.issue_id,
                1,
                Some(identifier),
                None,
                RetryDelay::Continuation,
                events_tx,
            );
        }
        WorkerOutcome::Failed(reason) => {
            schedule_retry(
                state,
                workflow.config.clone(),
                exit.issue_id,
                entry.retry_attempt.unwrap_or(0) + 1,
                Some(identifier),
                Some(reason),
                RetryDelay::Backoff,
                events_tx,
            );
        }
        WorkerOutcome::TimedOut => {
            schedule_retry(
                state,
                workflow.config.clone(),
                exit.issue_id,
                entry.retry_attempt.unwrap_or(0) + 1,
                Some(identifier),
                Some("turn_timeout".to_string()),
                RetryDelay::Backoff,
                events_tx,
            );
        }
        WorkerOutcome::Stalled => {
            schedule_retry(
                state,
                workflow.config.clone(),
                exit.issue_id,
                entry.retry_attempt.unwrap_or(0) + 1,
                Some(identifier),
                Some("stalled".to_string()),
                RetryDelay::Backoff,
                events_tx,
            );
        }
        WorkerOutcome::CanceledByReconciliation => {
            state.claimed.remove(&exit.issue_id);
        }
    }
}

async fn handle_retry_due(
    issue_id: String,
    workflow: LoadedWorkflow,
    state: &mut OrchestratorState,
    events_tx: &mpsc::UnboundedSender<WorkerEvent>,
) {
    let Some(entry) = state.retry_attempts.remove(&issue_id) else {
        return;
    };

    let tracker = match build_tracker(&workflow.config.tracker) {
        Ok(client) => client,
        Err(err) => {
            error!(error = %err, "retry tracker client init failed");
            return;
        }
    };

    let candidates = match tracker.fetch_candidate_issues().await {
        Ok(issues) => issues,
        Err(err) => {
            warn!(issue_id = %issue_id, error = %err, "retry poll failed");
            schedule_retry(
                state,
                workflow.config.clone(),
                issue_id,
                entry.attempt + 1,
                Some(entry.identifier),
                Some("retry poll failed".to_string()),
                RetryDelay::Backoff,
                events_tx,
            );
            return;
        }
    };

    let issue = candidates.into_iter().find(|issue| issue.id == issue_id);
    let Some(issue) = issue else {
        state.claimed.remove(&entry.issue_id);
        return;
    };

    if !workflow.config.tracker.is_active_state(&issue.state) {
        state.claimed.remove(&entry.issue_id);
        return;
    }

    if available_global_slots(state, &workflow.config) == 0
        || !has_available_state_slot(&issue.state, state, &workflow.config)
    {
        schedule_retry(
            state,
            workflow.config.clone(),
            entry.issue_id,
            entry.attempt + 1,
            Some(issue.identifier),
            Some("no available orchestrator slots".to_string()),
            RetryDelay::Backoff,
            events_tx,
        );
        return;
    }

    dispatch_issue(issue, Some(entry.attempt), workflow, state, events_tx);
}

fn dispatch_issue(
    issue: Issue,
    attempt: Option<u32>,
    workflow: LoadedWorkflow,
    state: &mut OrchestratorState,
    events_tx: &mpsc::UnboundedSender<WorkerEvent>,
) {
    let issue_id = issue.id.clone();
    let identifier = issue.identifier.clone();
    let attempt_num = attempt.unwrap_or(0);
    info!(issue_id = %issue_id, identifier = %identifier, attempt = attempt_num, "agent spawned");
    let (stop_tx, stop_rx) = watch::channel(None);
    let worker = tokio::spawn(run_agent_attempt(
        issue.clone(),
        attempt,
        workflow,
        events_tx.clone(),
        stop_rx,
    ));

    if let Some(existing) = state.retry_attempts.remove(&issue_id) {
        existing.task.abort();
    }
    state.claimed.insert(issue_id.clone());
    state.running.insert(
        issue_id,
        RunningEntry {
            worker,
            stop_tx,
            identifier,
            issue,
            session_id: None,
            thread_id: None,
            turn_id: None,
            agent_pid: None,
            last_agent_message: None,
            last_agent_event: None,
            last_agent_timestamp: None,
            agent_input_tokens: 0,
            agent_output_tokens: 0,
            agent_total_tokens: 0,
            last_reported_input_tokens: 0,
            last_reported_output_tokens: 0,
            last_reported_total_tokens: 0,
            retry_attempt: attempt,
            started_at: Utc::now(),
            pending_cleanup: false,
            turn_count: 0,
        },
    );
}

async fn reconcile_running_issues(
    state: &mut OrchestratorState,
    workflow: &LoadedWorkflow,
    _events_tx: &mpsc::UnboundedSender<WorkerEvent>,
) {
    reconcile_stalled_runs(state, &workflow.config);

    if state.running.is_empty() {
        return;
    }

    let tracker = match build_tracker(&workflow.config.tracker) {
        Ok(client) => client,
        Err(err) => {
            warn!(error = %err, "tracker client init failed during reconciliation");
            return;
        }
    };

    let ids = state.running.keys().cloned().collect::<Vec<_>>();
    let refreshed = match tracker.fetch_issue_states_by_ids(&ids).await {
        Ok(issues) => issues,
        Err(err) => {
            warn!(error = %err, "issue state refresh failed; keeping workers running");
            return;
        }
    };

    let refreshed_by_id = refreshed
        .into_iter()
        .map(|issue| (issue.id.clone(), issue))
        .collect::<HashMap<_, _>>();

    for issue_id in ids {
        let Some(running) = state.running.get_mut(&issue_id) else {
            continue;
        };

        if let Some(refreshed) = refreshed_by_id.get(&issue_id) {
            if workflow.config.tracker.is_terminal_state(&refreshed.state) {
                running.pending_cleanup = true;
                let _ = running.stop_tx.send(Some(StopReason::Terminal));
            } else if workflow.config.tracker.is_active_state(&refreshed.state) {
                running.issue = refreshed.clone();
            } else {
                let _ = running.stop_tx.send(Some(StopReason::NonActive));
            }
        }
    }
}

fn reconcile_stalled_runs(state: &mut OrchestratorState, config: &ServiceConfig) {
    if config.runner.stall_timeout_ms() <= 0 {
        return;
    }

    let now = Utc::now();
    for running in state.running.values_mut() {
        let elapsed_ms = now
            .signed_duration_since(running.last_agent_timestamp.unwrap_or(running.started_at))
            .num_milliseconds();
        if elapsed_ms > config.runner.stall_timeout_ms() {
            let _ = running.stop_tx.send(Some(StopReason::Stalled));
        }
    }
}

fn should_dispatch(issue: &Issue, state: &OrchestratorState, config: &ServiceConfig) -> bool {
    if issue.id.is_empty()
        || issue.identifier.is_empty()
        || issue.title.is_empty()
        || issue.state.is_empty()
    {
        return false;
    }
    if !config.tracker.is_active_state(&issue.state)
        || config.tracker.is_terminal_state(&issue.state)
    {
        return false;
    }
    if state.running.contains_key(&issue.id) || state.claimed.contains(&issue.id) {
        return false;
    }
    if available_global_slots(state, config) == 0
        || !has_available_state_slot(&issue.state, state, config)
    {
        return false;
    }
    if issue.state.eq_ignore_ascii_case("todo")
        && issue.blocked_by.iter().any(|blocker| {
            blocker
                .state
                .as_deref()
                .map(|state| !config.tracker.is_terminal_state(state))
                .unwrap_or(true)
        })
    {
        return false;
    }
    true
}

fn sort_issues_for_dispatch(left: &Issue, right: &Issue) -> Ordering {
    match compare_priority(left.priority, right.priority) {
        Ordering::Equal => match left.created_at.cmp(&right.created_at) {
            Ordering::Equal => left.identifier.cmp(&right.identifier),
            other => other,
        },
        other => other,
    }
}

fn compare_priority(left: Option<i64>, right: Option<i64>) -> Ordering {
    match (left, right) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn available_global_slots(state: &OrchestratorState, config: &ServiceConfig) -> usize {
    config
        .scheduler
        .max_concurrent
        .saturating_sub(state.running.len())
}

fn has_available_state_slot(
    state_name: &str,
    state: &OrchestratorState,
    config: &ServiceConfig,
) -> bool {
    let key = state_name.to_lowercase();
    let limit = config
        .scheduler
        .max_concurrent_by_state
        .get(&key)
        .copied()
        .unwrap_or(config.scheduler.max_concurrent);
    let running_for_state = state
        .running
        .values()
        .filter(|entry| entry.issue.state.eq_ignore_ascii_case(state_name))
        .count();
    running_for_state < limit
}

fn schedule_retry(
    state: &mut OrchestratorState,
    config: ServiceConfig,
    issue_id: String,
    attempt: u32,
    identifier: Option<String>,
    error: Option<String>,
    delay_mode: RetryDelay,
    events_tx: &mpsc::UnboundedSender<WorkerEvent>,
) {
    if let Some(existing) = state.retry_attempts.remove(&issue_id) {
        existing.task.abort();
    }

    let delay_ms = match delay_mode {
        RetryDelay::Continuation => 1_000,
        RetryDelay::Backoff => {
            let multiplier =
                10_000_u64.saturating_mul(2_u64.saturating_pow(attempt.saturating_sub(1)));
            multiplier.min(config.scheduler.retry_backoff_ms)
        }
    };
    let due_at = Utc::now() + ChronoDuration::milliseconds(delay_ms as i64);
    let tx = events_tx.clone();
    let retry_issue_id = issue_id.clone();
    let task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let _ = tx.send(WorkerEvent::RetryDue(retry_issue_id));
    });

    state.retry_attempts.insert(
        issue_id.clone(),
        RetryEntry {
            issue_id,
            identifier: identifier.unwrap_or_default(),
            attempt,
            _due_at: due_at,
            _error: error,
            task,
        },
    );
}

async fn startup_terminal_workspace_cleanup(workflow: &LoadedWorkflow) {
    let tracker = match build_tracker(&workflow.config.tracker) {
        Ok(client) => client,
        Err(err) => {
            warn!(error = %err, "failed to initialize tracker for startup cleanup");
            return;
        }
    };

    let terminal_issues = match tracker
        .fetch_issues_by_states(workflow.config.tracker.terminal_states())
        .await
    {
        Ok(issues) => issues,
        Err(err) => {
            warn!(error = %err, "startup terminal workspace cleanup skipped");
            return;
        }
    };

    let workspace_manager = WorkspaceManager::new(
        workflow.config.workspace.root.clone(),
        workflow.config.hooks.clone(),
        Some(workflow.config.workflow_dir.clone()),
    );
    for issue in terminal_issues {
        if let Err(err) = workspace_manager.cleanup(&issue.identifier).await {
            warn!(issue_identifier = %issue.identifier, error = %err, "failed to clean terminal workspace");
        }
    }
}

fn shutdown_running(state: &mut OrchestratorState) {
    for running in state.running.values_mut() {
        let _ = running.stop_tx.send(Some(StopReason::Shutdown));
        running.worker.abort();
    }
    for retry in state.retry_attempts.values() {
        retry.task.abort();
    }
}

#[derive(Debug)]
struct OrchestratorState {
    poll_interval_ms: u64,
    max_concurrent_agents: usize,
    running: HashMap<String, RunningEntry>,
    claimed: HashSet<String>,
    retry_attempts: HashMap<String, RetryEntry>,
    completed: HashSet<String>,
    agent_totals: TokenTotals,
    agent_rate_limits: Option<serde_json::Value>,
}

impl OrchestratorState {
    fn new(config: &ServiceConfig) -> Self {
        Self {
            poll_interval_ms: config.polling.interval_ms,
            max_concurrent_agents: config.scheduler.max_concurrent,
            running: HashMap::new(),
            claimed: HashSet::new(),
            retry_attempts: HashMap::new(),
            completed: HashSet::new(),
            agent_totals: TokenTotals::default(),
            agent_rate_limits: None,
        }
    }
}

#[derive(Debug)]
struct RunningEntry {
    worker: JoinHandle<()>,
    stop_tx: watch::Sender<Option<StopReason>>,
    identifier: String,
    issue: Issue,
    session_id: Option<String>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    agent_pid: Option<u32>,
    last_agent_message: Option<String>,
    last_agent_event: Option<String>,
    last_agent_timestamp: Option<DateTime<Utc>>,
    agent_input_tokens: u64,
    agent_output_tokens: u64,
    agent_total_tokens: u64,
    last_reported_input_tokens: u64,
    last_reported_output_tokens: u64,
    last_reported_total_tokens: u64,
    retry_attempt: Option<u32>,
    started_at: DateTime<Utc>,
    pending_cleanup: bool,
    turn_count: u32,
}

#[derive(Debug)]
struct RetryEntry {
    issue_id: String,
    identifier: String,
    attempt: u32,
    _due_at: DateTime<Utc>,
    _error: Option<String>,
    task: JoinHandle<()>,
}

#[derive(Debug, Clone, Copy)]
enum RetryDelay {
    Continuation,
    Backoff,
}

// ─── Asahi auto-start helpers ───────────────────────────────────────────────

async fn find_available_port() -> Result<u16> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    drop(listener);
    Ok(addr.port())
}

async fn wait_for_asahi(endpoint: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let health_url = format!("{}/api/issues", endpoint);

    for attempt in 0..60 {
        match client.get(&health_url).send().await {
            Ok(response) if response.status().is_success() => return Ok(()),
            _ => {
                if attempt >= 59 {
                    return Err(LunaError::Tracker(
                        "asahi failed to start within timeout".to_string(),
                    ));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    Ok(())
}
