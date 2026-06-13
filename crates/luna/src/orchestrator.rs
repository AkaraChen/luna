use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tokio::{
    signal,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
    time::{Duration, Instant, interval, interval_at},
};
use tracing::{debug, error, info, warn};

use crate::{
    agent::{StopReason, UsageUpdate, WorkerEvent, WorkerExit, WorkerOutcome, run_agent_attempt},
    config::{ServiceConfig, TrackerConfig},
    error::{LunaError, Result},
    model::{Issue, TokenTotals},
    shell_command::ShellActivityInspection,
    tracker::build_tracker,
    workflow::{LoadedWorkflow, WorkflowStore, parse_workflow_definition},
    workspace::WorkspaceManager,
};

pub async fn run(workflow_path: PathBuf) -> Result<()> {
    let mut store = WorkflowStore::load(workflow_path.clone())?;

    let raw_contents = tokio::fs::read_to_string(&workflow_path).await?;
    let raw_def = parse_workflow_definition(&raw_contents)?;
    let auto_start_asahi = !raw_def.config.contains_key("tracker");

    let mut embedded_asahi = start_embedded_asahi_if_needed(&mut store, auto_start_asahi).await?;

    let initial = store.current().clone();

    info!(
        tracker = ?std::mem::discriminant(&initial.config.tracker),
        runner = ?std::mem::discriminant(&initial.config.runner),
        interval_ms = initial.config.polling.interval_ms,
        max_concurrent = initial.config.scheduler.max_concurrent,
        shell_activity_patterns = ?initial.config.shell_activity_patterns,
        "luna orchestrator started"
    );
    if initial.config.shell_activity_patterns.is_empty() {
        warn!("shell activity tracking disabled; shell_activity_patterns is empty");
    } else {
        info!(
            shell_activity_patterns = ?initial.config.shell_activity_patterns,
            "shell activity tracking configured"
        );
    }

    let (events_tx, mut events_rx) = mpsc::unbounded_channel();

    let mut state = OrchestratorState::new(&initial.config);

    startup_terminal_workspace_cleanup(&initial).await;

    let mut ticker = interval(Duration::from_millis(
        initial.config.polling.interval_ms.max(1),
    ));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut comment_ticker = interval(Duration::from_secs(2));
    comment_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("received shutdown signal");
                shutdown_running(&mut state);
                if let Some(handle) = embedded_asahi.as_mut() {
                    handle.shutdown();
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
            _ = comment_ticker.tick() => {
                if let Err(err) = poll_comments(&mut store, &mut state, &events_tx).await {
                    error!(error = %err, "comment poll failed");
                }
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
            info!(
                workflow = %store.path().display(),
                shell_activity_patterns = ?store.current().config.shell_activity_patterns,
                "reloaded workflow configuration"
            );
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

    info!(
        candidate_count = candidates.len(),
        running = state.running.len(),
        claimed = state.claimed.len(),
        "poll tick"
    );

    // Group candidates by project slug, sort each group by created_at (oldest first)
    let mut by_project: HashMap<Option<String>, Vec<Issue>> = HashMap::new();
    for issue in candidates {
        let key = issue.project.as_ref().map(|p| p.slug.clone());
        by_project.entry(key).or_default().push(issue);
    }
    for issues in by_project.values_mut() {
        issues.sort_by(sort_issues_for_dispatch);
    }

    // Determine dispatch order:
    // 1. If there are running issues, prefer the same project to batch work
    // 2. Otherwise pick the project with the oldest issue
    let active_project = state
        .running
        .values()
        .next()
        .and_then(|e| e.issue.project.as_ref().map(|p| p.slug.clone()));

    let mut ordered_issues = Vec::new();
    if let Some(slug) = active_project {
        if let Some(issues) = by_project.remove(&Some(slug)) {
            ordered_issues.extend(issues);
        }
    }

    // Sort remaining projects by their oldest issue's created_at
    let mut remaining: Vec<_> = by_project.into_iter().collect();
    remaining.sort_by(|(_, a), (_, b)| {
        let a_oldest = a.first().and_then(|i| i.created_at);
        let b_oldest = b.first().and_then(|i| i.created_at);
        a_oldest.cmp(&b_oldest)
    });
    for (_, issues) in remaining {
        ordered_issues.extend(issues);
    }

    let mut dispatched = 0;
    for issue in ordered_issues {
        if available_global_slots(state, &workflow.config) == 0 {
            info!("no available global slots, skipping remaining issues");
            break;
        }
        if should_dispatch(&issue, state, &workflow.config) {
            info!(issue_id = %issue.id, identifier = %issue.identifier, state = %issue.state, project = ?issue.project.as_ref().map(|p| &p.slug), "dispatching agent");
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
        WorkerEvent::CommandExecuted(cmd) => {
            handle_command_executed(cmd, store, state).await;
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
    entry.last_reported_input_tokens = entry.last_reported_input_tokens.max(usage.input_tokens);
    entry.last_reported_output_tokens = entry.last_reported_output_tokens.max(usage.output_tokens);
    entry.last_reported_total_tokens = entry.last_reported_total_tokens.max(usage.total_tokens);

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

async fn handle_command_executed(
    cmd: crate::agent::CommandExecutionEvent,
    store: &WorkflowStore,
    state: &mut OrchestratorState,
) {
    let Some(entry) = state.running.get(&cmd.issue_id) else {
        warn!(
            issue_id = %cmd.issue_id,
            identifier = %cmd.issue_identifier,
            command = %cmd.command,
            "command activity ignored because issue is no longer running"
        );
        return;
    };

    let config = &store.current().config;
    let activity_match = match crate::shell_command::inspect_shell_activity(
        &cmd.command,
        &config.shell_activity_patterns,
    ) {
        ShellActivityInspection::Matched(activity_match) => activity_match,
        ShellActivityInspection::NoPatterns => {
            debug!(
                issue_id = %cmd.issue_id,
                identifier = %cmd.issue_identifier,
                command = %cmd.command,
                "command activity inspection skipped because no patterns are configured"
            );
            return;
        }
        ShellActivityInspection::ParseFailed => {
            warn!(
                issue_id = %cmd.issue_id,
                identifier = %cmd.issue_identifier,
                command = %cmd.command,
                shell_activity_patterns = ?config.shell_activity_patterns,
                "command activity inspection failed to parse shell command"
            );
            return;
        }
        ShellActivityInspection::NoMatch { parsed_commands } => {
            debug!(
                issue_id = %cmd.issue_id,
                identifier = %cmd.issue_identifier,
                command = %cmd.command,
                parsed_commands = ?parsed_commands,
                shell_activity_patterns = ?config.shell_activity_patterns,
                "command activity did not match configured patterns"
            );
            return;
        }
    };

    info!(
        issue_id = %cmd.issue_id,
        identifier = %cmd.issue_identifier,
        command = %cmd.command,
        matched_pattern = %activity_match.pattern,
        matched_tokens = ?activity_match.command_tokens,
        cwd = cmd.cwd.as_deref().unwrap_or("unknown"),
        duration_ms = cmd.duration_ms,
        exit_code = cmd.exit_code,
        "command activity matched"
    );

    let tracker = match build_tracker(&config.tracker) {
        Ok(t) => t,
        Err(err) => {
            warn!(error = %err, "failed to build tracker for command activity");
            return;
        }
    };

    let body = format!("🤖 Agent executed: `{}`", cmd.command);

    if let Err(err) = tracker.create_comment(&entry.issue, &body).await {
        warn!(
            issue_id = %cmd.issue_id,
            error = %err,
            "failed to create comment for command activity"
        );
    } else {
        info!(
            issue_id = %cmd.issue_id,
            identifier = %cmd.issue_identifier,
            matched_pattern = %activity_match.pattern,
            "created tracker comment for command activity"
        );
    }

    if let Err(err) = tracker
        .create_activity(
            &entry.issue,
            "command_executed",
            &format!("Agent executed: {}", cmd.command),
            Some(&body),
        )
        .await
    {
        warn!(
            issue_id = %cmd.issue_id,
            error = %err,
            "failed to create activity for command activity"
        );
    } else {
        info!(
            issue_id = %cmd.issue_id,
            identifier = %cmd.issue_identifier,
            matched_pattern = %activity_match.pattern,
            "created tracker activity for command activity"
        );
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
    let (comment_tx, comment_rx) = mpsc::channel::<String>(16);
    let worker = tokio::spawn(run_agent_attempt(
        issue.clone(),
        attempt,
        workflow,
        events_tx.clone(),
        stop_rx,
        comment_rx,
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
            comment_tx: Some(comment_tx),
            seen_comment_ids: HashSet::new(),
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
    // Skip issues blocked by unresolved blockers regardless of state
    if issue.blocked_by.iter().any(|blocker| {
        blocker
            .state
            .as_deref()
            .map(|state| !config.tracker.is_terminal_state(state))
            .unwrap_or(true)
    }) {
        return false;
    }
    true
}

fn sort_issues_for_dispatch(left: &Issue, right: &Issue) -> Ordering {
    // Oldest first, then by priority, then by identifier
    match left.created_at.cmp(&right.created_at) {
        Ordering::Equal => match compare_priority(left.priority, right.priority) {
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

async fn poll_comments(
    store: &mut WorkflowStore,
    state: &mut OrchestratorState,
    _events_tx: &mpsc::UnboundedSender<WorkerEvent>,
) -> Result<()> {
    if state.running.is_empty() {
        return Ok(());
    }

    let workflow = store.current().clone();
    let tracker = build_tracker(&workflow.config.tracker)?;

    for entry in state.running.values_mut() {
        let comments = match tracker.fetch_comments(&entry.issue).await {
            Ok(c) => c,
            Err(err) => {
                warn!(issue_id = %entry.issue.id, error = %err, "failed to fetch comments");
                continue;
            }
        };

        for comment in comments {
            if entry.seen_comment_ids.insert(comment.id.clone()) {
                if let Some(tx) = entry.comment_tx.take() {
                    let _ = tx.send(comment.body).await;
                    entry.comment_tx = Some(tx);
                }
            }
        }
    }

    Ok(())
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
    comment_tx: Option<mpsc::Sender<String>>,
    seen_comment_ids: HashSet<String>,
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

#[derive(Debug)]
struct EmbeddedAsahiHandle {
    #[cfg(test)]
    endpoint: String,
    #[cfg(test)]
    db_path: PathBuf,
    shutdown: Option<oneshot::Sender<()>>,
}

impl EmbeddedAsahiHandle {
    fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

async fn start_embedded_asahi_if_needed(
    store: &mut WorkflowStore,
    auto_start_asahi: bool,
) -> Result<Option<EmbeddedAsahiHandle>> {
    let should_embed = auto_start_asahi
        || matches!(
            &store.current().config.tracker,
            TrackerConfig::Asahi(cfg) if cfg.db.is_some()
        );

    if !should_embed {
        return Ok(None);
    }

    let port = match &store.current().config.tracker {
        TrackerConfig::Asahi(cfg) if cfg.port.is_some() => cfg.port.unwrap(),
        _ => find_available_port().await?,
    };
    let db_path = embedded_asahi_db_path(&store.current().config);
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let rocket = asahi::rocket_with_database_url_and_port(db_url, port);

    let (tx, rx) = oneshot::channel::<()>();
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

    let endpoint = format!("http://127.0.0.1:{port}");
    wait_for_asahi(&endpoint).await?;

    if let TrackerConfig::Asahi(ref mut config) = store.current_mut().config.tracker {
        config.endpoint = endpoint.clone();
    }

    info!("embedded asahi started on port {}", port);

    Ok(Some(EmbeddedAsahiHandle {
        #[cfg(test)]
        endpoint,
        #[cfg(test)]
        db_path,
        shutdown: Some(tx),
    }))
}

fn embedded_asahi_db_path(config: &ServiceConfig) -> PathBuf {
    let db_path = match &config.tracker {
        TrackerConfig::Asahi(cfg) => cfg
            .db
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("asahi.db")),
        _ => PathBuf::from("asahi.db"),
    };

    if db_path.is_absolute() {
        db_path
    } else {
        config.workflow_dir.join(db_path)
    }
}

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

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        time::Duration as StdDuration,
    };

    use chrono::{Duration as ChronoDuration, TimeZone};
    use serde_json::{Value, json};
    use tokio::sync::{mpsc, watch};

    use crate::{
        agent::{CommandExecutionEvent, SessionUpdate, WorkerEvent, WorkerExit, WorkerOutcome},
        config::{RunnerConfig, ServiceConfig, TrackerConfig, resolve_service_config},
        model::{BlockerRef, Issue, ProjectRef, WorkflowDefinition},
        test_support::{MockHttpServer, MockResponse, issue_json},
        workflow::WorkflowStore,
    };

    use super::{
        OrchestratorState, RetryEntry, RunningEntry, StopReason, UsageUpdate, apply_session_update,
        apply_usage_update, compare_priority, embedded_asahi_db_path, handle_command_executed,
        handle_retry_due, handle_worker_event, handle_worker_exit, has_available_state_slot,
        on_tick, poll_comments, reconcile_running_issues, reconcile_stalled_runs, should_dispatch,
        sort_issues_for_dispatch, start_embedded_asahi_if_needed,
        startup_terminal_workspace_cleanup,
    };

    fn codex_config(tracker_yaml: &str, scheduler_yaml: &str, runner_yaml: &str) -> ServiceConfig {
        let yaml = serde_yaml::from_str(&format!(
            r#"
tracker:
{tracker_yaml}
runner:
  kind: codex
{runner_yaml}
scheduler:
{scheduler_yaml}
"#
        ))
        .unwrap();
        let definition = WorkflowDefinition {
            config: yaml,
            prompt_template: "hello".to_string(),
        };
        let config = resolve_service_config(&definition, Path::new("/tmp/WORKFLOW.md")).unwrap();
        assert!(matches!(config.runner, RunnerConfig::Codex(_)));
        config
    }

    fn github_codex_config() -> ServiceConfig {
        codex_config(
            "  kind: github_project\n  owner: acme\n  project_number: 12",
            "  max_concurrent: 2",
            "",
        )
    }

    fn asahi_codex_config() -> ServiceConfig {
        codex_config("  kind: asahi\n  db: ./asahi.db", "  max_concurrent: 2", "")
    }

    fn asahi_endpoint_codex_workflow(
        endpoint: &str,
        workspace_root: PathBuf,
    ) -> (tempfile::TempDir, WorkflowStore) {
        let temp = tempfile::tempdir().expect("tempdir");
        let workflow_path = temp.path().join("WORKFLOW.md");
        write_asahi_endpoint_codex_workflow(&workflow_path, endpoint, &workspace_root, 2, 30_000);
        let store = WorkflowStore::load(workflow_path).expect("workflow");
        (temp, store)
    }

    fn write_asahi_endpoint_codex_workflow(
        workflow_path: &Path,
        endpoint: &str,
        workspace_root: &Path,
        max_concurrent: usize,
        interval_ms: u64,
    ) {
        let workspace_root = workspace_root
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        std::fs::write(
            workflow_path,
            format!(
                r#"---
tracker:
  kind: asahi
  endpoint: "{endpoint}"
workspace:
  root: "{workspace_root}"
hooks:
  timeout_ms: 1000
polling:
  interval_ms: {interval_ms}
scheduler:
  max_concurrent: {max_concurrent}
  retry_backoff_ms: 10000
runner:
  kind: codex
  command: '"codex'
shell_activity_patterns:
  - git commit
---
Issue {{{{ issue.identifier }}}}: {{{{ issue.title }}}}
"#,
            ),
        )
        .expect("write workflow");
    }

    struct FakeGh {
        _dir: tempfile::TempDir,
        command: String,
        log_path: PathBuf,
    }

    fn fake_github_project_gh() -> FakeGh {
        let dir = tempfile::tempdir().unwrap();
        let gh_path = dir.path().join("gh");
        let log_path = dir.path().join("calls.log");
        let response = json!({
            "data": {
                "repositoryOwner": {
                    "projectV2": {
                        "url": "https://github.com/orgs/acme/projects/12",
                        "items": {
                            "pageInfo": {
                                "hasNextPage": false,
                                "endCursor": null
                            },
                            "nodes": [
                                {
                                    "id": "PVTI_1",
                                    "createdAt": "2026-01-01T00:00:00Z",
                                    "updatedAt": "2026-01-02T00:00:00Z",
                                    "statusFieldValue": {
                                        "__typename": "ProjectV2ItemFieldSingleSelectValue",
                                        "name": "Todo"
                                    },
                                    "priorityFieldValue": {
                                        "__typename": "ProjectV2ItemFieldTextValue",
                                        "text": "P1"
                                    },
                                    "content": {
                                        "__typename": "Issue",
                                        "id": "I_42",
                                        "number": 42,
                                        "title": "Fix GitHub workflow",
                                        "body": "Body",
                                        "url": "https://github.com/acme/repo/issues/42",
                                        "state": "OPEN",
                                        "closed": false,
                                        "createdAt": "2026-01-01T00:00:00Z",
                                        "updatedAt": "2026-01-02T00:00:00Z",
                                        "repository": {
                                            "nameWithOwner": "acme/repo"
                                        },
                                        "labels": {
                                            "nodes": [
                                                { "name": "CI" }
                                            ]
                                        }
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        })
        .to_string();
        let script = format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
CALL_LOG='{log_path}'
printf '%q ' "$@" >> "$CALL_LOG"
printf '\n' >> "$CALL_LOG"
if [[ "${{1:-}}" == "api" && "${{2:-}}" == "graphql" ]]; then
  cat <<'JSON'
{response}
JSON
else
  echo "unexpected gh invocation: $*" >&2
  exit 64
fi
"#,
            log_path = log_path.display(),
        );
        std::fs::write(&gh_path, script).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&gh_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&gh_path, permissions).unwrap();
        }

        FakeGh {
            _dir: dir,
            command: gh_path.to_string_lossy().to_string(),
            log_path,
        }
    }

    fn fake_github_comments_gh() -> FakeGh {
        let dir = tempfile::tempdir().unwrap();
        let gh_path = dir.path().join("gh");
        let log_path = dir.path().join("calls.log");
        let count_path = dir.path().join("comment-count");
        let script = format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
CALL_LOG='{log_path}'
COUNT_FILE='{count_path}'
printf '%q ' "$@" >> "$CALL_LOG"
printf '\n' >> "$CALL_LOG"
if [[ "${{1:-}}" == "api" && "${{2:-}}" == "repos/acme/repo/issues/42/comments" ]]; then
  count=0
  if [[ -f "$COUNT_FILE" ]]; then
    count="$(cat "$COUNT_FILE")"
  fi
  count=$((count + 1))
  printf '%s' "$count" > "$COUNT_FILE"
  if [[ "$count" == "1" ]]; then
    cat <<'JSON'
[
  {{"node_id":"comment-1","body":"first github comment","created_at":"2026-01-01T00:00:00Z"}},
  {{"id":"comment-2","body":"second github comment","created_at":"2026-01-02T00:00:00Z"}}
]
JSON
  else
    cat <<'JSON'
[
  {{"node_id":"comment-1","body":"first github comment","created_at":"2026-01-01T00:00:00Z"}},
  {{"node_id":"comment-3","body":"third github comment","created_at":"2026-01-03T00:00:00Z"}}
]
JSON
  fi
else
  echo "unexpected gh invocation: $*" >&2
  exit 64
fi
"#,
            log_path = log_path.display(),
            count_path = count_path.display(),
        );
        std::fs::write(&gh_path, script).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&gh_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&gh_path, permissions).unwrap();
        }

        FakeGh {
            _dir: dir,
            command: gh_path.to_string_lossy().to_string(),
            log_path,
        }
    }

    fn fake_github_issue_comment_gh() -> FakeGh {
        let dir = tempfile::tempdir().unwrap();
        let gh_path = dir.path().join("gh");
        let log_path = dir.path().join("calls.log");
        let script = format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
CALL_LOG='{log_path}'
printf '%q ' "$@" >> "$CALL_LOG"
printf '\n' >> "$CALL_LOG"
if [[ "${{1:-}}" == "issue" && "${{2:-}}" == "comment" ]]; then
  exit 0
else
  echo "unexpected gh invocation: $*" >&2
  exit 64
fi
"#,
            log_path = log_path.display(),
        );
        std::fs::write(&gh_path, script).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&gh_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&gh_path, permissions).unwrap();
        }

        FakeGh {
            _dir: dir,
            command: gh_path.to_string_lossy().to_string(),
            log_path,
        }
    }

    fn write_github_project_codex_workflow(
        workflow_path: &Path,
        gh_command: &str,
        workspace_root: &Path,
    ) {
        write_github_project_codex_workflow_with_patterns(
            workflow_path,
            gh_command,
            workspace_root,
            &["gh pr create"],
        );
    }

    fn write_github_project_codex_workflow_with_default_patterns(
        workflow_path: &Path,
        gh_command: &str,
        workspace_root: &Path,
    ) {
        write_github_project_codex_workflow_inner(workflow_path, gh_command, workspace_root, None);
    }

    fn write_github_project_codex_workflow_with_patterns(
        workflow_path: &Path,
        gh_command: &str,
        workspace_root: &Path,
        shell_activity_patterns: &[&str],
    ) {
        write_github_project_codex_workflow_inner(
            workflow_path,
            gh_command,
            workspace_root,
            Some(shell_activity_patterns),
        );
    }

    fn write_github_project_codex_workflow_inner(
        workflow_path: &Path,
        gh_command: &str,
        workspace_root: &Path,
        shell_activity_patterns: Option<&[&str]>,
    ) {
        let gh_command = gh_command.replace('\\', "\\\\").replace('"', "\\\"");
        let workspace_root = workspace_root
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let shell_activity_yaml = match shell_activity_patterns {
            Some([]) => "shell_activity_patterns: []\n".to_string(),
            Some(shell_activity_patterns) => {
                let patterns = shell_activity_patterns
                    .iter()
                    .map(|pattern| format!("  - {pattern}\n"))
                    .collect::<String>();
                format!("shell_activity_patterns:\n{patterns}")
            }
            None => String::new(),
        };
        std::fs::write(
            workflow_path,
            format!(
                r#"---
tracker:
  kind: github_project
  owner: acme
  project_number: 12
  gh_command: "{gh_command}"
workspace:
  root: "{workspace_root}"
hooks:
  timeout_ms: 1000
scheduler:
  max_concurrent: 2
  retry_backoff_ms: 10000
runner:
  kind: codex
  command: '"codex'
{shell_activity_yaml}
---
Issue {{{{ issue.identifier }}}}: {{{{ issue.title }}}}
"#,
            ),
        )
        .expect("write workflow");
    }

    fn issue(id: &str, identifier: &str, state: &str) -> Issue {
        Issue {
            id: id.to_string(),
            identifier: identifier.to_string(),
            title: format!("Issue {identifier}"),
            description: None,
            priority: None,
            state: state.to_string(),
            branch_name: None,
            url: None,
            labels: Vec::new(),
            blocked_by: Vec::new(),
            created_at: None,
            updated_at: None,
            project: None,
            source_data: None,
        }
    }

    fn issue_with_project(id: &str, identifier: &str, state: &str, project_slug: &str) -> Issue {
        let mut issue = issue(id, identifier, state);
        issue.project = Some(ProjectRef {
            id: format!("project-{project_slug}"),
            slug: project_slug.to_string(),
            name: project_slug.to_string(),
            state: "Active".to_string(),
            priority: None,
        });
        issue
    }

    fn running_entry(issue: Issue) -> (RunningEntry, watch::Receiver<Option<StopReason>>) {
        let (stop_tx, stop_rx) = watch::channel(None);
        let (comment_tx, _comment_rx) = mpsc::channel(1);
        let worker = tokio::spawn(async {
            std::future::pending::<()>().await;
        });
        let entry = RunningEntry {
            worker,
            stop_tx,
            comment_tx: Some(comment_tx),
            seen_comment_ids: Default::default(),
            identifier: issue.identifier.clone(),
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
            retry_attempt: None,
            started_at: chrono::Utc::now(),
            pending_cleanup: false,
            turn_count: 0,
        };
        (entry, stop_rx)
    }

    fn running_entry_with_comment_rx(
        issue: Issue,
    ) -> (
        RunningEntry,
        mpsc::Receiver<String>,
        watch::Receiver<Option<StopReason>>,
    ) {
        let (stop_tx, stop_rx) = watch::channel(None);
        let (comment_tx, comment_rx) = mpsc::channel(8);
        let worker = tokio::spawn(async {
            std::future::pending::<()>().await;
        });
        let entry = RunningEntry {
            worker,
            stop_tx,
            comment_tx: Some(comment_tx),
            seen_comment_ids: Default::default(),
            identifier: issue.identifier.clone(),
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
            retry_attempt: None,
            started_at: chrono::Utc::now(),
            pending_cleanup: false,
            turn_count: 0,
        };
        (entry, comment_rx, stop_rx)
    }

    fn retry_entry(issue_id: &str, identifier: &str, attempt: u32) -> RetryEntry {
        RetryEntry {
            issue_id: issue_id.to_string(),
            identifier: identifier.to_string(),
            attempt,
            _due_at: chrono::Utc::now(),
            _error: None,
            task: tokio::spawn(async {
                std::future::pending::<()>().await;
            }),
        }
    }

    #[test]
    fn codex_configs_cover_github_and_asahi_tracker_state_semantics() {
        let github = github_codex_config();
        assert!(matches!(github.tracker, TrackerConfig::GitHubProject(_)));
        assert!(github.tracker.is_active_state("todo"));
        assert!(github.tracker.is_active_state("IN PROGRESS"));
        assert!(github.tracker.is_terminal_state("done"));
        assert!(!github.tracker.is_active_state("Backlog"));

        let asahi = asahi_codex_config();
        assert!(matches!(asahi.tracker, TrackerConfig::Asahi(_)));
        assert!(asahi.tracker.is_active_state("Todo"));
        assert!(asahi.tracker.is_active_state("in progress"));
        assert!(asahi.tracker.is_terminal_state("Done"));
        assert!(!asahi.tracker.is_active_state("Backlog"));
    }

    #[tokio::test]
    async fn should_dispatch_accepts_valid_codex_github_and_asahi_issues() {
        let github = github_codex_config();
        let asahi = asahi_codex_config();
        let github_state = OrchestratorState::new(&github);
        let asahi_state = OrchestratorState::new(&asahi);

        assert!(should_dispatch(
            &issue("1", "acme/repo#1", "Todo"),
            &github_state,
            &github
        ));
        assert!(should_dispatch(
            &issue("2", "ASAHI-2", "In Progress"),
            &asahi_state,
            &asahi
        ));
    }

    #[tokio::test]
    async fn should_dispatch_rejects_invalid_or_unavailable_codex_work() {
        let config = github_codex_config();
        let mut state = OrchestratorState::new(&config);

        assert!(!should_dispatch(
            &issue("", "acme/repo#1", "Todo"),
            &state,
            &config
        ));
        assert!(!should_dispatch(&issue("1", "", "Todo"), &state, &config));
        assert!(!should_dispatch(
            &issue("1", "acme/repo#1", "Backlog"),
            &state,
            &config
        ));
        assert!(!should_dispatch(
            &issue("1", "acme/repo#1", "Done"),
            &state,
            &config
        ));

        state.claimed.insert("1".to_string());
        assert!(!should_dispatch(
            &issue("1", "acme/repo#1", "Todo"),
            &state,
            &config
        ));
        state.claimed.clear();

        let (entry, _stop_rx) = running_entry(issue("1", "acme/repo#1", "Todo"));
        state.running.insert("1".to_string(), entry);
        assert!(!should_dispatch(
            &issue("1", "acme/repo#1", "Todo"),
            &state,
            &config
        ));
        state.running.get("1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn should_dispatch_respects_global_and_state_concurrency_limits() {
        let config = codex_config(
            "  kind: github_project\n  owner: acme\n  project_number: 12",
            "  max_concurrent: 1\n  max_concurrent_by_state:\n    Todo: 1",
            "",
        );
        let mut state = OrchestratorState::new(&config);
        let (entry, _stop_rx) = running_entry(issue("1", "acme/repo#1", "Todo"));
        state.running.insert("1".to_string(), entry);

        assert_eq!(super::available_global_slots(&state, &config), 0);
        assert!(!has_available_state_slot("Todo", &state, &config));
        assert!(!should_dispatch(
            &issue("2", "acme/repo#2", "Todo"),
            &state,
            &config
        ));
        state.running.get("1").unwrap().worker.abort();
    }

    #[test]
    fn should_dispatch_rejects_unresolved_blockers_only() {
        let config = github_codex_config();
        let state = OrchestratorState::new(&config);
        let mut blocked = issue("1", "acme/repo#1", "Todo");
        blocked.blocked_by = vec![BlockerRef {
            id: Some("blocker".to_string()),
            identifier: Some("acme/repo#0".to_string()),
            state: Some("Todo".to_string()),
        }];
        assert!(!should_dispatch(&blocked, &state, &config));

        blocked.blocked_by[0].state = Some("Done".to_string());
        assert!(should_dispatch(&blocked, &state, &config));

        blocked.blocked_by[0].state = None;
        assert!(!should_dispatch(&blocked, &state, &config));
    }

    #[test]
    fn sort_dispatches_oldest_then_highest_priority_then_identifier() {
        let mut oldest_low = issue("1", "C", "Todo");
        oldest_low.created_at = Some(chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        oldest_low.priority = Some(3);

        let mut oldest_high_b = issue("2", "B", "Todo");
        oldest_high_b.created_at = Some(chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        oldest_high_b.priority = Some(1);

        let mut oldest_high_a = issue("3", "A", "Todo");
        oldest_high_a.created_at = Some(chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        oldest_high_a.priority = Some(1);

        let mut newer = issue("4", "D", "Todo");
        newer.created_at = Some(chrono::Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap());
        newer.priority = Some(0);

        let mut issues = vec![newer, oldest_low, oldest_high_b, oldest_high_a];
        issues.sort_by(sort_issues_for_dispatch);

        assert_eq!(
            issues
                .into_iter()
                .map(|issue| issue.identifier)
                .collect::<Vec<_>>(),
            vec!["A", "B", "C", "D"]
        );
        assert_eq!(compare_priority(Some(1), Some(2)), std::cmp::Ordering::Less);
        assert_eq!(compare_priority(Some(1), None), std::cmp::Ordering::Less);
        assert_eq!(compare_priority(None, Some(1)), std::cmp::Ordering::Greater);
    }

    #[tokio::test]
    async fn reconcile_stalled_runs_sends_stalled_stop_for_codex_runner() {
        let config = codex_config(
            "  kind: github_project\n  owner: acme\n  project_number: 12",
            "  max_concurrent: 1",
            "  stall_timeout_ms: 1",
        );
        let mut state = OrchestratorState::new(&config);
        let (mut entry, stop_rx) = running_entry(issue("1", "acme/repo#1", "Todo"));
        entry.started_at = chrono::Utc::now() - ChronoDuration::milliseconds(10);
        state.running.insert("1".to_string(), entry);

        reconcile_stalled_runs(&mut state, &config);

        assert!(matches!(
            stop_rx.borrow().clone(),
            Some(StopReason::Stalled)
        ));
        state.running.get("1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn usage_updates_count_only_new_token_deltas() {
        let config = github_codex_config();
        let mut state = OrchestratorState::new(&config);
        let (entry, _stop_rx) = running_entry(issue("1", "acme/repo#1", "Todo"));
        state.running.insert("1".to_string(), entry);

        apply_usage_update(
            &mut state,
            "1",
            UsageUpdate {
                input_tokens: 100,
                output_tokens: 20,
                total_tokens: 120,
            },
        );
        apply_usage_update(
            &mut state,
            "1",
            UsageUpdate {
                input_tokens: 150,
                output_tokens: 25,
                total_tokens: 175,
            },
        );
        apply_usage_update(
            &mut state,
            "1",
            UsageUpdate {
                input_tokens: 140,
                output_tokens: 22,
                total_tokens: 162,
            },
        );
        apply_usage_update(
            &mut state,
            "1",
            UsageUpdate {
                input_tokens: 151,
                output_tokens: 26,
                total_tokens: 177,
            },
        );

        assert_eq!(state.agent_totals.input_tokens, 151);
        assert_eq!(state.agent_totals.output_tokens, 26);
        assert_eq!(state.agent_totals.total_tokens, 177);
        state.running.get("1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn session_updates_refresh_running_entry_metadata() {
        let config = github_codex_config();
        let mut state = OrchestratorState::new(&config);
        let (entry, _stop_rx) = running_entry(issue("1", "acme/repo#1", "Todo"));
        state.running.insert("1".to_string(), entry);

        apply_session_update(
            crate::agent::SessionUpdate {
                issue_id: "1".to_string(),
                issue_identifier: "acme/repo#1".to_string(),
                event: "turn/started".to_string(),
                timestamp: chrono::Utc::now(),
                session_id: Some("session".to_string()),
                thread_id: Some("thread".to_string()),
                turn_id: Some("turn".to_string()),
                agent_pid: Some(123),
                message: Some("delta".to_string()),
                usage: Some(UsageUpdate {
                    input_tokens: 10,
                    output_tokens: 2,
                    total_tokens: 12,
                }),
                rate_limits: Some(serde_json::json!({"remaining": 1})),
                turn_count: Some(2),
            },
            &mut state,
        );

        let running = state.running.get("1").unwrap();
        assert_eq!(running.last_agent_event.as_deref(), Some("turn/started"));
        assert_eq!(running.session_id.as_deref(), Some("session"));
        assert_eq!(running.thread_id.as_deref(), Some("thread"));
        assert_eq!(running.turn_id.as_deref(), Some("turn"));
        assert_eq!(running.agent_pid, Some(123));
        assert_eq!(running.last_agent_message.as_deref(), Some("delta"));
        assert_eq!(running.turn_count, 2);
        assert_eq!(state.agent_totals.total_tokens, 12);
        assert_eq!(
            state.agent_rate_limits,
            Some(serde_json::json!({"remaining": 1}))
        );
        running.worker.abort();
    }

    #[tokio::test]
    async fn worker_event_entry_routes_codex_session_command_and_exit() {
        let fake = fake_github_issue_comment_gh();
        let workflow_temp = tempfile::tempdir().unwrap();
        let workspace_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        write_github_project_codex_workflow(
            &workflow_path,
            &fake.command,
            &workspace_temp.path().join("workspaces"),
        );
        let mut store = WorkflowStore::load(workflow_path).expect("workflow");
        let mut state = OrchestratorState::new(&store.current().config);
        let (entry, _stop_rx) = running_entry(issue("PVTI_1", "acme/repo#42", "Todo"));
        state.running.insert("PVTI_1".to_string(), entry);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        handle_worker_event(
            WorkerEvent::Session(SessionUpdate {
                issue_id: "PVTI_1".to_string(),
                issue_identifier: "acme/repo#42".to_string(),
                event: "turn/started".to_string(),
                timestamp: chrono::Utc::now(),
                session_id: Some("session".to_string()),
                thread_id: Some("thread".to_string()),
                turn_id: Some("turn".to_string()),
                agent_pid: Some(456),
                message: Some("ready".to_string()),
                usage: Some(UsageUpdate {
                    input_tokens: 33,
                    output_tokens: 7,
                    total_tokens: 40,
                }),
                rate_limits: Some(serde_json::json!({"remaining": 3})),
                turn_count: Some(1),
            }),
            &mut store,
            &mut state,
            &events_tx,
        )
        .await;

        let running = state.running.get("PVTI_1").unwrap();
        assert_eq!(running.session_id.as_deref(), Some("session"));
        assert_eq!(running.thread_id.as_deref(), Some("thread"));
        assert_eq!(running.turn_id.as_deref(), Some("turn"));
        assert_eq!(running.agent_pid, Some(456));
        assert_eq!(running.last_agent_message.as_deref(), Some("ready"));
        assert_eq!(state.agent_totals.total_tokens, 40);

        handle_worker_event(
            WorkerEvent::CommandExecuted(CommandExecutionEvent {
                issue_id: "PVTI_1".to_string(),
                issue_identifier: "acme/repo#42".to_string(),
                command: "gh pr create -R acme/repo --fill".to_string(),
                cwd: Some("/tmp/work".to_string()),
                duration_ms: Some(300),
                exit_code: Some(0),
            }),
            &mut store,
            &mut state,
            &events_tx,
        )
        .await;

        let calls = std::fs::read_to_string(&fake.log_path).expect("fake gh log");
        assert!(calls.contains("issue comment 42"));

        handle_worker_event(
            WorkerEvent::Exited(WorkerExit {
                issue_id: "PVTI_1".to_string(),
                issue_identifier: "acme/repo#42".to_string(),
                outcome: WorkerOutcome::Normal,
                runtime_seconds: 1.25,
                error: None,
            }),
            &mut store,
            &mut state,
            &events_tx,
        )
        .await;

        assert!(!state.running.contains_key("PVTI_1"));
        assert!(state.completed.contains("PVTI_1"));
        assert_eq!(state.retry_attempts.get("PVTI_1").unwrap().attempt, 1);
        assert_eq!(state.agent_totals.seconds_running, 1.25);
        state.retry_attempts.get("PVTI_1").unwrap().task.abort();
    }

    #[tokio::test]
    async fn worker_event_entry_routes_codex_retry_due() {
        let server = MockHttpServer::spawn(vec![MockResponse::json(
            200,
            json!({
                "issues": [issue_json("1", "ASAHI-1", "Todo", None)]
            }),
        )])
        .await;
        let endpoint = server.endpoint.clone();
        let workspace_temp = tempfile::tempdir().unwrap();
        let (_temp, mut store) =
            asahi_endpoint_codex_workflow(&endpoint, workspace_temp.path().join("workspaces"));
        let mut state = OrchestratorState::new(&store.current().config);
        state.claimed.insert("1".to_string());
        state
            .retry_attempts
            .insert("1".to_string(), retry_entry("1", "ASAHI-1", 4));
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        handle_worker_event(
            WorkerEvent::RetryDue("1".to_string()),
            &mut store,
            &mut state,
            &events_tx,
        )
        .await;

        assert!(!state.retry_attempts.contains_key("1"));
        assert!(state.running.contains_key("1"));
        assert_eq!(state.running.get("1").unwrap().retry_attempt, Some(4));
        state.running.get("1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn worker_exit_outcomes_schedule_codex_retries_or_release_claims() {
        let workspace_temp = tempfile::tempdir().unwrap();
        let (_temp, store) = asahi_endpoint_codex_workflow(
            "http://127.0.0.1:9",
            workspace_temp.path().join("workspaces"),
        );
        let workflow = store.current().clone();
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let mut state = OrchestratorState::new(&workflow.config);

        let normal_workspace = workflow.config.workspace.root.join("ASAHI-1");
        tokio::fs::create_dir_all(&normal_workspace)
            .await
            .expect("workspace");
        let (mut normal_entry, _stop_rx) = running_entry(issue("1", "ASAHI-1", "Todo"));
        normal_entry.pending_cleanup = true;
        state.claimed.insert("1".to_string());
        state.running.insert("1".to_string(), normal_entry);

        handle_worker_exit(
            WorkerExit {
                issue_id: "1".to_string(),
                issue_identifier: "ASAHI-1".to_string(),
                outcome: WorkerOutcome::Normal,
                runtime_seconds: 1.5,
                error: None,
            },
            workflow.clone(),
            &mut state,
            &events_tx,
        )
        .await;

        assert!(state.running.get("1").is_none());
        assert!(state.completed.contains("1"));
        assert!(!normal_workspace.exists());
        assert_eq!(state.retry_attempts.get("1").unwrap().attempt, 1);
        assert_eq!(state.agent_totals.seconds_running, 1.5);

        let (mut failed_entry, _stop_rx) = running_entry(issue("2", "ASAHI-2", "Todo"));
        failed_entry.retry_attempt = Some(2);
        state.running.insert("2".to_string(), failed_entry);
        handle_worker_exit(
            WorkerExit {
                issue_id: "2".to_string(),
                issue_identifier: "ASAHI-2".to_string(),
                outcome: WorkerOutcome::Failed("boom".to_string()),
                runtime_seconds: 2.0,
                error: Some("boom".to_string()),
            },
            workflow.clone(),
            &mut state,
            &events_tx,
        )
        .await;
        let failed_retry = state.retry_attempts.get("2").unwrap();
        assert_eq!(failed_retry.attempt, 3);
        assert_eq!(failed_retry._error.as_deref(), Some("boom"));

        let (timeout_entry, _stop_rx) = running_entry(issue("3", "ASAHI-3", "Todo"));
        state.running.insert("3".to_string(), timeout_entry);
        handle_worker_exit(
            WorkerExit {
                issue_id: "3".to_string(),
                issue_identifier: "ASAHI-3".to_string(),
                outcome: WorkerOutcome::TimedOut,
                runtime_seconds: 3.0,
                error: Some("turn_timeout".to_string()),
            },
            workflow.clone(),
            &mut state,
            &events_tx,
        )
        .await;
        assert_eq!(
            state.retry_attempts.get("3").unwrap()._error.as_deref(),
            Some("turn_timeout")
        );

        let (stalled_entry, _stop_rx) = running_entry(issue("4", "ASAHI-4", "Todo"));
        state.running.insert("4".to_string(), stalled_entry);
        handle_worker_exit(
            WorkerExit {
                issue_id: "4".to_string(),
                issue_identifier: "ASAHI-4".to_string(),
                outcome: WorkerOutcome::Stalled,
                runtime_seconds: 4.0,
                error: Some("stalled".to_string()),
            },
            workflow.clone(),
            &mut state,
            &events_tx,
        )
        .await;
        assert_eq!(
            state.retry_attempts.get("4").unwrap()._error.as_deref(),
            Some("stalled")
        );

        let (canceled_entry, _stop_rx) = running_entry(issue("5", "ASAHI-5", "Todo"));
        state.claimed.insert("5".to_string());
        state.running.insert("5".to_string(), canceled_entry);
        handle_worker_exit(
            WorkerExit {
                issue_id: "5".to_string(),
                issue_identifier: "ASAHI-5".to_string(),
                outcome: WorkerOutcome::CanceledByReconciliation,
                runtime_seconds: 5.0,
                error: Some("canceled_by_reconciliation".to_string()),
            },
            workflow,
            &mut state,
            &events_tx,
        )
        .await;
        assert!(!state.claimed.contains("5"));
        assert!(!state.retry_attempts.contains_key("5"));

        for retry in state.retry_attempts.values() {
            retry.task.abort();
        }
    }

    #[tokio::test]
    async fn retry_due_dispatches_active_codex_issue_and_releases_missing_issue() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(
                200,
                json!({
                    "issues": [
                        issue_json("1", "ASAHI-1", "Todo", None),
                        issue_json("2", "ASAHI-2", "Done", None)
                    ]
                }),
            ),
            MockResponse::json(200, json!({ "issues": [] })),
        ])
        .await;
        let endpoint = server.endpoint.clone();
        let workspace_temp = tempfile::tempdir().unwrap();
        let (_temp, store) =
            asahi_endpoint_codex_workflow(&endpoint, workspace_temp.path().join("workspaces"));
        let workflow = store.current().clone();
        let (events_tx, _events_rx) = mpsc::unbounded_channel();
        let mut state = OrchestratorState::new(&workflow.config);
        state.claimed.insert("1".to_string());
        state
            .retry_attempts
            .insert("1".to_string(), retry_entry("1", "ASAHI-1", 2));

        handle_retry_due("1".to_string(), workflow.clone(), &mut state, &events_tx).await;

        assert!(state.retry_attempts.get("1").is_none());
        assert!(state.running.contains_key("1"));
        state.running.get("1").unwrap().worker.abort();

        state.claimed.insert("missing".to_string());
        state.retry_attempts.insert(
            "missing".to_string(),
            retry_entry("missing", "ASAHI-missing", 1),
        );
        handle_retry_due("missing".to_string(), workflow, &mut state, &events_tx).await;

        assert!(!state.claimed.contains("missing"));
        assert!(!state.retry_attempts.contains_key("missing"));
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 2);
        assert!(requests[0].target.starts_with("/api/issues?"));
        assert!(requests[1].target.starts_with("/api/issues?"));
    }

    #[tokio::test]
    async fn poll_comments_forwards_only_new_asahi_comments_to_running_codex() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(
                200,
                json!({
                    "comments": [
                        {
                            "id": "c1",
                            "issue_id": "1",
                            "body": "first",
                            "created_at": "2026-01-01T00:00:00Z"
                        },
                        {
                            "id": "c2",
                            "issue_id": "1",
                            "body": "second",
                            "created_at": "2026-01-02T00:00:00Z"
                        }
                    ]
                }),
            ),
            MockResponse::json(
                200,
                json!({
                    "comments": [
                        {
                            "id": "c1",
                            "issue_id": "1",
                            "body": "first",
                            "created_at": "2026-01-01T00:00:00Z"
                        },
                        {
                            "id": "c3",
                            "issue_id": "1",
                            "body": "third",
                            "created_at": "2026-01-03T00:00:00Z"
                        }
                    ]
                }),
            ),
        ])
        .await;
        let endpoint = server.endpoint.clone();
        let workspace_temp = tempfile::tempdir().unwrap();
        let (_temp, mut store) =
            asahi_endpoint_codex_workflow(&endpoint, workspace_temp.path().join("workspaces"));
        let mut state = OrchestratorState::new(&store.current().config);
        let (entry, mut comment_rx, _stop_rx) =
            running_entry_with_comment_rx(issue("1", "ASAHI-1", "Todo"));
        state.running.insert("1".to_string(), entry);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        poll_comments(&mut store, &mut state, &events_tx)
            .await
            .expect("poll comments");
        poll_comments(&mut store, &mut state, &events_tx)
            .await
            .expect("poll comments again");

        assert_eq!(comment_rx.try_recv().unwrap(), "first");
        assert_eq!(comment_rx.try_recv().unwrap(), "second");
        assert_eq!(comment_rx.try_recv().unwrap(), "third");
        assert!(comment_rx.try_recv().is_err());
        let requests = server.recorded_requests().await;
        assert_eq!(requests[0].target, "/api/issues/1/comments");
        assert_eq!(requests[1].target, "/api/issues/1/comments");
        state.running.get("1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn poll_comments_ignores_fetch_errors_and_missing_codex_comment_sender() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(500, json!({"error": "boom"})),
            MockResponse::json(
                200,
                json!({
                    "comments": [
                        {
                            "id": "c1",
                            "issue_id": "1",
                            "body": "not delivered",
                            "created_at": "2026-01-01T00:00:00Z"
                        }
                    ]
                }),
            ),
        ])
        .await;
        let endpoint = server.endpoint.clone();
        let workspace_temp = tempfile::tempdir().unwrap();
        let (_temp, mut store) =
            asahi_endpoint_codex_workflow(&endpoint, workspace_temp.path().join("workspaces"));
        let mut state = OrchestratorState::new(&store.current().config);
        let (entry, mut comment_rx, _stop_rx) =
            running_entry_with_comment_rx(issue("1", "ASAHI-1", "Todo"));
        state.running.insert("1".to_string(), entry);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        poll_comments(&mut store, &mut state, &events_tx)
            .await
            .expect("fetch error is logged but not returned");
        assert!(comment_rx.try_recv().is_err());
        assert!(state.running.get("1").unwrap().seen_comment_ids.is_empty());

        state.running.get_mut("1").unwrap().comment_tx = None;
        poll_comments(&mut store, &mut state, &events_tx)
            .await
            .expect("missing comment sender is ignored");

        assert!(comment_rx.try_recv().is_err());
        assert!(
            state
                .running
                .get("1")
                .unwrap()
                .seen_comment_ids
                .contains("c1")
        );
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].target, "/api/issues/1/comments");
        assert_eq!(requests[1].target, "/api/issues/1/comments");
        state.running.get("1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn poll_comments_forwards_only_new_github_comments_to_running_codex() {
        let fake = fake_github_comments_gh();
        let workflow_temp = tempfile::tempdir().unwrap();
        let workspace_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        write_github_project_codex_workflow(
            &workflow_path,
            &fake.command,
            &workspace_temp.path().join("workspaces"),
        );
        let mut store = WorkflowStore::load(workflow_path).expect("workflow");
        let mut state = OrchestratorState::new(&store.current().config);
        let (entry, mut comment_rx, _stop_rx) =
            running_entry_with_comment_rx(issue("PVTI_1", "acme/repo#42", "Todo"));
        state.running.insert("PVTI_1".to_string(), entry);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        poll_comments(&mut store, &mut state, &events_tx)
            .await
            .expect("poll github comments");
        poll_comments(&mut store, &mut state, &events_tx)
            .await
            .expect("poll github comments again");

        assert_eq!(comment_rx.try_recv().unwrap(), "first github comment");
        assert_eq!(comment_rx.try_recv().unwrap(), "second github comment");
        assert_eq!(comment_rx.try_recv().unwrap(), "third github comment");
        assert!(comment_rx.try_recv().is_err());
        let calls = std::fs::read_to_string(&fake.log_path).expect("fake gh log");
        assert_eq!(
            calls.matches("repos/acme/repo/issues/42/comments").count(),
            2
        );
        state.running.get("PVTI_1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn command_activity_posts_asahi_comment_and_activity_for_matching_codex_command() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(200, json!({})),
            MockResponse::json(200, json!({})),
        ])
        .await;
        let endpoint = server.endpoint.clone();
        let workspace_temp = tempfile::tempdir().unwrap();
        let (_temp, store) =
            asahi_endpoint_codex_workflow(&endpoint, workspace_temp.path().join("workspaces"));
        let mut state = OrchestratorState::new(&store.current().config);
        let (entry, _stop_rx) = running_entry(issue("1", "ASAHI-1", "Todo"));
        state.running.insert("1".to_string(), entry);

        handle_command_executed(
            CommandExecutionEvent {
                issue_id: "1".to_string(),
                issue_identifier: "ASAHI-1".to_string(),
                command: "git commit -m test".to_string(),
                cwd: Some("/tmp/work".to_string()),
                duration_ms: Some(1200),
                exit_code: Some(0),
            },
            &store,
            &mut state,
        )
        .await;

        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].target, "/api/issues/1/comments");
        assert_eq!(requests[1].method, "POST");
        assert_eq!(requests[1].target, "/api/issues/1/activities");
        let comment_body = serde_json::from_str::<Value>(&requests[0].body).unwrap();
        assert!(
            comment_body["body"]
                .as_str()
                .unwrap()
                .contains("git commit -m test")
        );
        let activity_body = serde_json::from_str::<Value>(&requests[1].body).unwrap();
        assert_eq!(activity_body["kind"], "command_executed");
        assert!(
            activity_body["title"]
                .as_str()
                .unwrap()
                .contains("git commit -m test")
        );
        state.running.get("1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn command_activity_posts_github_backing_issue_comment_for_matching_codex_command() {
        let fake = fake_github_issue_comment_gh();
        let workflow_temp = tempfile::tempdir().unwrap();
        let workspace_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        write_github_project_codex_workflow(
            &workflow_path,
            &fake.command,
            &workspace_temp.path().join("workspaces"),
        );
        let store = WorkflowStore::load(workflow_path).expect("workflow");
        let mut state = OrchestratorState::new(&store.current().config);
        let (entry, _stop_rx) = running_entry(issue("PVTI_1", "acme/repo#42", "Todo"));
        state.running.insert("PVTI_1".to_string(), entry);

        handle_command_executed(
            CommandExecutionEvent {
                issue_id: "PVTI_1".to_string(),
                issue_identifier: "acme/repo#42".to_string(),
                command: "gh pr create -R acme/repo --fill".to_string(),
                cwd: Some("/tmp/work".to_string()),
                duration_ms: Some(2500),
                exit_code: Some(0),
            },
            &store,
            &mut state,
        )
        .await;

        let calls = std::fs::read_to_string(&fake.log_path).expect("fake gh log");
        assert!(calls.contains("issue comment 42"));
        assert!(calls.contains("-R acme/repo"));
        assert!(calls.contains("--body"));
        assert!(calls.contains("gh\\ pr\\ create"));
        state.running.get("PVTI_1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn command_activity_posts_github_comment_for_matching_ci_watch_command() {
        let fake = fake_github_issue_comment_gh();
        let workflow_temp = tempfile::tempdir().unwrap();
        let workspace_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        write_github_project_codex_workflow_with_patterns(
            &workflow_path,
            &fake.command,
            &workspace_temp.path().join("workspaces"),
            &["gh run watch"],
        );
        let store = WorkflowStore::load(workflow_path).expect("workflow");
        let mut state = OrchestratorState::new(&store.current().config);
        let (entry, _stop_rx) = running_entry(issue("PVTI_1", "acme/repo#42", "Todo"));
        state.running.insert("PVTI_1".to_string(), entry);

        handle_command_executed(
            CommandExecutionEvent {
                issue_id: "PVTI_1".to_string(),
                issue_identifier: "acme/repo#42".to_string(),
                command: "gh run watch 123456 -R acme/repo".to_string(),
                cwd: Some("/tmp/work".to_string()),
                duration_ms: Some(5000),
                exit_code: Some(0),
            },
            &store,
            &mut state,
        )
        .await;

        let calls = std::fs::read_to_string(&fake.log_path).expect("fake gh log");
        assert!(calls.contains("issue comment 42"));
        assert!(calls.contains("-R acme/repo"));
        assert!(calls.contains("gh\\ run\\ watch"));
        state.running.get("PVTI_1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn command_activity_posts_github_comment_for_default_ci_check_command() {
        let fake = fake_github_issue_comment_gh();
        let workflow_temp = tempfile::tempdir().unwrap();
        let workspace_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        write_github_project_codex_workflow_with_default_patterns(
            &workflow_path,
            &fake.command,
            &workspace_temp.path().join("workspaces"),
        );
        let store = WorkflowStore::load(workflow_path).expect("workflow");
        let mut state = OrchestratorState::new(&store.current().config);
        let (entry, _stop_rx) = running_entry(issue("PVTI_1", "acme/repo#42", "Todo"));
        state.running.insert("PVTI_1".to_string(), entry);

        handle_command_executed(
            CommandExecutionEvent {
                issue_id: "PVTI_1".to_string(),
                issue_identifier: "acme/repo#42".to_string(),
                command: "gh pr checks 42 -R acme/repo --watch".to_string(),
                cwd: Some("/tmp/work".to_string()),
                duration_ms: Some(7500),
                exit_code: Some(0),
            },
            &store,
            &mut state,
        )
        .await;

        let calls = std::fs::read_to_string(&fake.log_path).expect("fake gh log");
        assert!(calls.contains("issue comment 42"));
        assert!(calls.contains("-R acme/repo"));
        assert!(calls.contains("gh\\ pr\\ checks"));
        state.running.get("PVTI_1").unwrap().worker.abort();
    }

    #[tokio::test]
    async fn command_activity_skip_paths_do_not_call_github_tracker_for_codex() {
        let fake = fake_github_issue_comment_gh();
        let workflow_temp = tempfile::tempdir().unwrap();
        let workspace_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        write_github_project_codex_workflow(
            &workflow_path,
            &fake.command,
            &workspace_temp.path().join("workspaces"),
        );
        let store = WorkflowStore::load(workflow_path).expect("workflow");
        let mut state = OrchestratorState::new(&store.current().config);

        let command_event = |issue_id: &str, command: &str| CommandExecutionEvent {
            issue_id: issue_id.to_string(),
            issue_identifier: "acme/repo#42".to_string(),
            command: command.to_string(),
            cwd: Some("/tmp/work".to_string()),
            duration_ms: Some(100),
            exit_code: Some(0),
        };

        handle_command_executed(
            command_event("missing", "gh pr create -R acme/repo --fill"),
            &store,
            &mut state,
        )
        .await;

        let (entry, _stop_rx) = running_entry(issue("PVTI_1", "acme/repo#42", "Todo"));
        state.running.insert("PVTI_1".to_string(), entry);
        handle_command_executed(command_event("PVTI_1", "git status"), &store, &mut state).await;
        handle_command_executed(
            command_event("PVTI_1", "echo \"unterminated"),
            &store,
            &mut state,
        )
        .await;

        let no_patterns_workflow_path = workflow_temp.path().join("NO_PATTERNS_WORKFLOW.md");
        write_github_project_codex_workflow_with_patterns(
            &no_patterns_workflow_path,
            &fake.command,
            &workspace_temp.path().join("no-pattern-workspaces"),
            &[],
        );
        let no_patterns_store =
            WorkflowStore::load(no_patterns_workflow_path).expect("no pattern workflow");
        let mut no_patterns_state = OrchestratorState::new(&no_patterns_store.current().config);
        let (entry, _stop_rx) = running_entry(issue("PVTI_2", "acme/repo#42", "Todo"));
        no_patterns_state
            .running
            .insert("PVTI_2".to_string(), entry);
        handle_command_executed(
            command_event("PVTI_2", "gh pr create -R acme/repo --fill"),
            &no_patterns_store,
            &mut no_patterns_state,
        )
        .await;

        let calls = std::fs::read_to_string(&fake.log_path).unwrap_or_default();
        assert!(calls.is_empty(), "unexpected fake gh calls: {calls}");
        state.running.get("PVTI_1").unwrap().worker.abort();
        no_patterns_state
            .running
            .get("PVTI_2")
            .unwrap()
            .worker
            .abort();
    }

    #[tokio::test]
    async fn on_tick_reload_error_keeps_last_codex_config_but_skips_dispatch() {
        let server = MockHttpServer::spawn(vec![MockResponse::json(
            200,
            json!({
                "issues": [issue_json("1", "ASAHI-1", "Todo", None)]
            }),
        )])
        .await;
        let endpoint = server.endpoint.clone();
        let workspace_temp = tempfile::tempdir().unwrap();
        let workflow_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        write_asahi_endpoint_codex_workflow(
            &workflow_path,
            &endpoint,
            &workspace_temp.path().join("workspaces"),
            2,
            30_000,
        );
        let mut store = WorkflowStore::load(workflow_path.clone()).expect("workflow");
        let mut state = OrchestratorState::new(&store.current().config);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        tokio::time::sleep(StdDuration::from_millis(25)).await;
        std::fs::write(
            &workflow_path,
            "---\ntracker:\n  kind: asahi\n  endpoint: [\n---\nbroken\n",
        )
        .expect("write broken workflow");

        on_tick(&mut store, &mut state, &events_tx)
            .await
            .expect("tick should recover from reload error");

        assert!(matches!(
            store.current().config.tracker,
            TrackerConfig::Asahi(_)
        ));
        assert!(state.running.is_empty());
        assert!(state.claimed.is_empty());
    }

    #[tokio::test]
    async fn on_tick_reload_success_dispatches_codex_from_new_asahi_endpoint() {
        let old_workspace_temp = tempfile::tempdir().unwrap();
        let workflow_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        write_asahi_endpoint_codex_workflow(
            &workflow_path,
            "http://127.0.0.1:9",
            &old_workspace_temp.path().join("old-workspaces"),
            1,
            30_000,
        );
        let mut store = WorkflowStore::load(workflow_path.clone()).expect("workflow");
        let mut state = OrchestratorState::new(&store.current().config);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        let server = MockHttpServer::spawn(vec![
            MockResponse::json(
                200,
                json!({
                    "issues": [issue_json("new", "ASAHI-99", "Todo", None)]
                }),
            ),
            MockResponse::json(200, json!({})),
        ])
        .await;
        let endpoint = server.endpoint.clone();
        let new_workspace_temp = tempfile::tempdir().unwrap();

        tokio::time::sleep(StdDuration::from_millis(25)).await;
        write_asahi_endpoint_codex_workflow(
            &workflow_path,
            &endpoint,
            &new_workspace_temp.path().join("new-workspaces"),
            3,
            123,
        );

        on_tick(&mut store, &mut state, &events_tx)
            .await
            .expect("tick should reload and dispatch");

        assert_eq!(state.poll_interval_ms, 123);
        assert_eq!(state.max_concurrent_agents, 3);
        assert!(state.running.contains_key("new"));
        assert!(state.claimed.contains("new"));
        assert!(
            state
                .running
                .get("new")
                .unwrap()
                .issue
                .identifier
                .eq("ASAHI-99")
        );

        let requests = tokio::time::timeout(StdDuration::from_secs(2), server.recorded_requests())
            .await
            .expect("new Asahi endpoint should receive candidate fetch and activity");
        assert_eq!(requests.len(), 2);
        assert!(requests[0].target.starts_with("/api/issues?"));
        assert_eq!(requests[1].method, "POST");
        assert_eq!(requests[1].target, "/api/issues/new/activities");

        for entry in state.running.values() {
            entry.worker.abort();
        }
    }

    #[tokio::test]
    async fn on_tick_dispatches_github_project_codex_issue_from_fake_gh() {
        let fake = fake_github_project_gh();
        let workflow_temp = tempfile::tempdir().unwrap();
        let workspace_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        write_github_project_codex_workflow(
            &workflow_path,
            &fake.command,
            &workspace_temp.path().join("workspaces"),
        );
        let mut store = WorkflowStore::load(workflow_path).expect("workflow");
        let mut state = OrchestratorState::new(&store.current().config);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        on_tick(&mut store, &mut state, &events_tx)
            .await
            .expect("tick should dispatch GitHub Project issue");

        let calls = std::fs::read_to_string(&fake.log_path).expect("fake gh log");
        assert!(calls.contains("api graphql"), "fake gh calls: {calls}");
        assert!(calls.contains("projectNumber"), "fake gh calls: {calls}");
        assert!(calls.contains("statusField"), "fake gh calls: {calls}");
        assert!(calls.contains("priorityField"), "fake gh calls: {calls}");

        let running_keys = state.running.keys().cloned().collect::<Vec<_>>();
        assert!(
            state.running.contains_key("PVTI_1"),
            "running keys after fake gh dispatch: {running_keys:?}; calls: {calls}"
        );
        assert!(state.claimed.contains("PVTI_1"));
        let running = state.running.get("PVTI_1").unwrap();
        assert_eq!(running.issue.identifier, "acme/repo#42");
        assert_eq!(running.issue.state, "Todo");
        assert_eq!(running.issue.priority, Some(1));

        for entry in state.running.values() {
            entry.worker.abort();
        }
    }

    #[tokio::test]
    async fn on_tick_prefers_running_project_and_respects_codex_global_slots() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(
                200,
                json!({
                    "issues": [
                        issue_json("existing", "ASAHI-1", "Todo", Some("proj-a"))
                    ]
                }),
            ),
            MockResponse::json(
                200,
                json!({
                    "issues": [
                        issue_json("new-b", "ASAHI-3", "Todo", Some("proj-b")),
                        issue_json("new-a", "ASAHI-2", "Todo", Some("proj-a"))
                    ]
                }),
            ),
            MockResponse::json(200, json!({})),
        ])
        .await;
        let endpoint = server.endpoint.clone();
        let workflow_temp = tempfile::tempdir().unwrap();
        let workspace_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        write_asahi_endpoint_codex_workflow(
            &workflow_path,
            &endpoint,
            &workspace_temp.path().join("workspaces"),
            2,
            30_000,
        );
        let mut store = WorkflowStore::load(workflow_path).expect("workflow");
        let mut state = OrchestratorState::new(&store.current().config);
        let (entry, _stop_rx) =
            running_entry(issue_with_project("existing", "ASAHI-1", "Todo", "proj-a"));
        state.running.insert("existing".to_string(), entry);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        on_tick(&mut store, &mut state, &events_tx)
            .await
            .expect("tick should batch same project and respect slots");

        assert!(state.running.contains_key("existing"));
        assert!(state.running.contains_key("new-a"));
        assert!(!state.running.contains_key("new-b"));
        assert!(state.claimed.contains("new-a"));
        assert!(!state.claimed.contains("new-b"));
        assert_eq!(
            state
                .running
                .get("new-a")
                .unwrap()
                .issue
                .project
                .as_ref()
                .map(|project| project.slug.as_str()),
            Some("proj-a")
        );

        let requests = tokio::time::timeout(StdDuration::from_secs(2), server.recorded_requests())
            .await
            .expect("Asahi should receive reconcile, candidates, and activity");
        assert_eq!(requests.len(), 3);
        assert!(requests[0].target.starts_with("/api/issues?"));
        assert!(requests[0].target.contains("ids=existing"));
        assert!(requests[1].target.starts_with("/api/issues?"));
        assert!(requests[1].target.contains("states=Todo"));
        assert!(
            requests[1].target.contains("In%20Progress")
                || requests[1].target.contains("In+Progress"),
            "candidate request target: {}",
            requests[1].target
        );
        assert_eq!(requests[2].method, "POST");
        assert_eq!(requests[2].target, "/api/issues/new-a/activities");

        for entry in state.running.values() {
            entry.worker.abort();
        }
    }

    #[tokio::test]
    async fn embedded_asahi_autostarts_for_missing_tracker_codex_workflow() {
        let workflow_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        std::fs::write(
            &workflow_path,
            r#"---
runner:
  kind: codex
  command: '"codex'
---
Issue {{ issue.identifier }}: {{ issue.title }}
"#,
        )
        .expect("write workflow");
        let mut store = WorkflowStore::load(workflow_path).expect("workflow");

        let mut handle = start_embedded_asahi_if_needed(&mut store, true)
            .await
            .expect("embedded Asahi should start")
            .expect("embedded Asahi handle");

        let expected_db_path = workflow_temp.path().join("asahi.db");
        assert_eq!(
            embedded_asahi_db_path(&store.current().config),
            expected_db_path
        );
        assert_eq!(handle.db_path, expected_db_path);
        assert!(handle.db_path.exists());
        match &store.current().config.tracker {
            TrackerConfig::Asahi(config) => {
                assert_eq!(config.endpoint, handle.endpoint);
                assert!(config.endpoint.starts_with("http://127.0.0.1:"));
            }
            other => panic!("expected Asahi tracker, got {other:?}"),
        }

        let response = reqwest::get(format!("{}/api/issues", handle.endpoint))
            .await
            .expect("embedded Asahi request");
        assert!(response.status().is_success());

        handle.shutdown();
    }

    #[tokio::test]
    async fn embedded_asahi_skips_explicit_endpoint_without_db_for_codex_workflow() {
        let workflow_temp = tempfile::tempdir().unwrap();
        let workflow_path = workflow_temp.path().join("WORKFLOW.md");
        std::fs::write(
            &workflow_path,
            r#"---
tracker:
  kind: asahi
  endpoint: "http://127.0.0.1:9"
runner:
  kind: codex
  command: '"codex'
---
Issue {{ issue.identifier }}: {{ issue.title }}
"#,
        )
        .expect("write workflow");
        let mut store = WorkflowStore::load(workflow_path).expect("workflow");

        let handle = start_embedded_asahi_if_needed(&mut store, false)
            .await
            .expect("explicit endpoint-only Asahi should not fail");

        assert!(handle.is_none());
        match &store.current().config.tracker {
            TrackerConfig::Asahi(config) => {
                assert_eq!(config.endpoint, "http://127.0.0.1:9");
                assert_eq!(config.db, None);
            }
            other => panic!("expected Asahi tracker, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn startup_cleanup_removes_terminal_asahi_workspaces_for_codex_workflow() {
        let server = MockHttpServer::spawn(vec![MockResponse::json(
            200,
            json!({
                "issues": [
                    issue_json("1", "ASAHI-1", "Done", None),
                    issue_json("2", "ASAHI-2", "Done", None)
                ]
            }),
        )])
        .await;
        let endpoint = server.endpoint.clone();
        let workspace_temp = tempfile::tempdir().unwrap();
        let workspace_root = workspace_temp.path().join("workspaces");
        let asahi_one = workspace_root.join("ASAHI-1");
        let asahi_two = workspace_root.join("ASAHI-2");
        let active = workspace_root.join("ASAHI-3");
        tokio::fs::create_dir_all(&asahi_one).await.unwrap();
        tokio::fs::create_dir_all(&asahi_two).await.unwrap();
        tokio::fs::create_dir_all(&active).await.unwrap();
        let (_temp, store) = asahi_endpoint_codex_workflow(&endpoint, workspace_root);

        startup_terminal_workspace_cleanup(store.current()).await;

        assert!(!asahi_one.exists());
        assert!(!asahi_two.exists());
        assert!(active.exists());
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 1);
        assert!(requests[0].target.starts_with("/api/issues?"));
        assert!(requests[0].target.contains("states=Done"));
    }

    #[tokio::test]
    async fn reconcile_running_asahi_issues_updates_active_and_stops_nonactive_or_terminal_codex() {
        let server = MockHttpServer::spawn(vec![MockResponse::json(
            200,
            json!({
                "issues": [
                    issue_json("1", "ASAHI-1", "Done", None),
                    issue_json("2", "ASAHI-2", "Backlog", None),
                    issue_json("3", "ASAHI-3", "In Progress", None)
                ]
            }),
        )])
        .await;
        let endpoint = server.endpoint.clone();
        let workspace_temp = tempfile::tempdir().unwrap();
        let (_temp, store) =
            asahi_endpoint_codex_workflow(&endpoint, workspace_temp.path().join("workspaces"));
        let workflow = store.current().clone();
        let mut state = OrchestratorState::new(&workflow.config);
        let (terminal_entry, terminal_stop_rx) = running_entry(issue("1", "ASAHI-1", "Todo"));
        let (nonactive_entry, nonactive_stop_rx) = running_entry(issue("2", "ASAHI-2", "Todo"));
        let (active_entry, active_stop_rx) = running_entry(issue("3", "ASAHI-3", "Todo"));
        state.running.insert("1".to_string(), terminal_entry);
        state.running.insert("2".to_string(), nonactive_entry);
        state.running.insert("3".to_string(), active_entry);
        let (events_tx, _events_rx) = mpsc::unbounded_channel();

        reconcile_running_issues(&mut state, &workflow, &events_tx).await;

        assert!(state.running.get("1").unwrap().pending_cleanup);
        assert!(matches!(
            terminal_stop_rx.borrow().clone(),
            Some(StopReason::Terminal)
        ));
        assert!(matches!(
            nonactive_stop_rx.borrow().clone(),
            Some(StopReason::NonActive)
        ));
        assert!(active_stop_rx.borrow().is_none());
        assert_eq!(state.running.get("3").unwrap().issue.state, "In Progress");

        for entry in state.running.values() {
            entry.worker.abort();
        }
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 1);
        assert!(requests[0].target.starts_with("/api/issues?"));
        assert!(requests[0].target.contains("ids="));
        assert!(requests[0].target.contains('1'));
        assert!(requests[0].target.contains('2'));
        assert!(requests[0].target.contains('3'));
    }
}
