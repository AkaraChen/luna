# Plan 001: Make CI run the test suites and add a one-command verification story

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- .github/workflows/ci.yml justfile`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: tests
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

The repo has ~174 Rust tests (orchestrator concurrency, retry scheduling, tracker
reconciliation, Asahi API lifecycle) but CI runs **zero** of them — the only gates
are `cargo fmt --check`, `cargo clippy`, and a web typecheck/lint. A regression in
the daemon's core loop merges green today. Every other plan in `plans/` relies on
`cargo test --workspace` as its verification gate, so this plan must land first.

## Current state

- `.github/workflows/ci.yml` — two jobs: `rust` (fmt + clippy) and `asahi-web`
  (`vp check`). No test invocation anywhere. Relevant excerpts:

  ```yaml
  # .github/workflows/ci.yml (rust job steps)
  - uses: actions/checkout@v4
    with:
      submodules: recursive
  - uses: dtolnay/rust-toolchain@stable
    with:
      components: clippy, rustfmt
  - uses: swatinem/rust-cache@v2
  - name: Check Rust formatting
    run: cargo fmt --all -- --check
  - name: Lint Rust
    run: cargo clippy --workspace --all-targets --no-deps
  ```

  ```yaml
  # .github/workflows/ci.yml (asahi-web job steps)
  - uses: actions/checkout@v4
  - uses: oven-sh/setup-bun@v2
    with:
      bun-version: 1.3.13
  - run: bun install --frozen-lockfile
  - name: Check Asahi web
    run: bun run --cwd apps/asahi-web vp check src AGENTS.md DESIGN.md
  ```

- `justfile` — recipes `install`, `asahi-frontend`, `asahi-backend`. No `test`
  recipe.
- The Rust workspace members are `crates/luna`, `crates/asahi`,
  `apps/asahi-desktop/src-tauri` (root `Cargo.toml`). `crates/luna` has a **path
  dependency on the `angel-engine/` git submodule**, so any job compiling Rust
  must check out with `submodules: recursive` (the existing `rust` job already
  does).
- Asahi tests spin up Rocket against `sqlite::memory:` — no external services
  needed (see `crates/asahi/src/api/issues.rs:344`,
  `Client::tracked(app::rocket_with_database_url("sqlite::memory:"))`).
- The web app has **no test script yet** — web tests are created by plan
  011-web-dashboard-tests, which also adds the CI step for them. Do not add a
  web test step here.
- The `asahi-desktop/src-tauri` crate needs platform GUI libs to build on Linux
  (see `.github/workflows/asahi-desktop-release.yml` installing
  `libwebkit2gtk-4.1-dev` etc.). To avoid pulling GUI deps into the test job,
  run tests per-crate (`-p luna -p asahi`) rather than `--workspace`.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Rust tests (local) | `cargo test -p luna -p asahi --locked` | exit 0, all pass |
| Lint | `cargo clippy --workspace --all-targets --no-deps` | exit 0 |
| Format check | `cargo fmt --all -- --check` | exit 0 |
| YAML sanity | `ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml")' \|\| python3 -c "import yaml,sys;yaml.safe_load(open('.github/workflows/ci.yml'))"` | exit 0 |
| just recipe | `just test` | runs the Rust tests, exit 0 |

## Scope

**In scope** (the only files you should modify):
- `.github/workflows/ci.yml`
- `justfile`

**Out of scope** (do NOT touch, even though they look related):
- `.github/workflows/asahi-desktop-release.yml` — release pipeline, unrelated.
- `apps/asahi-web/package.json` — the web `test` script belongs to plan 011.
- Any test source file — this plan wires existing tests into CI, nothing more.

## Git workflow

- Branch: `advisor/001-ci-test-baseline`
- Commit style: conventional commits, matching repo history (e.g.
  `ci: add lint and format checks` at `74ad45b`). Suggested:
  `ci: run rust test suites in ci and add just test recipe`.
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Verify the Rust tests pass locally

Run `cargo test -p luna -p asahi --locked` from the repo root. All tests must
pass before you change CI — if any fail at HEAD, that is a STOP condition
(report the failing tests; do not "fix" them in this plan).

**Verify**: `cargo test -p luna -p asahi --locked` → exit 0, `0 failed` in every suite summary.

### Step 2: Add a `test` job to `.github/workflows/ci.yml`

Add a third job alongside `rust` and `asahi-web`, mirroring the `rust` job's
setup (checkout with `submodules: recursive`, `dtolnay/rust-toolchain@stable`,
`swatinem/rust-cache@v2`), whose final step is:

```yaml
- name: Run Rust tests
  run: cargo test -p luna -p asahi --locked
```

Name the job `rust-test` ("Rust tests"). Keep it a separate job from `rust` so
lint failures and test failures report independently.

**Verify**: the YAML sanity command from the table → exit 0.

### Step 3: Add a bun dependency cache to the `asahi-web` job

In the existing `asahi-web` job, between `setup-bun` and `bun install`, add:

```yaml
- uses: actions/cache@v4
  with:
    path: ~/.bun/install/cache
    key: bun-${{ runner.os }}-${{ hashFiles('bun.lock') }}
    restore-keys: bun-${{ runner.os }}-
```

**Verify**: YAML sanity command → exit 0.

### Step 4: Add a `test` recipe to the justfile

Append to `justfile`:

```make
# Run all test suites.
test:
    cargo test -p luna -p asahi --locked
```

(When plan 011 adds web tests, it will extend this recipe with the web suite —
leave a trailing newline, nothing else.)

**Verify**: `just test` → runs the Rust tests, exit 0.

## Test plan

No new tests are written; the deliverable is that the ~174 existing tests run
in CI. Local verification stands in for the CI run: `just test` → exit 0.
If you have `act` or push access is later granted, the first CI run on the
branch is the real gate — but do not push as part of this plan.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `just test` exits 0
- [ ] `.github/workflows/ci.yml` contains a job running `cargo test -p luna -p asahi --locked` with `submodules: recursive` checkout
- [ ] `.github/workflows/ci.yml` `asahi-web` job contains an `actions/cache` step keyed on `bun.lock`
- [ ] `git status` shows only `.github/workflows/ci.yml` and `justfile` modified
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- Any test fails at HEAD in Step 1 — the baseline itself is broken; that's a
  finding, not something to patch here.
- `cargo test -p luna -p asahi` needs more than ~15 minutes locally — flag it;
  a slow suite changes how CI should be structured (splitting, nextest).
- The `justfile` no longer matches the excerpt in "Current state".

## Maintenance notes

- Plan 011 (web dashboard tests) extends both the CI workflow and the `just
  test` recipe with `vp test` — whoever executes it should append, not replace.
- If the `asahi-desktop` Tauri crate ever gets tests, the per-crate `-p` list
  must be revisited (it was chosen to avoid Linux GUI build deps in CI).
- Reviewer: check that the test job does NOT use `--workspace` (would drag in
  the Tauri crate's system deps on ubuntu runners).
