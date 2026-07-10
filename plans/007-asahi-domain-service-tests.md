# Plan 007: Test Asahi's validation layer and the untested API surfaces (wiki, projects, notifications)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- crates/asahi/src/domain/ crates/asahi/src/api/`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: plans/001-ci-test-baseline.md
- **Category**: tests
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

Asahi's issues API is well-tested (11 integration tests incl. concurrency and
notification dedup), but the rest of the crate is nearly bare: the entire
`domain/` validation layer has **zero** tests, `api/notifications.rs` has
**zero** tests, and `api/wiki.rs` / `api/projects.rs` have one each — while
the service layer behind them (wiki versioning/rollback, project lifecycle,
notification read/archive) is 2,000+ lines exercised only through the issues
path. These are the trust boundary (validation) and the features agents and
the dashboard rely on. This plan also unblocks plan 008, which refactors the
service query layer and needs a net under it.

## Current state

- Test counts at `74ad45b` (from the audit, spot-verified): `domain/issue.rs`,
  `domain/wiki.rs`, `domain/project.rs`, `domain/notification.rs`,
  `domain/comment.rs`, `domain/activity.rs` — 0 tests each.
  `api/notifications.rs` — 0. `api/wiki.rs` — 1. `api/projects.rs` — 1.
  `api/issues.rs` — 11 (the healthy exemplar). `service/mod.rs` — 0 direct.
- The proven integration-test harness, `crates/asahi/src/api/issues.rs:342-366`:

  ```rust
  #[test]
  fn manages_issue_lifecycle() {
      let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
          .expect("valid rocket instance");
      let created = client.post("/api/issues")
          .header(ContentType::JSON)
          .body(r#"{ "project_slug": "engineering", "team_key": "ENG",
                     "title": "Build the HTTP tracker API", ... }"#)
          .dispatch();
      assert_eq!(created.status(), Status::Ok);
      let issue: Issue = created.into_json().expect("issue json");
      assert_eq!(issue.identifier, "ENG-1");
      ...
  }
  ```

  Migrations run automatically on connect, so each `Client::tracked(...)`
  gets a fresh, fully-migrated in-memory DB. Response DTOs used in tests are
  imported from the api modules (e.g. `NotificationListResponse`,
  `ProjectListResponse` — see the `use` block at `api/issues.rs:334-340`).

- `domain/issue.rs` shape (verified):

  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "PascalCase")]
  pub enum IssueState {
      Backlog, Todo,
      #[serde(rename = "In Progress")] InProgress,
      Done,
  }
  impl IssueState { pub const ALL: &[IssueState] = &[...]; }
  // plus Display and FromStr impls (FromStr's Err = String)
  ```

  Other domain files follow the same style: plain types + `garde`-validated
  input structs. The negative-validation coverage that exists today is only
  the two `rejects_invalid_state_*` cases in `api/issues.rs` (~`:779,:792`).

- Wiki specifics worth testing (from the schema/migrations): versioned pages
  with an audit trail (`entity/wiki_page_version.rs`, `wiki_audit.rs`;
  migration `m20260501_000004_create_project_wiki`), service methods
  `create_wiki_node`, `update_wiki_node`, `rollback_wiki_page`,
  `list_wiki_audits` (`service/mod.rs:337-728` region). Routes in
  `api/wiki.rs`.
- Notification semantics (service `service/mod.rs:1045-1125` region): list
  with unread filter + clamped limit, `read`/`unread`/`archive` transitions,
  `upsert_notification` dedup per issue (the dedup itself is asserted from the
  issues tests — don't duplicate those assertions; test the *endpoints*).
- Conventions (from `crates/asahi/AGENTS.md`): handlers thin, `Result<T,
  ApiError>`; validation in `domain/` via garde; business logic in `service/`.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Tests | `cargo test -p asahi --locked` | exit 0 |
| Targeted | `cargo test -p asahi domain:: --locked`, `cargo test -p asahi wiki --locked`, etc. | exit 0 |
| Lint | `cargo clippy -p asahi --all-targets --no-deps` | exit 0 |

## Scope

**In scope** (test code only — `#[cfg(test)]` modules and assertions):
- `crates/asahi/src/domain/*.rs` — add unit tests
- `crates/asahi/src/api/notifications.rs`, `api/wiki.rs`, `api/projects.rs` —
  add integration tests

**Out of scope** (do NOT touch):
- Any production code path. If a test reveals a bug, WRITE THE TEST to
  document current behavior with a `// BUG:` comment (or `#[ignore]` with a
  reason if current behavior is unacceptable to encode), report it, and do
  NOT fix it in this plan.
- `crates/asahi/src/service/mod.rs` — its behavior is covered *through* the
  API tests here; direct service tests come with plan 008's refactor.
- `api/issues.rs` — already covered; don't add or reshuffle.

## Git workflow

- Branch: `advisor/007-asahi-domain-service-tests`
- Commit style: conventional commits, matching repo history. Suggested:
  `test: cover asahi validation and api surfaces`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Domain unit tests

