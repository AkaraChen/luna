# Plan 008: Batch Asahi's N+1 hydration and replace full-table locator scans with indexed queries

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- crates/asahi/src/service/mod.rs`
> If the file changed since this plan was written, compare the "Current state"
> excerpts against the live code before proceeding; on a mismatch, treat it as
> a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: plans/007-asahi-domain-service-tests.md (safety net), plans/004-unique-issue-identity.md (touches the same file; land first to avoid conflicts)
- **Category**: perf
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

Listing N issues costs `1 + ~4N` SQLite queries (labels, project, relations,
blockers fetched per issue), and *single-record* lookups by locator load the
**entire issues or projects table** into memory and filter in Rust — inside a
loop, for blocker resolution. This path backs both the dashboard board view
and Luna's Asahi-tracker poll, so the cost is paid on every orchestrator tick
and grows linearly with tracker age. The needed indexes already exist
(`idx_issues_identifier`, `idx_issue_labels_issue_id`,
`idx_issue_relations_issue_id`); only the query shapes are wrong.

## Current state

All in `crates/asahi/src/service/mod.rs` (verified at `74ad45b`).

- The per-item hydration loop (`:1457-1463`) and its 3-4 queries per issue
  (`:1465-1514`):

  ```rust
  async fn hydrate_issues(&self, models: Vec<issue::Model>) -> ServiceResult<Vec<Issue>> {
      let mut issues = Vec::with_capacity(models.len());
      for model in models {
          issues.push(self.hydrate_issue(model).await?);
      }
      Ok(issues)
  }

  async fn hydrate_issue(&self, model: issue::Model) -> ServiceResult<Issue> {
      let labels = issue_label::Entity::find()
          .filter(issue_label::Column::IssueId.eq(model.id.clone()))
          .order_by_asc(issue_label::Column::Name).all(&self.db).await?...;
      let project = match model.project_id.as_deref() {
          Some(project_id) => project::Entity::find_by_id(project_id.to_string())
              .one(&self.db).await?.map(model_to_project_ref),
          None => None,
      };
      let relations = issue_relation::Entity::find()
          .filter(issue_relation::Column::IssueId.eq(model.id.clone()))
          .all(&self.db).await?;
      // blockers: one more query with Column::Id.is_in(blocker_ids)
      ...
      Ok(model_to_issue(model, project, labels, blockers))
  }
  ```

- The full-table locator scans (`:1403-1445`):

  ```rust
  // find_project_model (:1403)
  let models = project::Entity::find().all(&self.db).await?;
  Ok(models.into_iter().find(|model| {
      let candidate = model_to_project(model.clone());
      project_matches_locator(&candidate, locator)
  }))

  // find_project_model_by_slug (:1410-1419)
  let models = project::Entity::find().all(&self.db).await?;
  Ok(models.into_iter().find(|model| model.slug.eq_ignore_ascii_case(&slug)))

  // resolve_issue_locators (:1421-1431) — calls find_issue_id per locator, in a loop

  // find_issue_id (:1433-1445)
  let models = issue::Entity::find().all(&self.db).await?;
  Ok(models.into_iter()
      .map(|model| model_to_issue(model, None, Vec::new(), Vec::new()))
      .find(|issue| issue_matches_locator(issue, locator))
      .map(|issue| issue.id))
  ```

- Callers of `find_issue_id`: `find_issue`, `list_activities` (`:1517-1521`),
  `list_comments`, comment/activity creation, `resolve_issue_locators` (used
  by `create_issue` for `blocked_by`). `hydrate_issues` is called from
  `list_issues` (`:760-791`).
- Notification hydration has the same per-item lookup
  (`hydrate_notifications` ~`:1635`, `find_by_id` per notification ~`:1646`) —
  bounded by the list clamp of 100 but polled every 2s by the dashboard.
- `issue_matches_locator` / `project_matches_locator` are free functions in
  the same file's helper region (~`:1789+`). **Read them before Step 2** —
  the WHERE clause you write must replicate exactly what they match (id,
  identifier, possibly `TEAM-123` case behavior or bare number). This plan
  intentionally does not transcribe them; matching semantics are the thing
  you must preserve.
- Existing schema indexes (migration
  `m20260430_000001_create_asahi_schema.rs:105-160`): `idx_issues_identifier`,
  `idx_comments_issue_id`, `idx_issue_labels_issue_id`,
  `idx_issue_relations_issue_id`, plus notification/activity indexes. Plan 004
  adds unique indexes on identifier and `(team_key, number)`.
- Safety net: plan 007's API integration tests (wiki/projects/notifications)
  plus the 11 issues tests — all exercise these paths end-to-end through
  `sqlite::memory:`.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Tests | `cargo test -p asahi --locked` | exit 0 |
| Full workspace | `cargo test -p luna -p asahi --locked` | exit 0 (luna's asahi-tracker tests hit these endpoints via HTTP fixtures) |
| Lint | `cargo clippy -p asahi --all-targets --no-deps` | exit 0 |

## Scope

**In scope**:
- `crates/asahi/src/service/mod.rs` — query shapes and private helpers only

**Out of scope**:
- Any API handler, DTO, or response shape — this is a pure read-path refactor;
  callers must not notice.
- Schema/migrations — the indexes needed already exist (or arrive via 004).
- Pagination (audit PERF-05) — separate contract change, not planned here.
- `apps/asahi-web/` — no client changes.

## Git workflow

- Branch: `advisor/008-asahi-query-batching`
- Commit style: conventional commits, matching repo history. Suggested:
  `perf: batch asahi issue hydration`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Batch `hydrate_issues` to constant query count

Rewrite `hydrate_issues` to, given the page of `models`:
1. Collect `issue_ids`, distinct `project_id`s.
2. One query: `issue_label::Entity` with `Column::IssueId.is_in(issue_ids)`,
   ordered by name; group into `HashMap<issue_id, Vec<String>>`.
3. One query: `project::Entity` with `Column::Id.is_in(project_ids)` →
   `HashMap<id, ProjectRef>`.
4. One query: `issue_relation::Entity` with `Column::IssueId.is_in(issue_ids)`
   → per-issue blocker-id lists.
5. One query: `issue::Entity` with `Column::Id.is_in(all_blocker_ids)` →
   `BlockerRef` map (id, identifier, state — same fields as today, `:1506-1510`).
6. Assemble via the existing `model_to_issue(model, project, labels, blockers)`
   in the original order of `models`.

Keep `hydrate_issue` (single) delegating to the batch fn with a one-element
vec, so there is exactly one assembly code path. Preserve today's per-issue
label ordering (name asc) and blocker ordering (relation order — note
`:1503-1505` preserves `blocker_ids` order via `filter_map` over the map;
replicate that).

**Verify**: `cargo test -p asahi --locked` → exit 0 (issues tests assert
labels/blockers content and ordering).

### Step 2: Replace `find_issue_id` and project locator scans with WHERE clauses

1. Read `issue_matches_locator` and `project_matches_locator` and enumerate
   the match forms in a comment above the new code.
2. `find_issue_id`: build a single `issue::Entity::find().filter(...)` whose
   condition ORs the forms (e.g. `Column::Id.eq(locator)` OR
   `Column::Identifier.eq(normalized)` — with whatever case normalization the
   helper applies; SeaORM `Condition::any()`). Return `.one(&self.db)`s id.
   If a match form cannot be expressed as a column predicate (e.g. it matches
   against a *derived* value that isn't stored), see STOP conditions.
3. `find_project_model` / `find_project_model_by_slug`: same treatment
   (slug equality is `eq_ignore_ascii_case` — SQLite `LIKE` without wildcards
   or `Expr::cust` lower() comparison; pick one and note SQLite-specific
   collation in a comment).
4. `resolve_issue_locators`: resolve all locators in ONE query (condition-any
   across all locators), then map results back per-locator to preserve the
   per-locator `IssueNotFound` error (`:1427`) and output order.
5. Keep `issue_matches_locator`/`project_matches_locator` if other callers
   remain; if the SQL now fully covers them and nothing else calls them,
   delete them (compiler tells you).

**Verify**: `cargo test -p asahi --locked` → exit 0 — the issues tests create
by `blocked_by: ["ENG-1"]` (identifier locator) and fetch by locator forms;
`cargo test -p luna --locked` → exit 0.

### Step 3: Batch notification hydration

Apply the Step-1 pattern to `hydrate_notifications` (~`:1635-1656`): collect
distinct `issue_id`s, one `is_in` query, map in memory.

**Verify**: `cargo test -p asahi notifications --locked` → exit 0 (tests from
plan 007).

### Step 4: Equivalence spot-check on locator semantics

Add service-level tests (a `#[cfg(test)]` module in `service/mod.rs`, or
extend plan 004's if present) pinning locator behavior: resolve by full id,
by identifier exact case, by identifier different case (encode whatever the
OLD `issue_matches_locator` did — check the helper before deciding the
expected value), unknown locator → `Ok(None)` / `IssueNotFound` as today.

