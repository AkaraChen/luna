# Plan 005: Cap failure retries so broken issues stop retrying forever

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- crates/luna/src/orchestrator.rs crates/luna/src/config.rs crates/luna/src/init.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: plans/001-ci-test-baseline.md
- **Category**: bug
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

Every `Failed` / `TimedOut` / `Stalled` agent exit schedules another attempt,
unconditionally, forever. Only the *delay* is capped (exponential up to
`retry_backoff_ms`), never the *count*. An issue that can never succeed — bad
prompt, failing lifecycle hook, agent binary missing — churns at the backoff
ceiling indefinitely, posting `agent_started` activities each round and
holding a claim. There is no give-up / dead-letter path. A daemon meant to run
unattended needs one.

## Current state

All excerpts verified at `74ad45b`.

- `crates/luna/src/orchestrator.rs:321-374` — `handle_worker_exit` outcome
  handling. Note **two distinct retry flavors**:

  ```rust
  match exit.outcome {
      WorkerOutcome::Normal => {
          state.completed.insert(exit.issue_id.clone());
          schedule_retry(state, ..., /*attempt*/ 1, ..., RetryDelay::Continuation, ...);
      }
      WorkerOutcome::Failed(reason) => {
          schedule_retry(state, ..., entry.retry_attempt.unwrap_or(0) + 1, ...,
              Some(reason), RetryDelay::Backoff, ...);
      }
      WorkerOutcome::TimedOut => { /* same shape, "turn_timeout" */ }
      WorkerOutcome::Stalled  => { /* same shape, "stalled" */ }
      WorkerOutcome::CanceledByReconciliation => {
          state.claimed.remove(&exit.issue_id);
      }
  }
  ```

  `RetryDelay::Continuation` (attempt reset to 1) is the **multi-turn
  continuation mechanic** after a successful turn — it must NOT be capped.

- `crates/luna/src/orchestrator.rs:761-802` — `schedule_retry` computes the
  delay (this IS exponential: `10_000 * 2^(attempt-1)` capped at
  `config.scheduler.retry_backoff_ms`) and stores a `RetryEntry`; no attempt
  check anywhere:

  ```rust
  let delay_ms = match delay_mode {
      RetryDelay::Continuation => 1_000,
      RetryDelay::Backoff => {
          let multiplier = 10_000_u64.saturating_mul(2_u64.saturating_pow(attempt.saturating_sub(1)));
          multiplier.min(config.scheduler.retry_backoff_ms)
      }
  };
  ```

- `crates/luna/src/orchestrator.rs:491-555` — `handle_retry_due` re-dispatches
  when the retry fires. It has two `schedule_retry(..., entry.attempt + 1, ...,
  RetryDelay::Backoff, ...)` re-schedules that are **not failures**: "retry
  poll failed" (`:513-523`, tracker fetch error) and "no available orchestrator
  slots" (`:541-551`). Incrementing the attempt there means a busy scheduler
  or a flaky tracker *consumes the failure budget* of an innocent issue — the
  cap must account for this (Step 2).

- `crates/luna/src/config.rs:313-339` — `SchedulerConfig`:

  ```rust
  #[derive(Clone, Debug, Deserialize, Validate)]
  #[serde(deny_unknown_fields)]
  pub struct SchedulerConfig {
      #[serde(default = "default_max_concurrent")] pub max_concurrent: usize,
      #[serde(default = "default_max_turns")]      pub max_turns: u32,
      #[serde(default = "default_max_retry_backoff_ms")] pub retry_backoff_ms: u64,
      #[serde(default)] pub max_concurrent_by_state: HashMap<String, usize>,
  }
  ```

- Existing tests to model on: `worker_exit_outcomes_schedule_codex_retries_or_release_claims`
  and `retry_due_dispatches_active_codex_issue_and_releases_missing_issue` in
  `orchestrator.rs`'s test module (~`:1962` and `:2086`), with fixtures
  `retry_entry(issue_id, identifier, attempt)` (`:1560`) and config builders
  (`codex_config`/`github_codex_config`, `:1095-1125`).
- Conventions: structured `tracing` with `issue_id`/`identifier` fields;
  config defaults as `default_*` functions + `DEFAULT_*` consts in `config.rs`.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Tests | `cargo test -p luna --locked` | exit 0 |
| Targeted | `cargo test -p luna orchestrator --locked` | exit 0 |
| Lint | `cargo clippy -p luna --all-targets --no-deps` | exit 0 |

## Scope

**In scope**:
- `crates/luna/src/config.rs` — add `max_attempts` to `SchedulerConfig`
- `crates/luna/src/orchestrator.rs` — enforce the cap
- `crates/luna/src/init.rs` — mention the new key in the scaffolded
  scheduler section (commented, with default)

**Out of scope**:
- Tracker-side effects on give-up (posting a comment, moving the card to a
  "Failed" state) — deliberately deferred; see Maintenance notes.
- `RetryDelay::Continuation` behavior and `max_turns` — different mechanism.
- `crates/luna/src/agent/`, `job.rs`.

## Git workflow

- Branch: `advisor/005-retry-attempt-cap`
- Commit style: conventional commits, matching repo history. Suggested:
  `fix: cap failed issue retries`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Add `scheduler.max_attempts` to config

In `config.rs`, add to `SchedulerConfig`:

```rust
#[serde(default = "default_max_attempts")]
#[garde(range(min = 1))]
pub max_attempts: u32,
```

with `const DEFAULT_MAX_ATTEMPTS: u32 = 5;` and the `default_max_attempts()`
fn, following the exact pattern of `retry_backoff_ms`. Update the `Default`
impl. Document in a doc comment: "Maximum dispatch attempts per issue for
failure-class retries (failed / timed out / stalled). Continuation turns after
successful runs are not counted."

