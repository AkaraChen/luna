# Plan 012: Fix stale docs and dev-loop breakage; realign the README with what the project now is

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- AGENTS.md crates/luna/AGENTS.md crates/asahi/AGENTS.md justfile README.md README.CN.md .gitignore`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (but see per-step notes about plans 002/005 — if they
  have landed, describe the new reality; if not, describe today's)
- **Category**: docs / dx
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

The agent-facing knowledge base and the dev loop have drifted from the code:
three AGENTS.md files route schema work to a crate that no longer exists, the
backend dev recipe watches that dead path, the desktop app is absent from all
documentation, and the README neither explains how to install the thing nor
mentions that the project now ships a full local tracker (Asahi API +
dashboard + Tauri desktop app) alongside the GitHub-Projects story. For a repo
where agents execute plans, actively-wrong docs are worse than missing ones —
they send every future executor to a nonexistent path.

## Current state

All verified at `74ad45b`.

- `crates/asahi-migration/` **does not exist** — folded into
  `crates/asahi/src/migration/` at commit `f80f11b`. Workspace members (root
  `Cargo.toml`): `crates/luna`, `crates/asahi`, `apps/asahi-desktop/src-tauri`.
  Yet:
  - Root `AGENTS.md:18` structure tree lists `crates/asahi-migration/`;
    `AGENTS.md:37` routes "Schema migrations" to `crates/asahi-migration/src/m*.rs`.
  - `crates/asahi/AGENTS.md` "WHERE TO LOOK" row: "Change issue schema |
    `entity/issue.rs` + migration in `asahi-migration`"; anti-pattern: "Do not
    alter schema without a corresponding migration in `asahi-migration`".
  - `justfile:17` (`asahi-backend` recipe): `cargo watch -w crates/asahi
    -w crates/asahi-migration -w Cargo.toml -w Cargo.lock -x 'run -p asahi'`.
- `apps/asahi-desktop/` (Tauri app + luna sidecar, own release workflow
  `asahi-desktop-release.yml`, root scripts `asahi-desktop:build`/`:dev`) is
  mentioned in **no** AGENTS.md and not in the README; it has no AGENTS.md of
  its own. The `angel-engine/` submodule is likewise absent from the root
  structure tree (which still lists only `akrc-docs/`).
- `README.md:56-75` "Getting Started" jumps straight to `luna init` with no
  build/install instructions and no toolchain prerequisites; build prereqs
  (Rust, bun 1.3.13, just, cargo-watch) are only discoverable from the
  justfile. Runtime prereqs (Codex, `gh`) ARE documented (`:74`).
- `README.md` tells only the GitHub-Projects story: "Asahi", "Linear",
  "wiki", "desktop", "opencode" appear nowhere — yet the shipped default
  `WORKFLOW.md` at repo root uses `tracker: kind: asahi`, and
  `crates/luna/src/tracker/mod.rs` builds three backends. CLI subcommands
  beyond `init`/`comment` (`show`, `move`, `job`, `wiki`, `asahi-desktop` —
  see `crates/luna/src/main.rs`) are undocumented.
- `README.md:45` promises permission profiles — **coordinate with plan 002**:
  if 002 landed, the claim is true (leave 002's wording); if not, soften to
  describe the real fields (`approval_policy`, `thread_sandbox`,
  `turn_sandbox_policy` — note they are currently not enforced; 002 fixes
  that). Check `plans/README.md` status before editing this line.
- `README.md:49` "Exponential backoff retry" is **correct** (verified:
  `schedule_retry` computes `10s·2^(attempt-1)` capped at `retry_backoff_ms`)
  — do NOT change it. If plan 005 landed, you may add "with a configurable
  attempt cap".
- `.env.luna` exists at repo root, is **untracked and gitignored**
  (`.gitignore` line `.env.luna`), holds empty `GH_TOKEN=`/`GITHUB_TOKEN=`
  placeholders, and there is no committed `.env.luna.example`. `main.rs`'s
  `load_dotenv_file` loads `.env.luna` from the workflow directory.
- `README.CN.md` mirrors `README.md` in Chinese and must receive equivalent
  edits.
- Per-crate CLAUDE.md files are symlinks to their sibling AGENTS.md (root
  `CLAUDE.md -> AGENTS.md`) — edit AGENTS.md only; symlinks follow.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Stale-path scan | `grep -rn 'asahi-migration' --include='*.md' --include='justfile' .` (excluding `.reference`, `akrc-docs`, `angel-engine`, `plans`) | 0 hits after Step 1 |
| Dev loop | `just asahi-backend` (start, observe watch paths, Ctrl-C) | no missing-path warnings from cargo-watch |
| Structure truth | `ls crates/ apps/` | matches what the docs claim |
| Rust untouched | `cargo test -p luna -p asahi --locked` | exit 0 (nothing should change) |

## Scope

**In scope**:
- `AGENTS.md` (root), `crates/asahi/AGENTS.md`, `crates/luna/AGENTS.md`
- `apps/asahi-desktop/AGENTS.md` + `apps/asahi-desktop/CLAUDE.md` symlink (create)
- `justfile`
- `README.md`, `README.CN.md`
- `.env.luna.example` (create), `.gitignore` (only if needed for the example file)

**Out of scope**:
- Any Rust/TS source file.
- `WORKFLOW.md` at repo root (the operator's live config).
- `apps/asahi-web/AGENTS.md`, `DESIGN.md` — accurate enough; not audited as stale.
- Positioning rewrite of the README's philosophy sections — Step 4 *adds*
  the missing surface; it does not re-headline the project (that's the
  maintainer's call, flagged in the audit as a decision, not a task).

## Git workflow

- Branch: `advisor/012-docs-dx-sweep`
- Commit style: conventional commits, matching repo history. Suggested:
  `docs: refresh luna and asahi onboarding`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Purge the dead `asahi-migration` references and fix the dev loop

1. `justfile:17`: remove `-w crates/asahi-migration` (the migration module now
   lives under the already-watched `crates/asahi`).
2. Root `AGENTS.md`: drop the `asahi-migration` line from the structure tree;
   change the "Schema migrations" row to `crates/asahi/src/migration/m*.rs`.
3. `crates/asahi/AGENTS.md`: fix the schema-change row and the anti-pattern
   line to point at `src/migration/` ("add a migration module in
   `crates/asahi/src/migration/` and register it in `migration/mod.rs`").

**Verify**: the stale-path scan command → 0 hits (outside `plans/`,
`.reference/`, `akrc-docs/`, `angel-engine/`); `just asahi-backend` starts
without a missing-watch-path warning (Ctrl-C after it compiles).

### Step 2: Document the desktop app and the submodule

1. Root `AGENTS.md`: add `apps/asahi-desktop/` (Tauri shell + luna sidecar)
   and `angel-engine/` (vendored agent-runtime client submodule, read-only)
   to the structure tree; add a WHERE-TO-LOOK row for the desktop app.
2. Create `apps/asahi-desktop/AGENTS.md` following the sibling format
   (`crates/asahi/AGENTS.md` is the template): overview (Tauri app that
   spawns `luna asahi-desktop` as a sidecar and points the webview at the
   local dashboard URL — read `apps/asahi-desktop/src-tauri/src/main.rs`
   first and describe what is actually there), structure (`src-tauri/`, `ui/`,
   `scripts/`), commands (`bun run --cwd apps/asahi-desktop dev` / `build`,
   the `prepare:sidecar` script, release via the `asahi-v*` tag workflow),
   conventions/anti-patterns you observe in the code — do not invent any.
3. `ln -s AGENTS.md apps/asahi-desktop/CLAUDE.md` (matching the repo's
   existing symlink pattern).

**Verify**: `test -L apps/asahi-desktop/CLAUDE.md && head -3 apps/asahi-desktop/AGENTS.md`
→ symlink exists, file has content; every path named in the new file exists
(`ls` each).

### Step 3: README Getting Started — prerequisites and install

In `README.md` (and mirrored in `README.CN.md`), extend "Getting Started"
with, before the existing CLI examples:

- **Prerequisites**: Rust (stable, 2024 edition), Bun 1.3.13, `just`
  (optional but recipes assume it); Codex CLI + `gh` for the default runtime
  and GitHub workflows (already stated at `:74` — consolidate, don't
  duplicate); `git submodule update --init` for `angel-engine` (required to
  build) — verify that claim: `crates/luna/Cargo.toml:28` path-depends on the
  submodule, so a fresh clone without submodules cannot build luna. State it.
- **Install**: `just install` (or `cargo install --path ./crates/luna --force
  --locked`).
- **Dashboard dev**: one line pointing at `just asahi-frontend` /
  `just asahi-backend` (port 49306).
- **Secrets**: copy `.env.luna.example` → `.env.luna` next to your
  WORKFLOW.md (see Step 5).

**Verify**: every command you wrote is copy-paste runnable from a clean shell
at repo root (actually run the harmless ones: `just --list`, `cargo --version`,
`bun --version`).

### Step 4: README — surface the Asahi stack and the real CLI

Add a compact section (suggested title "What's in the box", placed after
"What You Can Do") to `README.md`/`README.CN.md`:

- Trackers: GitHub Projects, **Asahi (embedded local tracker — the default
  scaffold)**, Linear. One sentence each; note the embedded Asahi auto-starts
  when WORKFLOW.md has no explicit tracker.
- Asahi surface: REST API + web dashboard (issues, projects, wiki,
  notifications) + Asahi Desktop (Tauri).
- Runners: Codex (default, via the vendored angel-engine runtime), opencode,
  ACP.
- CLI: one-line table for `luna init | comment | show | move | wiki | job` —
  read `crates/luna/src/main.rs`'s clap definitions first and list what
  exists, with each subcommand's one-line help text as the description.
- Update `README.md:45` per the coordination note in "Current state"
  (permission profiles: true if 002 landed, softened if not).

Keep the existing philosophy/vision sections untouched. Match the README's
existing tone (short declarative sentences, no marketing superlatives).

**Verify**: `grep -c 'Asahi' README.md` ≥ 3; every CLI subcommand listed
exists in `main.rs` (grep each); no claims about features you didn't verify
in code.

### Step 5: Commit a `.env.luna.example`

Create `.env.luna.example` at repo root with the same keys as the live file
but empty values and a comment header ("copy to .env.luna beside your
WORKFLOW.md; loaded automatically"). Copy the key NAMES from `.env.luna`
(`GH_TOKEN`, `GITHUB_TOKEN`, and any others present) — **never copy values**
(they're empty today; if you find a non-empty value, STOP: report the
credential type and location only, and recommend rotation). Confirm
`.gitignore` ignores `.env.luna` but not the example.

**Verify**: `git check-ignore .env.luna` → ignored;
`git check-ignore .env.luna.example` → exit 1 (not ignored);
`grep -c '=' .env.luna.example` ≥ 2 with all values empty.

## Test plan

Docs plan — no code tests. Gates: the per-step grep/ls verifications, plus
`cargo test -p luna -p asahi --locked` at the end to prove nothing executable
changed.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] Stale-path scan (`asahi-migration` outside plans/vendored dirs) → 0 hits
- [ ] `apps/asahi-desktop/AGENTS.md` exists; `apps/asahi-desktop/CLAUDE.md` is a symlink to it
- [ ] README has Prerequisites/Install covering submodule init, `just install`, and the dashboard recipes; README.CN.md mirrors it
- [ ] README names Asahi, Linear, opencode, and the full CLI subcommand list (each verified against `main.rs`)
- [ ] `.env.luna.example` committed with empty values; `.env.luna` still ignored
- [ ] `cargo test -p luna -p asahi --locked` exits 0
- [ ] `git status` shows only in-scope files
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- `.env.luna` contains any non-empty credential value (report type + location
  only; rotation advice; never the value).
- `apps/asahi-desktop/src-tauri/src/main.rs` doesn't match the sidecar
  description here closely enough to document confidently — describe what you
  actually read, and if it's ambiguous, ask rather than guess.
- A README claim you're asked to write can't be verified in code (e.g. a CLI
  subcommand behaves differently from its name) — document reality or omit.

## Maintenance notes

- The root AGENTS.md carries a "Generated: 2026-05-27 / Commit: 2bd65af"
  header — update the header to reflect this manual revision, or the next
  regeneration pass may clobber these fixes; note that in the file.
- Deferred deliberately: the README positioning decision (GitHub-first vs
  local-first headline — maintainer's call, audit DIRECTION-01), pre-commit
  hooks (DX-02), Renovate grouping (DEPS-09), and a `luna doctor`-style env
  check.
- Plans 002 and 005 touch overlapping README/scaffold lines — whoever runs
  last reconciles wording (both plans state their exact claims).
