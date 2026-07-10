# Plan 009: Fetch the GitHub project board once per poll tick instead of twice

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- crates/luna/src/orchestrator.rs crates/luna/src/tracker/`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/001-ci-test-baseline.md; execute after 005 (both edit `orchestrator.rs`; 005 is smaller — land it first)
- **Category**: perf
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

Every poll tick crawls the **entire** GitHub project board twice through the
`gh` CLI's paginated GraphQL (50 items/page): once for reconciliation of
running issues, once for candidate selection, milliseconds apart. On a board
with a few hundred items at a short poll interval this is the daemon's
dominant cost and doubles GitHub API rate-limit burn for identical data. The
Asahi backend filters server-side and is cheap — this is GitHub-specific.

## Current state

- `crates/luna/src/orchestrator.rs:102-151` — `on_tick` does both fetches:

  ```rust
  async fn on_tick(store, state, events_tx) -> Result<()> {
      let current = store.current().clone();
      reconcile_running_issues(state, &current, events_tx).await;   // fetch #1 (inside)
      ...
      let tracker = match build_tracker(&workflow.config.tracker) { ... };
      let candidates = match tracker.fetch_candidate_issues().await { ... };  // fetch #2
  ```

- `orchestrator.rs:613-640` — `reconcile_running_issues` builds its own
  tracker and calls `tracker.fetch_issue_states_by_ids(&ids)`; it also early-
  returns when `state.running.is_empty()` (so fetch #1 only happens with
  running agents). Its result feeds terminal/active branching at `:651-660`.
- `crates/luna/src/tracker/github_project.rs:359-394` — all three trait
  methods are filters over the same full crawl:

  ```rust
  async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
      let all = self.fetch_all_items().await?;
      Ok(all.into_iter().filter(|issue| self.config.is_active_state(&issue.state)).collect())
  }
  async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>> {
      ... let all = self.fetch_all_items().await?; ...
  }
  async fn fetch_issue_states_by_ids(&self, issue_ids: &[String]) -> Result<Vec<Issue>> {
      ... let all = self.fetch_all_items().await?;
      Ok(all.into_iter().filter(|issue| ids.contains(&issue.id)).collect())
  }
  ```

  `fetch_all_items` (`:41-80`) is the paginated crawl (one `gh api graphql`
  subprocess per 50-item page).
- The `Tracker` trait lives in `crates/luna/src/tracker/mod.rs` (~`:26`), with
  backends `github_project`, `asahi`, `linear`. The Asahi backend filters
  server-side via query params (`tracker/asahi.rs:134-138`) — for it, two
  scoped calls are *cheaper* than one full snapshot. Design accordingly: the
  snapshot is an **optional capability**, not a trait-wide change of contract.
- **Critical semantic**: `fetch_candidate_issues` filters to active states,
  but reconciliation must see running issues that moved to *terminal* states
  (Done/Canceled) — a candidates-only snapshot is NOT sufficient for
  reconcile. The shared snapshot must be the unfiltered board.
- Test harness: orchestrator tests drive GitHub flows through a `FakeGh`
  fixture (`orchestrator.rs:1186` `fake_github_project_gh()`), which stubs the
  `gh` binary; tracker unit tests live in `github_project.rs`. Config fixtures:
  `github_codex_config()` (`:1117`).

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Tests | `cargo test -p luna --locked` | exit 0 |
| Targeted | `cargo test -p luna orchestrator --locked`, `cargo test -p luna github --locked` | exit 0 |
| Lint | `cargo clippy -p luna --all-targets --no-deps` | exit 0 |

## Scope

**In scope**:
- `crates/luna/src/tracker/mod.rs` — optional snapshot capability on the trait
- `crates/luna/src/tracker/github_project.rs` — implement it
- `crates/luna/src/orchestrator.rs` — `on_tick` / `reconcile_running_issues`
  restructuring to share one fetch

**Out of scope**:
- `tracker/asahi.rs`, `tracker/linear.rs` — they keep the default (no
  snapshot); do not add snapshot implementations.
- `handle_retry_due`'s `fetch_candidate_issues` call (`orchestrator.rs:509`)
  — retry-time fetches are rare and event-driven; leave them.
- Comment polling (`poll_comments`) — plan 010's territory.
- Caching across ticks / tracker-client reuse — see Maintenance notes.

## Git workflow

- Branch: `advisor/009-single-board-fetch-per-tick`
- Commit style: conventional commits, matching repo history. Suggested:
  `perf: fetch github board once per tick`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Add an opt-in snapshot method to the `Tracker` trait

In `tracker/mod.rs`, add a defaulted method:

```rust
/// One unfiltered fetch of every issue on the board, for backends where a
/// full crawl is the cheapest primitive (GitHub Projects). Backends that
/// filter server-side return None and callers fall back to scoped fetches.
async fn fetch_board_snapshot(&self) -> Result<Option<Vec<Issue>>> {
    Ok(None)
}
```

Implement it in `github_project.rs` as
`Ok(Some(self.fetch_all_items().await?))`.

**Verify**: `cargo build -p luna` → exit 0 (default keeps asahi/linear
compiling untouched).

### Step 2: Restructure `on_tick` around one snapshot

In `on_tick`:
1. Build the tracker **once**, before reconciliation (it's currently built
   twice — in `reconcile_running_issues` and in `on_tick`; note the workflow
   reload at `:112-125` happens between them today — preserve ordering
   semantics: reconcile uses the pre-reload workflow `current`, dispatch uses
   the post-reload one; the snapshot fetch should use the post-reload tracker
   config only if reconcile does too, so: fetch the snapshot ONCE after the
   reload, and pass it to reconciliation — reconciling against the fresher
   config is an improvement, but call it out in your report).
2. Call `tracker.fetch_board_snapshot().await`:
   - `Ok(Some(all))` → derive both datasets in memory:
     `running_states = all.iter().filter(|i| running_ids.contains(&i.id))`
     and `candidates = all.iter().filter(|i| config.is_active_state(&i.state))`.
   - `Ok(None)` → current behavior: `fetch_issue_states_by_ids` inside
     reconcile + `fetch_candidate_issues` for dispatch.
   - `Err` → log and keep workers running (mirror today's per-call error
     handling at `:634-638` and `:142-148`: reconcile errors keep workers;
     candidate errors skip dispatch).
3. Change `reconcile_running_issues` to accept the refreshed issues (or an
   `Option<&[Issue]>` + tracker for fallback) instead of always fetching —
   keep its terminal/active branching (`:651-660`) byte-for-byte.

**Verify**: `cargo test -p luna orchestrator --locked` → exit 0.

### Step 3: Count the subprocess calls in a test

Add an orchestrator test (model on the existing FakeGh-driven tests, e.g.
`poll_comments_forwards_only_new_github_comments_to_running_codex` at `:2260`
for harness shape): drive one tick with ≥1 running issue on the GitHub
config and assert the FakeGh invocation count for the board query is **1**
(the fixture records calls — read `FakeGh` at `:1186-1274` to find its call
log; if it doesn't record, extend the fixture). Also add a
`fetch_board_snapshot` unit test in `github_project.rs` mirroring the
existing `fetch_all_items`-based method tests.

**Verify**: `cargo test -p luna --locked` → exit 0 including the new tests.

## Test plan

- New: snapshot-derivation test (running-state filtering + candidate filtering
  from one snapshot yields same results as the two old calls — table-driven
  over a small board with active, terminal, and unrelated issues); the
  one-subprocess-per-tick assertion (Step 3); `fetch_board_snapshot` returns
  `None` default for the Asahi tracker (one-liner).
- Existing: full `cargo test -p luna --locked` must stay green — the
  reconcile tests (`worker_event_entry_routes_*`, `reconcile_stalled_runs_*`)
  and dispatch tests encode the semantics being preserved.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo test -p luna --locked` exits 0, including the new tests