Add a `#[cfg(test)] mod tests` to each domain file that has logic worth
pinning (skip pure data structs with no impls):

- `domain/issue.rs`: `IssueState` round-trips — `FromStr` accepts each of
  `ALL` (and check case behavior: try `"in progress"` lowercase and encode
  whatever it currently does), `Display` matches the serde names
  (`"In Progress"` with the space), serde serialize/deserialize round-trip,
  unknown state string → `Err`.
- `domain/wiki.rs`, `domain/project.rs`, `domain/comment.rs`,
  `domain/notification.rs`, `domain/activity.rs`: for each garde-validated
  input struct, one accept case and one reject case **per validation rule**
  (empty/blank required field, length/range bounds — read the `#[garde(...)]`
  attributes and cover each). Call `.validate()` directly and assert
  `is_ok()`/`is_err()`.

**Verify**: `cargo test -p asahi domain:: --locked` → all new tests pass.

### Step 2: Notifications API integration tests

In `api/notifications.rs`, add a test module using the
`Client::tracked(app::rocket_with_database_url("sqlite::memory:"))` harness.
Seed data by POSTing an issue and a comment through the public API (comments
produce notifications — the issues tests demonstrate this). Cover:

1. list returns the seeded notification; `unread_only` filter excludes read
   ones; the limit parameter clamps.
2. `unread_count` reflects reads.
3. read → unread → archive transitions round-trip through their endpoints and
   are reflected in subsequent lists.
4. acting on a nonexistent notification id → the error status the handler
   maps to (read `api/error.rs` to know the expected code — assert the actual
   contract, don't guess).

**Verify**: `cargo test -p asahi notifications --locked` → all pass.

### Step 3: Wiki API integration tests

In `api/wiki.rs`'s test module (extend the existing single test's style):

1. create page → read it back (content + metadata).
2. update page → version count increments; both versions retrievable if the
   API exposes versions (read the routes first; test what exists).
3. rollback to version 1 → content matches version 1, and the audit trail
   (`list_wiki_audits` route) records create/update/rollback entries in order.
4. folder/node nesting if `create_wiki_node` distinguishes folders vs pages
   (read `domain/wiki.rs` to see the node kinds) — create nested structure,
   list/tree endpoint returns it.
5. invalid input reject: blank title/slug per the garde rules.

**Verify**: `cargo test -p asahi wiki --locked` → all pass.

### Step 4: Projects API integration tests

In `api/projects.rs`'s test module:

1. create → list → fetch-by-locator (slug and id forms if supported).
2. update fields round-trip.
3. delete: what happens to issues attached to the project? Read
   `service/mod.rs` `delete_project` (~`:249`) FIRST and encode its actual
   behavior (orphan? cascade? refuse?) — with a comment stating it's a
   characterization test.
4. duplicate slug create → assert the current behavior (likely error; see
   `normalize_slug`), with the same characterization framing.

**Verify**: `cargo test -p asahi projects --locked` → all pass.

## Test plan

This plan IS the test plan. Target: ≥25 new tests (roughly: ~10 domain, ~6
notifications, ~6 wiki, ~4 projects). Structural pattern for API tests:
`api/issues.rs:342` (`manages_issue_lifecycle`). Pattern for domain tests:
plain `#[test]` fns, no fixtures needed.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo test -p asahi --locked` exits 0
- [ ] `grep -rc '#\[test\]' crates/asahi/src/domain/ | awk -F: '{s+=$2} END {print s}'` ≥ 10
- [ ] `grep -c '#\[test\]' crates/asahi/src/api/notifications.rs` ≥ 4
- [ ] `grep -c '#\[test\]' crates/asahi/src/api/wiki.rs` ≥ 5 (was 1)
- [ ] `grep -c '#\[test\]' crates/asahi/src/api/projects.rs` ≥ 4 (was 1)
- [ ] `git diff --stat` shows NO changes outside `#[cfg(test)]` regions / test modules
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- A test exposes a real bug (e.g. rollback loses versions, delete_project
  cascades destructively, a validation rule is dead). Encode current behavior
  as a characterization test with a `// BUG:`/`// CHARACTERIZATION:` comment
  and report it — do not fix production code.
- The wiki routes differ substantially from the create/update/rollback/audit
  surface described here (drift or misread) — report the actual route list
  before writing tests against imagined endpoints.
- Any new test needs production code changes to be testable (missing derive,
  private type) beyond adding `#[derive(Debug/PartialEq)]` or `pub(crate)`
  visibility to a DTO — those two narrow exceptions are permitted; anything
  more, stop.

## Maintenance notes

- Plan 008 (query batching) refactors `service/mod.rs` under these tests —
  they are its safety net; land this plan first.
- Characterization tests marked `// BUG:` are an inventory for follow-up
  plans; whoever triages them should search that marker.
- Reviewer: check the notification/wiki tests assert through public HTTP
  endpoints only (no reaching into `service::` internals) — that's what keeps
  them valid across the plan-008 refactor.
