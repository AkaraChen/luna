# Plan 003: Make in-progress agent turns actually cancellable (stop leaking child agents)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- crates/luna/src/agent/angel_runtime.rs`
> If the file changed since this plan was written, compare the "Current state"
> excerpts against the live code before proceeding; on a mismatch, treat it as
> a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: plans/001-ci-test-baseline.md
- **Category**: bug
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

Stall detection, turn timeouts, board reconciliation ("issue moved to Done →
stop the agent"), and Ctrl-C shutdown all try to stop a running agent by
sending `AngelCommand::Cancel` / `Shutdown`. None of them work while a turn is
in flight: the worker is a single blocking loop that fully processes one
`RunTurn` before dequeuing the next command, and the turn-draining loop has no
deadline. `worker.abort()` cannot interrupt a `spawn_blocking` thread, and
`session.close()` only runs on clean loop exit. Net effect: a wedged agent
keeps running, the child codex/opencode process is orphaned on daemon shutdown,
and workspaces never get cleaned. This is the difference between "Luna kills
unresponsive agents" (README promise) and reality.

## Current state

All in `crates/luna/src/agent/angel_runtime.rs`.

- The worker loop — commands are strictly serialized, so `Cancel` waits behind
  a running `RunTurn` (`:254-283`):

  ```rust
  impl AngelWorker {
      fn run(mut self, mut command_rx: mpsc::UnboundedReceiver<AngelCommand>) {
          while let Some(command) = command_rx.blocking_recv() {
              match command {
                  AngelCommand::Start { respond } => { ... }
                  AngelCommand::RunTurn { prompt, turn_number, respond } => {
                      let result = self.run_turn(prompt, turn_number);
                      let _ = respond.send(result);
                  }
                  AngelCommand::SendComment { body, respond } => { ... }
                  AngelCommand::Cancel => {
                      if let Err(err) = self.session.cancel_turn() { ... }
                  }
                  AngelCommand::Shutdown => break,
              }
          }
          self.session.close();
      }
  ```

- The drain loop — no overall deadline; `None` (poll timeout) just continues
  forever (`:323-340`):

  ```rust
  fn drain_until_result(&mut self, turn_number: u32) -> Result<TurnExit> {
      loop {
          match self.session.next_turn_event(Duration::from_millis(250))
              .map_err(angel_error)?
          {
              Some(event) => { ...; if Result → return Ok(TurnExit::Completed); }
              None => continue,
          }
      }
  }
  ```

- The async side — on stop/timeout it *sends* Cancel (which queues) and
  returns immediately, leaving the blocking thread running (`:545-561`):

  ```rust
  tokio::select! {
      result = rx => { ... }
      changed = stop_rx.changed() => {
          let _ = self.command_tx.send(AngelCommand::Cancel);
          return Ok(TurnExit::Stopped(reason));
      }
      _ = tokio::time::sleep(Duration::from_millis(self.config.turn_timeout_ms)) => {
          let _ = self.command_tx.send(AngelCommand::Cancel);
          Ok(TurnExit::TimedOut)
      }
  }
  ```

- Shutdown — `abort()` on a `spawn_blocking` handle does not stop the thread
  (`:576-579`):

  ```rust
  async fn shutdown(&mut self) {
      let _ = self.command_tx.send(AngelCommand::Shutdown);
      self.worker.abort();
  }
  ```

- The worker is created at `:199-215`: `AngelSession::new(options)` inside
  `spawn_blocking`, then `let worker = tokio::task::spawn_blocking(move || worker.run(command_rx));`
- `AngelSession`, `cancel_turn`, `next_turn_event`, `close` come from the
  vendored `angel-engine/crates/angel-engine-client` (read-only submodule).
  This plan was written without inspecting the client's thread-safety; Step 1
  establishes it.
- Existing tests for this file live in its `#[cfg(test)]` module (~`:596` on)
  and test `project_turn_event`/launch-config as pure functions — there is no
  fake `AngelSession`. `cargo test -p luna angel` runs them.
- Repo convention: no blocking the orchestrator loop on agent I/O (stated in
  `crates/luna/AGENTS.md` anti-patterns) — your changes stay inside the worker.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Tests | `cargo test -p luna --locked` | exit 0 |
| Targeted | `cargo test -p luna angel --locked` | exit 0 |
| Lint | `cargo clippy -p luna --all-targets --no-deps` | exit 0 |

## Scope

**In scope**:
- `crates/luna/src/agent/angel_runtime.rs`
- `crates/luna/src/job.rs` — ONLY if its copy of the drain loop
  (`run_angel_job_session`) gets the same deadline fix (Step 5)

**Out of scope**:
- `angel-engine/` — read-only. If cancellation genuinely requires a client
  change, that's a STOP-and-report, not an edit.
- `crates/luna/src/orchestrator.rs` — the stop/stall/timeout signals already
  arrive correctly; only their handling in the worker is broken.

## Git workflow

- Branch: `advisor/003-cancellable-agent-turns`
- Commit style: conventional commits, matching repo history. Suggested:
  `fix: make agent turns cancellable`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Establish the client's cancellation semantics (read-only)

Read `angel-engine/crates/angel-engine-client/src/` and answer, quoting code
in your report:

1. Is `AngelSession: Send`/`Sync`? Can `cancel_turn()` be called from a
   different thread than the one iterating `next_turn_event`? (Look at the
   struct's fields — channels/Mutex vs `Rc`/raw handles.)
2. What does `cancel_turn()` do to a turn in progress — does the event stream
   subsequently yield a Result/terminal event?
3. What does `close()` do (kill the child process? graceful shutdown?), and
   what does dropping `AngelSession` do?

The design below (shared cancel flag checked inside the drain loop, cancel
issued from the worker thread itself) deliberately avoids needing `Sync`. If
the answers reveal a simpler path (e.g. a cancellable handle designed for
cross-thread use), prefer it and note the deviation in your report.

**Verify**: report contains the three answers with code quotes.

### Step 2: Add a shared cancel flag the drain loop observes

1. Add `cancel_requested: Arc<AtomicBool>` to both `AngelWorker` and
   `AngelRuntimeSession` (created in `launch_runtime`, cloned into the worker).
2. In the async side (`run_turn` select arms at `:549-560` and `shutdown` at
   `:576`), set the flag with `store(true, Ordering::SeqCst)` **before**
   sending `Cancel`/`Shutdown`. Reset it to `false` at the start of each
   `AngelCommand::RunTurn` dispatch on the async side (`:536-543`) so a new
   turn isn't stillborn from a previous cancel.
3. In `drain_until_result`, on every iteration check the flag; when set, call
   `self.session.cancel_turn()` **once** (guard with a local bool), then keep
   draining until the terminal event arrives or the grace deadline (Step 3)
   expires, returning `Ok(TurnExit::Stopped(StopReason::Shutdown))`-shaped
   exit — see Step 3 for the exact return. Apply the same check at the top of
   the `start()`/`run_turn()` event loops (`:293-295`, `:317-319`).

**Verify**: `cargo build -p luna` → exit 0.

### Step 3: Give `drain_until_result` a hard deadline

1. Compute a deadline at loop entry: `Instant::now() + turn_timeout_ms +
   DRAIN_GRACE` where `DRAIN_GRACE` is a module constant (suggest 30s). Pass
   `turn_timeout_ms` into `AngelWorker` (it's already on
   `AngelRuntimeLaunchConfig`).
2. When the deadline passes (or the cancel-grace deadline after a
   `cancel_turn()` — suggest 10s from the cancel), call
   `self.session.cancel_turn()` (if not already done) and return
   `Err(LunaError::Agent("angel turn drain deadline exceeded".into()))` — an
   `Err` here propagates to the `RunTurn` responder, and the async side's
   select has already returned TimedOut/Stopped, so the oneshot send just
   fails harmlessly (`let _ = respond.send(result)` at `:268`).

**Verify**: `cargo test -p luna angel --locked` → exit 0.

### Step 4: Make `close()` unconditional and shutdown non-lying

1. Wrap session teardown so it always runs: keep `self.session.close()` after
   the loop, but the loop can now always exit (deadline from Step 3 bounds
   every blocking call). Additionally handle the `Err` propagation paths in
   `run()` — currently `start()`/`run_turn()` errors return through the
   responder, which is fine; confirm no early-`return` skips `close()` (the
   `?` operators are inside the helper methods, not `run()`, so today only
   `break` exits — keep it that way).
2. In `AngelRuntimeSession::shutdown` (`:576-579`): set the cancel flag, send
   `Shutdown`, then **replace** `self.worker.abort()` with a bounded join:
   await the `JoinHandle` under `tokio::time::timeout` (suggest `DRAIN_GRACE +
   5s`); on timeout, log a warning that the worker thread is being detached
   (and only then `abort()` as a no-op-but-tidy fallback). `shutdown` is
   already `async` so awaiting is available.

**Verify**: `cargo test -p luna --locked` → exit 0 (orchestrator tests
exercise shutdown paths, e.g. `reconcile_stalled_runs_sends_stalled_stop_for_codex_runner`).

### Step 5: Apply the drain-deadline to the job runner's copy

`crates/luna/src/job.rs` has a near-duplicate turn loop
(`run_angel_job_session`, ~`:256-303`) polling `next_turn_event(250ms)`. Give
it the same hard deadline (turn timeout + grace). Cancellation-on-signal is
less critical there (one-off foreground command), so the deadline alone is
sufficient — do not build the flag machinery twice.

**Verify**: `cargo test -p luna job --locked` → exit 0.

### Step 6: End-to-end sanity (manual, no live agent required)

Run the daemon against a workflow whose runner command is a script that never
emits a result (e.g. `command: "sleep 3600"` will fail the protocol handshake
— if that errors out too early to exercise the drain loop, note it and rely on
the unit deadline tests instead). Then Ctrl-C: the process must exit within
the grace window with no orphaned child (`pgrep -f sleep` / `ps` check). If
setting this up costs more than ~30 minutes, skip it and say so in the report;
the deadline unit coverage is the required gate.

**Verify**: daemon exits ≤ grace window after Ctrl-C; no orphan process.

## Test plan

The drain loop currently takes `&mut self` on a concrete `AngelSession`, which
can't be faked without a seam. Minimal seam: extract the deadline/cancel
decision into a pure helper, e.g.
`fn drain_step(now: Instant, deadline: Instant, cancel_requested: bool, cancel_issued: &mut bool) -> DrainAction`
(enum: `Continue`, `IssueCancel`, `GiveUp`), and unit-test *that* in the
existing `#[cfg(test)]` module:

- not cancelled, before deadline → `Continue`
- cancel flag set, not yet issued → `IssueCancel` (and sets `cancel_issued`)
- cancel issued, grace expired → `GiveUp`
- deadline passed without cancel → `GiveUp`

Plus the existing suites must stay green: `cargo test -p luna --locked`.
Model the new tests on the pure-function tests already in the file
(e.g. the `project_turn_event` tests).

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo test -p luna --locked` exits 0, including ≥3 new drain-decision tests
- [ ] `grep -n 'None => continue' crates/luna/src/agent/angel_runtime.rs` returns no match inside `drain_until_result` (the unconditional continue is gone)
- [ ] `grep -n 'worker.abort()' crates/luna/src/agent/angel_runtime.rs` shows abort only as the post-timeout fallback, not the primary shutdown mechanism
- [ ] `grep -n 'from_millis(250)' crates/luna/src/job.rs` — job drain loop now checks a deadline (inspect surrounding lines)
- [ ] `cargo clippy -p luna --all-targets --no-deps` exits 0
- [ ] `git status` shows only in-scope files modified
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- Step 1 shows `cancel_turn()` cannot be safely called from the worker thread
  while `next_turn_event` polling is in progress on the same thread — i.e. the
  client API forces a cross-thread call and the session is not `Sync`. Report
  the client's actual API shape; the fix may belong in angel-engine.
- Step 1 shows `close()` does NOT terminate the child process — then the leak
  has a second cause and the plan under-fixes; report before proceeding.
- The worker/async protocol changes ripple into `orchestrator.rs` beyond
  compile-level adjustments.
- Any existing orchestrator test fails after Step 4 and the fix isn't obvious
  within two attempts.

## Maintenance notes

- The grace constants (drain grace, shutdown join timeout) are policy — a
  reviewer should sanity-check them against real agent turn lengths; too short
  kills slow-but-healthy turns at the deadline boundary.
- Plan 002 (permission wiring) touches the same file's launch config; whoever
  executes second rebases trivially — the regions are disjoint (launch config
  vs worker loop).
- Deferred: consolidating `job.rs`'s duplicated session driver into
  `agent/angel_runtime.rs` (audit DEBT-03). Step 5 patches its copy; it does
  not merge them.
- If angel-engine later exposes a first-class cancellation handle, the
  flag-plus-worker-thread-cancel design here can be simplified; leave a
  `// NOTE:` at the flag declaration pointing to this plan.
