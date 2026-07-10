# Plan 006: Dependency & binding hardening batch (dompurify, vite-plus pins, sea-orm/garde alignment, loopback bind)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- apps/asahi-web/package.json bun.lock crates/asahi/Cargo.toml crates/luna/Cargo.toml Cargo.lock crates/asahi/src/app.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW–MED (the garde bump is the only piece with API-change exposure)
- **Depends on**: plans/001-ci-test-baseline.md
- **Category**: security
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

Four independent, individually-small hardening items, batched because each is
mechanical: (1) `dompurify` is pinned in a range with published sanitizer-
bypass advisories, and it is the **only** thing standing between server-
supplied rich-text HTML and `dangerouslySetInnerHTML` in the dashboard;
(2) the `vite-plus` toolchain floats on `latest`, so local installs are
non-reproducible and the currently-resolved version carries advisories;
(3) `sea-orm-migration` is exact-pinned at `=1.1.19` while `sea-orm` floats —
they have already drifted to different patch releases, and `garde` exists at
two incompatible pre-1.0 versions across the workspace; (4) the standalone
`asahi` binary inherits Rocket's release-profile default bind address
(`0.0.0.0`), exposing the unauthenticated tracker API to the LAN, while the
luna-embedded path correctly pins loopback.

## Current state

- `apps/asahi-web/package.json` (verified at `74ad45b`):

  ```json
  "dependencies": {
    "@types/dompurify": "^3.2.0",
    "dompurify": "^3.4.5",
    ...
  },
  "devDependencies": {
    ...
    "vite": "npm:@voidzero-dev/vite-plus-core@latest",
    "vite-plus": "latest"
  },
  "overrides": {
    "vite": "npm:@voidzero-dev/vite-plus-core@latest",
    "vitest": "npm:@voidzero-dev/vite-plus-test@latest"
  }
  ```

  `bun audit` (run during the audit, rechecked 2026-07-03) flagged
  `dompurify <=3.4.6` (XSS / sanitizer-bypass cluster) and
  `vite-plus <=0.1.23` (critical Vitest-browser advisory, high Vite fs-deny
  bypass). `bun.lock` currently resolves `vite-plus@0.1.22`. DOMPurify usage:
  `apps/asahi-web/src/lib/sanitize.ts` (`sanitizeRichText`, tight allowlist).
  The same audit output also reports unrelated advisories under `shadcn` and
  transitive packages; do not scope-creep into those here.

- `crates/asahi/Cargo.toml`:

  ```toml
  sea-orm-migration = { version = "=1.1.19", ... }   # line 7 — exact pin
  garde = { version = "0.21", features = ["derive"] } # line 9
  sea-orm = { version = "1.1.19", ... }               # line 11 — caret
  ```

  `crates/luna/Cargo.toml:14`: `garde = { version = "0.22.1", features = ["derive"] }`.
  `cargo tree -d` (run during the audit) shows `sea-orm v1.1.20` resolved
  alongside `sea-orm-migration v1.1.19`, and both `garde 0.21.1` and `0.22.1`
  compiled. The vendored `angel-engine` submodule also uses garde 0.21 — that
  copy is out of scope and will remain; only first-party crates align.

- `crates/asahi/src/app.rs:14-45` — the asymmetry:

  ```rust
  pub fn rocket_with_database_url(database_url: impl Into<String>) -> Rocket<Build> {
      let database_url = database_url.into();
      rocket::build()                     // ← no address config: release default is 0.0.0.0
          .attach(AdHoc::try_on_ignite("Asahi Database", ...))
          .mount("/api", health::routes())
          ...
  }

  pub fn rocket_with_database_url_and_port(database_url, port) -> Rocket<Build> {
      let figment = rocket::Config::figment()
          .merge(("port", port))
          .merge(("address", "127.0.0.1"))   // ← embedded path pins loopback
          .merge(("cli_colors", false));
      rocket::custom(figment)...
  }
  ```

  The standalone binary (`crates/asahi/src/main.rs`) uses the first variant.
  There is no `Rocket.toml`. Figment semantics: `.merge` overrides
  env/profile values; `.join` supplies a *lower-priority* default that
  `ROCKET_ADDRESS` can still override.