- [ ] A single tick with running issues performs exactly one board crawl on the GitHub backend (asserted by test)
- [ ] `grep -n 'fetch_issue_states_by_ids' crates/luna/src/orchestrator.rs` shows it only on the fallback (snapshot-None) path
- [ ] `tracker/asahi.rs` and `tracker/linear.rs` are unmodified (`git status`)
- [ ] `cargo clippy -p luna --all-targets --no-deps` exits 0
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- Preserving the reconcile-before-reload vs dispatch-after-reload ordering
  (Step 2.1) turns out to be load-bearing in a test (a test fails specifically
  because reconcile now sees post-reload config) — the interleaving is then a
  real design constraint; report it rather than reordering tests.
- `FakeGh` cannot express call counting without rewriting the fixture
  wholesale.
- The refactor forces trait-signature changes on `fetch_candidate_issues` /
  `fetch_issue_states_by_ids` themselves (it shouldn't — the snapshot is
  additive).

## Maintenance notes

- Deliberately NOT done: caching the snapshot across ticks (staleness vs
  rate-limit tradeoff — a policy decision), reusing the tracker client between
  ticks (audit PERF-07; trivial once someone decides where it lives in
  `OrchestratorState`), and batching `handle_retry_due`'s fetch.
- If per-state server-side filtering ever lands in the GitHub GraphQL query,
  revisit whether the snapshot is still the cheapest primitive.
- Reviewer: diff `reconcile_running_issues`'s terminal/active branching
  against the old body — it must be unchanged; the only delta is where the
  issues come from.
