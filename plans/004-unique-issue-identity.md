# Plan 004: Enforce unique issue identity in Asahi (fix the concurrent-create race)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- crates/asahi/src/service/mod.rs crates/asahi/src/migration/`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition. In particular, if a migration named
> like `*unique_issue*` already exists, this plan may already be done — report.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: plans/001-ci-test-baseline.md
- **Category**: bug
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

`create_issue` computes the next issue number by reading `MAX(number)+1`
*outside* the insert transaction, and the schema has **no unique constraint**
on `identifier` or `(team_key, number)`. Two concurrent `POST /api/issues` for
the same team both read the same max and both insert — producing two issues
named e.g. `ASAHI-7`. Luna keys dispatch, reconciliation, and workspace
directory names on the identifier, so a collision corrupts orchestrator state
(two issues sharing one workspace). Rocket serves requests concurrently, and
both the dashboard and running agents create issues, so this is reachable in
normal use.

## Current state

- `crates/asahi/src/service/mod.rs:1331-1340` — the racy allocator (note
  `&self.db`, not a transaction):

  ```rust
  async fn next_issue_number(&self, team_key: &str) -> ServiceResult<i64> {
      let latest = issue::Entity::find()
          .filter(issue::Column::TeamKey.eq(team_key.to_string()))
          .order_by_desc(issue::Column::Number)
          .one(&self.db)
          .await?;
      Ok(latest.map(|issue| issue.number + 1).unwrap_or(1))
  }
  ```

- `crates/asahi/src/service/mod.rs:35-99` — `create_issue`: number and
  identifier are computed **before** `self.db.begin()`:

  ```rust
  let number = self.next_issue_number(&team_key).await?;   // line ~52
  let now = Utc::now();
  let id = Uuid::new_v4().to_string();
  let identifier = format!("{team_key}-{number}");          // line ~55
  ...
  let transaction = self.db.begin().await?;                 // line ~58
  issue::ActiveModel { ..., identifier: Set(identifier), team_key: Set(team_key),
      number: Set(number), ... }.insert(&transaction).await?;
  // ... labels and relations inserted in the same transaction ...
  transaction.commit().await?;
  ```

- `crates/asahi/src/migration/m20260430_000001_create_asahi_schema.rs:105-111`
  — the identifier index is **non-unique** (the local `create_index` helper at
  `:188-211` builds `Index::create().name(name).table(table).col(column)` with
  no `.unique()`):

  ```rust
  create_index(manager, "idx_issues_identifier", Issues::Table, Issues::Identifier).await?;
  ```

- `crates/asahi/src/migration/mod.rs` — migrations are registered in a vec;
  last entry is `m20260501_000005_remove_synthetic_default_project`. New
  migrations follow the `mYYYYMMDD_NNNNNN_snake_name.rs` pattern and are
  appended to both the `mod` list and the `migrations()` vec.
- Error type: `ServiceError` (`service/mod.rs` bottom region, ~`:1770`) with a
  `From<DbErr>` conversion; API maps it in `crates/asahi/src/api/error.rs`.
- DB is SQLite via SeaORM/sqlx. Migrations run automatically on connect
  (`crates/asahi/src/db.rs`, `connect_and_setup`), including for the
  `sqlite::memory:` test harness.
- Test convention: Rocket integration tests in `crates/asahi/src/api/issues.rs`
  use `Client::tracked(app::rocket_with_database_url("sqlite::memory:"))`
  (`:344`) — but `Client` is the *blocking* local client, unsuitable for a
  concurrency test. For concurrency, construct the service directly with a
  `DatabaseConnection` from `crate::db::connect_and_setup("sqlite::memory:")`
  inside a `#[tokio::test]` in `service/mod.rs`'s test module (create one if
  the module has none — it currently has zero tests).

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Tests | `cargo test -p asahi --locked` | exit 0 |
| Targeted | `cargo test -p asahi service:: --locked` | exit 0 |
| Lint | `cargo clippy -p asahi --all-targets --no-deps` | exit 0 |

## Scope