- Test harness note: `crates/asahi/src/api/issues.rs:344` builds
  `Client::tracked(app::rocket_with_database_url("sqlite::memory:"))` — your
  change to that function must keep tests working (a `.join`-ed address does).

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| JS install | `bun install` (repo root) | exit 0, lockfile updated |
| JS audit | `cd apps/asahi-web && bun audit` | no dompurify / vite-plus critical/high advisories |
| Web check | `bun run --cwd apps/asahi-web vp check src AGENTS.md DESIGN.md` | exit 0 |
| Web build | `bun run --cwd apps/asahi-web build` | exit 0 |
| Rust tests | `cargo test -p luna -p asahi --locked` | exit 0 |
| Rust lint | `cargo clippy --workspace --all-targets --no-deps` | exit 0 |
| Bind check | `ROCKET_PORT=49316 cargo run -p asahi --release`, then `lsof -nP -iTCP:49316 -sTCP:LISTEN` | listener on `127.0.0.1:49316`, not `*:49316` |

## Scope

**In scope**:
- `apps/asahi-web/package.json`, `bun.lock`
- `crates/asahi/Cargo.toml`, `crates/luna/Cargo.toml`, root `Cargo.toml`
  (only if hoisting shared versions to `[workspace.dependencies]`), `Cargo.lock`
- `crates/asahi/src/app.rs`
- Source files ONLY as required by the garde 0.21→0.22 API delta (expected:
  none or attribute tweaks in `crates/asahi/src/domain/*.rs`)

**Out of scope**:
- `angel-engine/` — its garde 0.21 stays; do not touch the submodule pointer.
- Tauri crate deps (`apps/asahi-desktop/src-tauri/Cargo.toml`) — Tauri
  capability/CSP hardening is a separate audit finding, not planned here.
- Any behavioral change to `sanitizeRichText`'s allowlist.
- `renovate.json` — grouping rules deferred (see Maintenance notes).

## Git workflow

- Branch: `advisor/006-dependency-hardening-batch`
- Commit style: conventional commits, matching repo history. Suggested:
  `fix: harden asahi dependencies and bind address`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Bump dompurify past the advisory range, drop the stale types stub

In `apps/asahi-web/package.json`: set `dompurify` to a fixed-range spec at or
above the first patched release. Registry check on 2026-07-03:
`bun pm view dompurify version` returned `3.4.11`, so use
`"dompurify": "^3.4.11"` unless the registry has advanced by execution time.
Remove `@types/dompurify` (dompurify 3.x ships its own types). Run
`bun install`.

**Verify**: `cd apps/asahi-web && bun audit` → no dompurify advisories;
`bun run --cwd apps/asahi-web vp check src AGENTS.md DESIGN.md` → exit 0 (type
resolution still works without the stub);
`bun run --cwd apps/asahi-web build` → exit 0.

### Step 2: Pin the vite-plus toolchain to explicit patched versions

Confirm the latest patched versions with:

```bash
bun pm view vite-plus version
bun pm view @voidzero-dev/vite-plus-core version
bun pm view @voidzero-dev/vite-plus-test version
```

Registry check on 2026-07-03 returned `vite-plus@0.2.2`,
`@voidzero-dev/vite-plus-core@0.2.2`, and
`@voidzero-dev/vite-plus-test@0.1.24`. Replace every `latest` / `@latest`
with those explicit versions, keeping the alias *structure* intact:

```json
"vite": "npm:@voidzero-dev/vite-plus-core@0.2.2",
"vite-plus": "0.2.2",
"overrides": {
  "vite": "npm:@voidzero-dev/vite-plus-core@0.2.2",
  "vitest": "npm:@voidzero-dev/vite-plus-test@0.1.24"
}
```

If the registry has newer non-prerelease versions at execution time, use
those instead and record the exact versions in the executor report.
Run `bun install`.

**Verify**: `grep -c 'latest' apps/asahi-web/package.json` → 0;
`bun audit` in `apps/asahi-web` → no vite-plus critical/high advisories
(if the newest release still carries one, pin it anyway and record the
residual advisory in your report); `vp check` and web build → exit 0.

### Step 3: Align sea-orm and garde across first-party crates

1. In `crates/asahi/Cargo.toml`: make the sea-orm pair move in lockstep —
   either both exact (`sea-orm = "=1.1.20"`, `sea-orm-migration = "=1.1.20"`)
   or both caret at the same minor. Prefer hoisting into root `Cargo.toml`
   `[workspace.dependencies]` with `workspace = true` references if the edit
   stays small; otherwise same-file alignment is fine.
