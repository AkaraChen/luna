# Plan 010: Parallelize comment polling, make its interval configurable, and stop it from being able to stall the daemon

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

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: plans/001-ci-test-baseline.md; execute after 005 and 009 (same file — this is the last of the three orchestrator plans)
- **Category**: perf (with one correctness fix folded in)
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

Every 2 seconds — hardcoded, regardless of the configured poll interval — the
orchestrator fetches the full comment list of every running issue, one
`gh`-subprocess or HTTP round-trip at a time, **serially, on the main select
loop**. With C concurrent agents that is 30·C fetches/minute whether or not
anything changed, and the loop's latency grows linearly with C. Worse, the
forwarding `send().await` into each agent's bounded(16) comment channel also
runs on the main loop: if one agent stops draining, the whole daemon — ticks,
dispatch, reconcile, every other agent — freezes until it drains.

## Current state

All excerpts verified at `74ad45b`.

- The hardcoded ticker, `crates/luna/src/orchestrator.rs:66-67` and its select
  arm `:91-95` (note: `poll_comments` is awaited directly on the main loop):

  ```rust
  let mut comment_ticker = interval(Duration::from_secs(2));
  ...
  _ = comment_ticker.tick() => {
      if let Err(err) = poll_comments(&mut store, &mut state, &events_tx).await { ... }
  }
  ```

- `poll_comments`, `:836-868` — serial per-issue fetch + awaited bounded send:

  ```rust
  for entry in state.running.values_mut() {
      let comments = match tracker.fetch_comments(&entry.issue).await {
          Ok(c) => c,
          Err(err) => { warn!(...); continue; }
      };
      for comment in comments {
          if entry.seen_comment_ids.insert(comment.id.clone()) {
              if let Some(tx) = entry.comment_tx.take() {
                  let _ = tx.send(comment.body).await;   // ← bounded(16); can block the daemon
                  entry.comment_tx = Some(tx);
              }
          }
      }
  }
  ```

  **Correctness trap to fix while here**: `seen_comment_ids.insert` happens
  *before* the send attempt. With `try_send` (Step 3) a comment dropped on a
  full channel would be marked seen and lost forever — the insert must become
  conditional on successful delivery.

- The comment channel is created at dispatch: `mpsc::channel::<String>(16)`
  (`orchestrator.rs` `dispatch_issue` region, ~`:569`); the agent drains it
  between turns via `try_recv` (`agent/mod.rs:273` region).
- `PollingConfig` lives in `crates/luna/src/config.rs` (search
  `struct PollingConfig`; it has `interval_ms` with a `default_*` fn) —
  follow that pattern for the new field.