**In scope**:
- `crates/asahi/src/migration/mod.rs`
- `crates/asahi/src/migration/m20260703_000006_unique_issue_identity.rs` (create)
- `crates/asahi/src/service/mod.rs` (allocation logic + tests)

**Out of scope**:
- `crates/asahi/src/entity/issue.rs` — column definitions don't change.
- `crates/asahi/src/api/` — no handler/response-shape changes.
- `crates/luna/` — nothing on the orchestrator side.
- Existing migration files — never edit an applied migration; only add.

## Git workflow

- Branch: `advisor/004-unique-issue-identity`
- Commit style: conventional commits, matching repo history. Suggested:
  `fix: enforce unique issue identifiers`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Write the migration — dedupe then constrain

Create `crates/asahi/src/migration/m20260703_000006_unique_issue_identity.rs`:

**up()**, in order:
1. **Dedupe pass** (raw SQL via `manager.get_connection().execute_unprepared`
   is acceptable here; SQLite dialect): for every group of rows sharing
   `(team_key, number)` beyond the first (ordered by `created_at`, then `id`),
   reassign `number` to `1 + MAX(number)` for that `team_key` at that moment
   (allocate sequentially per duplicate), and regenerate
   `identifier = team_key || '-' || number` and
   `url = '/api/issues/' || identifier` for the moved rows. Simplest correct
   SQLite approach: SELECT duplicates into Rust, loop and UPDATE row-by-row
   inside the migration (SeaORM `SchemaManager` exposes the connection; the
   migration runs before the unique index exists, so intermediate states are
   fine). Also dedupe on `identifier` alone the same way (covers rows where
   identifier was hand-set).
2. Create `uq_issues_team_key_number`: unique index on
   `issues (team_key, number)`.
3. Create `uq_issues_identifier`: unique index on `issues (identifier)`.
   Leave the existing non-unique `idx_issues_identifier` alone (harmless;
   dropping it is optional and riskier than keeping it).

Guard both creations with `manager.has_index(...)` like the existing helper
(`m20260430_000001_create_asahi_schema.rs:198`), but build with `.unique()`:

```rust
Index::create().name("uq_issues_identifier").table(Issues::Table)
    .col(Issues::Identifier).unique().to_owned()
```

Define the `Issues` iden enum locally in the new file (copy the pattern from
the existing migration — each migration file owns its idens; do not import
across migration files).

**down()**: drop the two unique indexes. (The dedupe is not reversible; that's
normal for data migrations.)

Register the module and append `Box::new(...)` in `migration/mod.rs`.

**Verify**: `cargo test -p asahi --locked` → exit 0 (every test bootstraps
`sqlite::memory:` through `Migrator::up`, so a broken migration fails loudly).

### Step 2: Allocate the number inside the transaction, retry on conflict

In `create_issue` (`service/mod.rs:35-99`):

1. Move `self.db.begin()` **before** number allocation. Change
   `next_issue_number` to take a connection generic (SeaORM pattern:
   `&(impl ConnectionTrait)`) so it can run on the transaction.
2. Wrap the allocate-and-insert of the `issue::ActiveModel` in a bounded retry
   (max 5 attempts): on a unique-constraint violation, roll back (or re-begin),
   re-read the max, and retry with the next number. Detect the violation by
   inspecting the `DbErr` — for SQLite via sqlx the error string contains
   `UNIQUE constraint failed: issues.` ; write a small helper
   `fn is_unique_violation(err: &DbErr) -> bool` that checks
   `err.to_string().contains("UNIQUE constraint failed")`, with a comment that
   this is SQLite-specific.
3. On exhausting retries, return the underlying `DbErr` via the existing
   `From<DbErr> for ServiceError` path.
4. Labels/relations inserts and `commit` stay as they are, inside the same
   (final, successful) transaction.

Note: SQLite serializes writers, so within one process the transaction move
alone nearly closes the race; the unique index is the actual guarantee (also
against multi-process access to the same DB file, e.g. `luna` embedded +
standalone `asahi` pointed at one file).