**Verify**: `cargo test -p luna config:: --locked` → exit 0.

### Step 2: Stop non-failure re-schedules from consuming the budget

In `handle_retry_due` (`orchestrator.rs:491-555`), change the two
wait-flavored re-schedules to pass `entry.attempt` (NOT `entry.attempt + 1`):

- the "retry poll failed" branch (`:513-523`)
- the "no available orchestrator slots" branch (`:541-551`)

Rationale (leave as a one-line comment at each site): waiting for a slot or a
flaky tracker poll is not a failed attempt; only agent-run failures consume
the budget. Note the delay still grows per `schedule_retry` call only via the
attempt number — with a constant attempt these branches now retry at a fixed
delay, which is acceptable (they were previously double-penalized).

**Verify**: `cargo test -p luna orchestrator --locked` → exit 0 (the existing
`retry_due_*` test asserts dispatch/release behavior, not attempt arithmetic;
if it does assert attempts, update it to the new semantics and say so).

### Step 3: Enforce the cap at the failure sites

In `handle_worker_exit`, before each of the three `schedule_retry(...,
RetryDelay::Backoff, ...)` calls (Failed / TimedOut / Stalled), compute
`let next_attempt = entry.retry_attempt.unwrap_or(0) + 1;` and gate:

```rust
if next_attempt > workflow.config.scheduler.max_attempts {
    error!(issue_id = %exit.issue_id, identifier = %identifier,
        attempts = next_attempt - 1,
        reason = %.../* "failed"|"turn_timeout"|"stalled" */,
        "issue exhausted retry attempts; giving up");
    state.claimed.remove(&exit.issue_id);
    return; // or fall through — ensure no retry is scheduled
}
```

Factor the three near-identical branches through one helper if it stays
readable (they differ only in the reason string). Removing the claim mirrors
what `CanceledByReconciliation` does (`:371-373`) — the issue becomes
re-dispatchable only if a *fresh* candidate poll picks it up again, which is
correct: a human touching the card (or a workflow edit) re-enters it with a
clean slate because the `RetryEntry` is gone. Confirm this by reading
`should_dispatch` (`:680`) — it consults `state.claimed` / `state.completed`;
verify a given-up issue is not permanently blocked from *manual* re-trigger
and describe the observed behavior in your report.

**Verify**: `cargo test -p luna orchestrator --locked` → exit 0.

### Step 4: Scaffold + docs line

In `init.rs`, find the scheduler section of the generated WORKFLOW.md template
(search `retry_backoff_ms` in the file) and add a commented
`# max_attempts: 5` line with a short comment. Keep the template's existing
comment style.

**Verify**: `cargo test -p luna init --locked` → exit 0 (scaffold tests
re-parse the generated file).

## Test plan

New tests in `orchestrator.rs`'s existing `#[cfg(test)]` module, modeled on
`worker_exit_outcomes_schedule_codex_retries_or_release_claims`:

1. `worker_exit_gives_up_after_max_attempts` — build a config with
   `max_attempts: 2`, a `running_entry` whose `retry_attempt` is `Some(2)`,
   deliver a `Failed` exit → assert `state.retry_attempts` does NOT contain
   the issue and `state.claimed` does not either.
2. `worker_exit_retries_below_max_attempts` — same setup with
   `retry_attempt: Some(1)` → assert a `RetryEntry` exists with `attempt == 2`.
3. `continuation_turns_are_not_capped` — `Normal` exit with a high prior
   attempt → assert a Continuation retry is scheduled regardless.
4. `slot_wait_does_not_consume_budget` — drive `handle_retry_due` with zero
   free slots (see `should_dispatch_respects_global_and_state_concurrency_limits`
   for how to saturate slots) → assert the re-scheduled `RetryEntry.attempt`
   equals the original attempt.

**Verification**: `cargo test -p luna --locked` → all pass, including 4 new.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo test -p luna --locked` exits 0, including the 4 new tests
- [ ] `grep -n 'max_attempts' crates/luna/src/config.rs crates/luna/src/orchestrator.rs crates/luna/src/init.rs` → hits in all three
- [ ] `grep -n 'entry.attempt + 1' crates/luna/src/orchestrator.rs` → no matches remaining in `handle_retry_due`'s wait branches
- [ ] `cargo clippy -p luna --all-targets --no-deps` exits 0
- [ ] `git status` shows only in-scope files modified
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- Reading `should_dispatch`/`state.claimed` in Step 3 reveals that removing
  the claim makes the orchestrator *immediately re-dispatch* the same broken
  issue on the next tick (i.e. claims were the only thing preventing
  re-pickup of active-state issues) — the give-up path would then need a
  cooldown/blocklist, which changes the design. Report what you found.
- Any existing test encodes "retries are unlimited" as intended behavior.
- The three failure branches have diverged from the excerpt (drift).

## Maintenance notes

- Give-up is currently **log-only**. The natural follow-ups, deliberately out
  of scope: post a tracker comment ("Luna gave up after N attempts: <reason>")
  and/or move the card to a configured failure state. Both belong with the
  tracker-parity work (audit DIRECTION-03), not here.
- Plan 010 (comment-poll batching) and plan 009 touch other parts of
  `orchestrator.rs`; regions are disjoint from this one.
- Reviewer: check the interaction between `max_attempts` and
  `max_turns` docs in the scaffold — users will confuse them; the comment
  wording matters ("attempts = fresh dispatches after failure; turns = agent
  conversation turns within a run").