- Existing tests to model on (all in `orchestrator.rs`'s test module):
  `poll_comments_forwards_only_new_asahi_comments_to_running_codex` (`:2134`),
  `poll_comments_ignores_fetch_errors_and_missing_codex_comment_sender`
  (`:2204`), `poll_comments_forwards_only_new_github_comments_to_running_codex`
  (`:2260`), with `running_entry_with_comment_rx` (`:1520`) as the fixture.
- Known adjacent issues deliberately NOT fixed here (audit findings, for
  awareness so you don't "fix" them ad hoc): comments are also independently
  fetched by the agent loop (double delivery), and `seen_comment_ids` starts
  empty so pre-existing comments are replayed on the first poll. Both need a
  design decision about which delivery path owns comments — out of scope.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Tests | `cargo test -p luna --locked` | exit 0 |
| Targeted | `cargo test -p luna poll_comments --locked` | exit 0 |
| Lint | `cargo clippy -p luna --all-targets --no-deps` | exit 0 |

## Scope

**In scope**:
- `crates/luna/src/orchestrator.rs` — `poll_comments` + the ticker setup
- `crates/luna/src/config.rs` — `PollingConfig.comment_interval_ms`
- `crates/luna/src/init.rs` — scaffold comment for the new key

**Out of scope**:
- `agent/mod.rs`'s own comment fetching (double-delivery, audit
  CORRECTNESS-05/06) — needs a path-ownership decision first.
- Changing the channel capacity or the agent-side drain cadence.
- `tracker/*` — no `since`-filtered fetch APIs in this plan (see Maintenance).

## Git workflow

- Branch: `advisor/010-comment-poll-batching`
- Commit style: conventional commits, matching repo history. Suggested:
  `perf: batch comment polling`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Make the interval configurable

Add to `PollingConfig` in `config.rs`:
`#[serde(default = "default_comment_interval_ms")] #[garde(range(min = 250))] pub comment_interval_ms: u64`
with `DEFAULT_COMMENT_INTERVAL_MS: u64 = 2_000` (today's behavior). Update
`Default`, add the scaffold line in `init.rs` (commented, like the other
polling keys). In `run()` (`orchestrator.rs:66`), build `comment_ticker` from
the config; also re-arm it after workflow reload the same way the main ticker
is re-armed (`:84-86`) — mirror that block, keyed on
`comment_interval_ms`. (If re-arming both tickers gets repetitive, a tiny
helper is fine.)

**Verify**: `cargo test -p luna config:: --locked` and
`cargo test -p luna init --locked` → exit 0.

### Step 2: Fetch all running issues' comments concurrently

Restructure `poll_comments`: because the loop needs `&mut` entries but the
fetches don't, split phases —

1. Collect `(issue_id, issue.clone())` for all running entries.
2. `futures::future::join_all` (the `futures` crate is already a luna
   dependency — verify in `crates/luna/Cargo.toml`; if absent, use a manual
   `FuturesUnordered` from `futures_util` already in the tree, or spawn tasks
   and join) over `tracker.fetch_comments(&issue)` for each, yielding
   `Vec<(issue_id, Result<Vec<Comment>>)>`.
3. Then apply results sequentially against `state.running.get_mut(&issue_id)`
   (entries may have exited during the awaits — skip missing ones silently;
   that's today's implicit behavior since the loop held the lock the whole
   time, and a just-exited agent doesn't need comments).
   Keep the per-issue error `warn!` + continue semantics.

**Verify**: `cargo test -p luna poll_comments --locked` → exit 0 (the three
existing tests cover new-only forwarding, error tolerance, and both backends).

### Step 3: Replace the blocking send with `try_send` — without losing comments

In the apply phase, for each *new* comment id:

```rust
if entry.seen_comment_ids.contains(&comment.id) { continue; }
match entry.comment_tx.as_ref() {
    Some(tx) => match tx.try_send(comment.body.clone()) {
        Ok(()) => { entry.seen_comment_ids.insert(comment.id.clone()); }
        Err(mpsc::error::TrySendError::Full(_)) => {
            warn!(issue_id = %entry.issue.id, "agent comment channel full; will redeliver next poll");
            break; // preserve comment ordering: don't deliver later ones first
        }
        Err(mpsc::error::TrySendError::Closed(_)) => { entry.comment_tx = None; break; }
    },
    None => break,
}
```

Key invariant (add as a comment): a comment id enters `seen_comment_ids` ONLY
after successful delivery, so a full channel redelivers next poll instead of
dropping. The `break` on Full/Closed preserves in-order delivery. Note the
existing `tx.take()/put-back` dance was only needed for the `&mut` +
`send().await` borrow; with `try_send` on `as_ref()` it disappears.

**Verify**: `cargo test -p luna poll_comments --locked` → exit 0.

## Test plan

New tests in the orchestrator test module (fixtures:
`running_entry_with_comment_rx`):

1. `poll_comments_redelivers_when_channel_full` — fill the entry's channel to
   capacity (send 16 items into the paired sender or use a capacity-1 channel
   in the fixture if it allows), poll once with 1 new comment → assert the
   comment id is NOT in `seen_comment_ids`; drain the channel, poll again with
   the same fetch result → assert delivered and now marked seen.
2. `poll_comments_fetches_all_running_issues_even_when_one_errors` — two
   running entries, tracker errors for the first → second still gets its
   comment (extends the existing error-tolerance test to the concurrent
   shape).
3. Existing three `poll_comments_*` tests stay green unmodified (they encode
   the dedup and backend behavior).

**Verification**: `cargo test -p luna --locked` → all pass, including 2 new.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo test -p luna --locked` exits 0, including the new tests
- [ ] `grep -n 'from_secs(2)' crates/luna/src/orchestrator.rs` → no match (interval comes from config)
- [ ] `grep -n '\.send(comment' crates/luna/src/orchestrator.rs` → no awaited bounded send in `poll_comments`; `try_send` present
- [ ] `seen_comment_ids.insert` occurs only after a successful `try_send` (inspect the apply loop)
- [ ] `grep -n 'comment_interval_ms' crates/luna/src/config.rs crates/luna/src/init.rs` → hits in both
- [ ] `cargo clippy -p luna --all-targets --no-deps` exits 0
- [ ] `git status` shows only in-scope files modified
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The `futures`/`futures_util` join primitive isn't available in luna's
  dependency tree and adding it would be a new direct dependency — confirm
  with the operator because that is a manifest change this plan didn't budget.
- Any existing `poll_comments_*` test depends on the insert-before-send
  ordering (i.e. asserts a comment is "seen" despite a missing sender —
  `poll_comments_ignores_fetch_errors_and_missing_codex_comment_sender` may!
  Read it first. If it does, the redelivery semantics change what that test
  encodes; update it deliberately and call the behavior change out in your
  report rather than silently rewriting the assertion).
- Concurrent fetching reorders comment delivery *across issues* in a way a
  test asserts against (none known, but the fixture-driven tests are precise).

## Maintenance notes

- The real long-term fix for fetch volume is a `since`/updated-after parameter
  on `Tracker::fetch_comments` (server-side filtering) and/or a single batched
  query for all running issues — both are tracker-API changes deferred with
  the backend-parity work.
- The double-delivery of comments (orchestrator channel + agent-loop fetch,
  audit CORRECTNESS-05) and the first-poll replay of pre-existing comments
  (CORRECTNESS-06) remain open; when someone consolidates comment delivery to
  one path, this plan's redelivery invariant is the piece to keep.
- Reviewer: the Full-channel `break` means one slow agent delays its own
  later comments (correct) but never other agents' (the point of the change)
  — check the apply loop is per-entry.