**Verify**: `cargo test -p asahi service:: --locked` → new tests pass.

## Test plan

Primary net: the existing + plan-007 integration suites (no changes needed —
that's the point). New tests: the locator-equivalence set in Step 4 (≥4
cases). Full gate: `cargo test -p luna -p asahi --locked` → all pass.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo test -p luna -p asahi --locked` exits 0
- [ ] `grep -n 'Entity::find().all(&self.db)' crates/asahi/src/service/mod.rs` → no matches in `find_issue_id`, `find_project_model`, `find_project_model_by_slug`
- [ ] `hydrate_issues` contains no per-model `.await` loop (inspect: the fn body issues a fixed number of queries)
- [ ] ≥4 new locator-equivalence tests exist and pass
- [ ] `cargo clippy -p asahi --all-targets --no-deps` exits 0
- [ ] `git status` shows only `crates/asahi/src/service/mod.rs` modified
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- `issue_matches_locator` matches on a value that is not stored in a column
  (derived formatting, cross-field composite) that SQL cannot replicate
  faithfully — report the exact matching rules found; a partial port would
  silently change lookup behavior.
- Plan 007's tests have not landed and `api/wiki.rs`/`api/projects.rs`/
  `api/notifications.rs` still have ≤1 test each — the net this refactor
  assumes is missing; execute 007 first or get operator sign-off.
- Any ordering assertion fails after Step 1 and the fix isn't the documented
  order-preservation detail (blocker order via `blocker_ids` iteration).

## Maintenance notes

- Pagination (unbounded `list_*` endpoints, audit PERF-05) is the natural
  next step and becomes much cheaper after this batching; it needs an API
  contract decision (limit/cursor params + dashboard changes), so it stayed
  out of scope.
- If `IssueService` is later split per-domain (audit DEBT-02), the batch
  hydration helpers become the shared mapper module's core — keep them free of
  `self` where easy.
- Reviewer: the diff to scrutinize is Step 2's OR-condition vs the old
  `issue_matches_locator` — ask for the enumerated match forms comment and
  check them against the deleted/retained helper line by line.