**Verify**: `cargo test -p asahi --locked` → exit 0 (the existing
`manages_issue_lifecycle` test in `api/issues.rs` asserts `ENG-1`/`ENG-2`
sequencing and must still pass unchanged).

### Step 3: Add the regression tests

In a `#[cfg(test)]` module in `service/mod.rs` (create it at the bottom of the
file):

1. `duplicate_identifier_insert_is_rejected` — `#[tokio::test]`: connect via
   `crate::db::connect_and_setup("sqlite::memory:")`, insert one issue row via
   the entity API with a fixed identifier/team_key/number, then attempt a
   second raw insert with the same `(team_key, number)`; assert it errors and
   `is_unique_violation` returns true for the error.
2. `concurrent_creates_yield_distinct_identifiers` — `#[tokio::test]`: one
   `IssueService` over one connection; `futures::future::join_all` (or
   `tokio::join!`) of 8 `create_issue` calls with the same `team_key`; assert
   8 successes and 8 distinct identifiers. (Note: `sqlite::memory:` with a
   sqlx pool — each pooled connection gets its own in-memory DB. Use the
   shared-cache URI form `sqlite:file:concurrent_test?mode=memory&cache=shared`
   or set the pool to a single connection so all tasks see one database;
   whichever `connect_and_setup` supports. If neither works within
   `connect_and_setup`'s signature, fall back to a `tempfile`-backed sqlite
   file — `tempfile` is already a workspace dev-dependency in `crates/luna`;
   add it to `crates/asahi`'s `[dev-dependencies]` if needed. A file-backed DB
   is the honest multi-connection test anyway.)

**Verify**: `cargo test -p asahi service:: --locked` → both new tests pass.

## Test plan

Covered by Step 3 (the two regression tests) plus the untouched
`manages_issue_lifecycle` integration test proving sequential numbering still
yields `ENG-1`, `ENG-2`. Full gate: `cargo test -p asahi --locked` → all pass.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo test -p asahi --locked` exits 0, including the 2 new tests
- [ ] New migration file exists and is registered (`grep -n 'unique_issue_identity' crates/asahi/src/migration/mod.rs` → 2 hits)
- [ ] `grep -n 'unique()' crates/asahi/src/migration/m20260703_000006_unique_issue_identity.rs` → ≥2 hits
- [ ] In `create_issue`, `begin()` precedes number allocation (inspect: `grep -n -A3 'pub async fn create_issue' crates/asahi/src/service/mod.rs` region)
- [ ] `cargo clippy -p asahi --all-targets --no-deps` exits 0
- [ ] `git status` shows only in-scope files modified
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- A `*unique_issue*` migration already exists (drift check) — the fix may have
  landed independently; verify and mark the plan accordingly instead of
  duplicating a migration.
- The dedupe pass in Step 1 turns out to need to touch tables other than
  `issues` (e.g. you discover identifiers denormalized into another table) —
  report what references them.
- `connect_and_setup` cannot produce a usable shared in-memory or file-backed
  DB for the concurrency test after two attempts — land Step 1–2 plus test 1,
  and report test 2 as blocked with what you tried.
- Any existing test asserts duplicate identifiers are permitted (none found at
  planning time, but if one exists, the intent conflict goes to the operator).

## Maintenance notes

- `is_unique_violation` is SQLite-string-based; if Asahi ever supports
  Postgres, that helper is the first thing to generalize (SeaORM exposes
  `SqlErr::UniqueConstraintViolation` on newer versions — worth checking at
  execution time; prefer it over string matching if available on the pinned
  sea-orm 1.1.x).
- Reviewer: scrutinize the dedupe SQL against a DB that actually contains
  duplicates (craft one in a scratch file with two identical `(team_key,
  number)` rows) — the renumber-and-regenerate step is the only risky code.
- Deferred: `update_issue` paths that let callers set `identifier`/`team_key`
  directly (if any) now surface unique violations as 500-ish errors; mapping
  them to a 409 in `api/error.rs` is a nice follow-up, not required here.