2. Bump asahi's `garde` to `0.22` to match luna. Fix any derive-attribute
   breakage in `crates/asahi/src/domain/*.rs` (expected minimal; garde 0.22's
   changelog is the reference — if the delta is more than attribute renames,
   see STOP conditions).
3. `cargo update -p sea-orm -p sea-orm-migration -p garde` (as needed) so
   `Cargo.lock` converges.

**Verify**: `cargo tree -d | grep -A2 '^garde'` → only one non-angel-engine
garde version; `cargo tree | grep 'sea-orm '` and `grep 'sea-orm-migration'`
→ same version; `cargo test -p luna -p asahi --locked` → exit 0.

### Step 4: Default the standalone Asahi server to loopback

In `crates/asahi/src/app.rs`, change `rocket_with_database_url` to build from
a figment with a **low-priority** loopback default:

```rust
let figment = rocket::Config::figment().join(("address", "127.0.0.1"));
rocket::custom(figment)
    .attach(...)   // rest unchanged
```

`.join` (not `.merge`) so `ROCKET_ADDRESS`/`Rocket.toml` can still opt into a
wider bind explicitly. Add a one-line comment: local-only tracker by design;
set `ROCKET_ADDRESS` to expose deliberately.

**Verify**: `cargo test -p asahi --locked` → exit 0 (the in-process test
client is bind-agnostic); then the bind check from the commands table on a
**release** build → listener shows `127.0.0.1:49316`. Also confirm the env
override still works:
`ROCKET_PORT=49317 ROCKET_ADDRESS=0.0.0.0 cargo run -p asahi --release`
→ listener on `*:49317`/`0.0.0.0:49317` (then stop it).

## Test plan

No new test files. The gates are: full Rust suites (`cargo test -p luna -p
asahi --locked`), web `vp check` + build, `bun audit` clean of the two named
advisory clusters, and the manual bind verification in Step 4 (both default
and env-override directions). If plan 011 (web tests) has landed first, also
run `vp test` — the sanitize tests it adds are the direct regression net for
the dompurify bump.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cd apps/asahi-web && bun audit` shows no dompurify or vite-plus critical/high advisories (or the residual is recorded in the report)
- [ ] `grep -c 'latest' apps/asahi-web/package.json` → 0
- [ ] `grep -n '@types/dompurify' apps/asahi-web/package.json` → no match
- [ ] `cargo tree -d` shows a single first-party garde version and a single sea-orm/sea-orm-migration version
- [ ] `cargo test -p luna -p asahi --locked` exits 0; web `vp check` and `bun run --cwd apps/asahi-web build` exit 0
- [ ] Release-mode standalone asahi listens on `127.0.0.1` by default and honors `ROCKET_ADDRESS` override
- [ ] `git status` shows only in-scope files modified
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The dompurify bump changes `sanitizeRichText` output for well-formed
  Tiptap content (spot-check by rendering an issue description in `vp dev` if
  in doubt) — a sanitizer behavior change needs human review.
- garde 0.22 requires more than attribute-level changes in
  `crates/asahi/src/domain/` (e.g. validator signature changes rippling into
  handlers) — the bump graduates from "alignment" to "migration"; report the
  delta size.
- Pinned vite-plus versions break `vp check`/`vp dev`/build and no adjacent
  patched version works after two attempts.
- `.join` on the figment does not produce a loopback default in release mode
  (verify empirically — if Rocket's provider precedence surprises, report
  rather than switching to `.merge`, which would break the env override).

## Maintenance notes

- Renovate currently extends only `config:recommended`; without grouping
  rules, the sea-orm pair (and React/@types) can drift apart again. Adding
  `packageRules` grouping `sea-orm*`, `tauri*`, and `react*` + enabling
  `lockFileMaintenance` is the cheap prevention — deferred (one-file change,
  do it opportunistically).
- The Tauri webview's `shell:allow-execute` capability + `csp: null`
  (audit SECURITY-04) remains open — separate decision, not planned here.
- Reviewer: the only subtle hunk is Step 4's `.join` vs `.merge` — check the
  verification evidence for both bind directions is in the executor's report.
