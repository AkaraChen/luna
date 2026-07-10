# Plan 002: Make agent permission/sandbox configuration real instead of hardcoded

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- crates/luna/src/config.rs crates/luna/src/agent/angel_runtime.rs crates/luna/src/job.rs crates/luna/src/init.rs README.md README.CN.md`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: plans/001-ci-test-baseline.md
- **Category**: security
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

Luna dispatches coding agents into git worktrees of the user's real repository,
feeding them untrusted tracker content (issue titles, bodies, comments). Today
the agent's permission posture is hardcoded: permission mode `"never"` (Codex)
/ `"bypassPermissions"` (Opencode), and every mid-turn approval request is
auto-granted for the whole session. Meanwhile the `WORKFLOW.md` config fields
that *look* like they control this (`approval_policy`, `thread_sandbox`,
`turn_sandbox_policy`) are parsed, validated, scaffolded by `luna init` — and
**never read by any production code**. An operator who hardens their workflow
gets zero enforcement and no warning. The README additionally promises named
permission profiles (`high_trust`, `workspace_write`, `read_only`) that the
config parser actively rejects. This plan wires the config through for real,
keeping today's permissive behavior as the explicit default (it was a
deliberate choice — commit `07589e6 "fix(luna): allow angel runtime permissions
by default"`), so nothing breaks for existing users.

## Current state

- `crates/luna/src/config.rs:223-247` — `CodexRunner` declares the inert fields:

  ```rust
  #[derive(Clone, Debug, Deserialize, Validate)]
  #[serde(deny_unknown_fields)]
  pub struct CodexRunner {
      ...
      #[garde(skip)]
      pub approval_policy: Option<JsonValue>,
      #[garde(inner(custom(not_blank)))]
      pub thread_sandbox: Option<String>,
      #[garde(skip)]
      pub turn_sandbox_policy: Option<JsonValue>,
      ...
  }
  ```

  `OpencodeRunner` (`config.rs:264-282`) has no permission fields at all.
  Because of `deny_unknown_fields`, adding any new key (e.g.
  `permission_profile`) to WORKFLOW.md today is a hard parse error — there is
  an explicit test named `rejects_permission_profile_on_runner` around
  `config.rs:1103-1123` asserting that.

- `crates/luna/src/agent/angel_runtime.rs:47-68` — the hardcoded modes:

  ```rust
  fn codex(config: &CodexRunner) -> Self {
      Self { ..., default_permission_mode: "never".to_string() }
  }
  fn opencode(config: &OpencodeRunner) -> Self {
      Self { ..., default_permission_mode: "bypassPermissions".to_string() }
  }
  ```

- `angel_runtime.rs:186-198` — `launch_runtime` builds
  `create_runtime_options(Some(kind), RuntimeOptionsOverrides { command, args,
  cwd, process_label, client_name, client_title, default_reasoning_effort,
  ..default() })`. The configured `approval_policy`/`thread_sandbox`/
  `turn_sandbox_policy` are never passed. (Their only other appearance in the
  file is `None` initializers inside a `#[cfg(test)]` fixture at lines 618-620.)

- `angel_runtime.rs:285-315` — `default_permission_mode` is sent as
  `SendTextRequest.permission_mode` on `start()` and every `run_turn()`.

- `angel_runtime.rs:356-367` — every elicitation is auto-approved:

  ```rust
  ProjectedAngelEvent::ResolveElicitation(elicitation_id) => {
      let events = self.session.resolve_elicitation(
          elicitation_id,
          angel_engine_client::ElicitationResponse::AllowForSession,
      )...
  ```

- `crates/luna/src/job.rs` — the one-off job runner **duplicates** this
  machinery: it has its own `default_permission_mode` helper and its own
  elicitation-resolution using `AllowForSession` (grep `default_permission_mode`
  and `AllowForSession` in `job.rs`). Any change here must land in both files.

- `crates/luna/src/init.rs:362-363` — `luna init` scaffolds (commented) example
  config containing `approval_policy: never` and `thread_sandbox:
  danger-full-access` strings — advertising the inert fields.

- `README.md:45` promises: "Choose a permission profile (`high_trust`,
  `workspace_write`, `read_only`) or fine-tune sandbox and approval policies
  directly." `README.CN.md` has the equivalent line. `crates/luna/AGENTS.md`
  has a WHERE-TO-LOOK row "Permission profiles | `src/config.rs`".

- The runtime client lives in the vendored submodule
  `angel-engine/crates/angel-engine-client/` (path dependency,
  `crates/luna/Cargo.toml:28`). **This plan was written without inspecting its
  internals** — Step 1 below is where you establish what the client actually
  supports. Do not modify anything inside `angel-engine/`.

- Convention: config structs use serde + garde with `deny_unknown_fields`;
  tests for config parsing live in `config.rs`'s `#[cfg(test)]` module — model
  new parse tests on `rejects_permission_profile_on_runner` (~`config.rs:1103`).

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Tests | `cargo test -p luna --locked` | exit 0 |
| Targeted tests | `cargo test -p luna config:: --locked` and `cargo test -p luna angel --locked` | exit 0 |
| Lint | `cargo clippy -p luna --all-targets --no-deps` | exit 0, no new warnings |
| Format | `cargo fmt --all` | exit 0 |

## Scope

**In scope** (the only files you should modify):
- `crates/luna/src/config.rs`
- `crates/luna/src/agent/angel_runtime.rs`
- `crates/luna/src/job.rs`
- `crates/luna/src/init.rs`
- `README.md`, `README.CN.md` (the permission-profile sentence only)
- `crates/luna/AGENTS.md` (the permission-profiles row only)

**Out of scope** (do NOT touch):
- `angel-engine/` — vendored submodule; read-only.
- `crates/luna/src/agent/acp.rs`, `agent/codex.rs`, `agent/command_line.rs` —
  other runners are not part of this wiring.
- `crates/luna/src/orchestrator.rs` — dispatch logic doesn't change.

## Git workflow

- Branch: `advisor/002-wire-agent-permission-config`
- Commit style: conventional commits, matching repo history. Suggested:
  `feat: wire agent permission config`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Establish what the angel-engine client supports (read-only)

Read (do not modify) the vendored client to find the exact names/types of:
1. `RuntimeOptionsOverrides` fields related to approval policy / sandbox
   (grep `pub struct RuntimeOptionsOverrides` in
   `angel-engine/crates/angel-engine-client/src/`).
2. `SendTextRequest.permission_mode`'s accepted values per runtime kind.
3. `ElicitationResponse` variants (what exists besides `AllowForSession` —
   e.g. `Allow`, `Deny`).

Record what you find in your report. If `RuntimeOptionsOverrides` has **no**
fields corresponding to `approval_policy`/`thread_sandbox`/`turn_sandbox_policy`,
that is a STOP condition (see below) — the fields cannot be honestly wired and
the operator must decide between removing them and extending the client.

**Verify**: you can quote the struct definitions in your report.

### Step 2: Add `permission_profile` and `permission_mode` to the runner configs

In `config.rs`:

1. Define an enum:

   ```rust
   #[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
   #[serde(rename_all = "snake_case")]
   pub enum PermissionProfile {
       HighTrust,
       WorkspaceWrite,
       ReadOnly,
   }
   ```

2. Add to `CodexRunner` and `OpencodeRunner`:
   `pub permission_profile: Option<PermissionProfile>` and
   `pub permission_mode: Option<String>` (garde: `inner(custom(not_blank))`
   for the string). Update the `Default` impls and every struct-literal
   construction site the compiler flags (including test fixtures such as
   `angel_runtime.rs:614-624`).

3. Add a resolution method on each runner (or a shared free function):

   ```rust
   /// Explicit permission_mode wins; otherwise the profile's mode;
   /// otherwise today's permissive default.
   pub fn resolved_permission_mode(&self) -> String
   ```

   Profile → mode mapping for Codex: `high_trust` → `"never"` (today's
   behavior), `workspace_write` and `read_only` → the corresponding values you
   found in Step 1 (Codex app-server permission modes; if the client defines
   constants, use those). For Opencode: `high_trust` → `"bypassPermissions"`;
   map the other two to the closest supported modes found in Step 1. Also add
   `resolved_elicitation_grants_all(&self) -> bool`: `true` only for
   `high_trust` **or** when neither profile nor mode is set (preserving today's
   default), `false` for `workspace_write`/`read_only`.

4. Update or replace the `rejects_permission_profile_on_runner` test: the field
   is now accepted; the test should assert that `permission_profile:
   high_trust` parses and resolves to the expected mode, and that an unknown
   profile value is rejected.

**Verify**: `cargo test -p luna config:: --locked` → exit 0.

### Step 3: Thread the resolved mode and elicitation policy through the runtime

In `angel_runtime.rs`:

1. `AngelRuntimeLaunchConfig::codex/opencode` set
   `default_permission_mode: config.resolved_permission_mode()` and a new
   `elicitation_grants_all: bool` field from
   `config.resolved_elicitation_grants_all()`.
2. Pass `elicitation_grants_all` into `AngelWorker`; in `handle_turn_event`'s
   `ResolveElicitation` arm, respond `AllowForSession` when `true`, otherwise
   the deny/decline variant found in Step 1 — and emit a `WorkerEvent` (use the
   existing `events` sender pattern) noting the denial so the operator can see
   why a turn was refused.
3. Wire `approval_policy` / `thread_sandbox` / `turn_sandbox_policy` from
   `CodexRunner` into the corresponding `RuntimeOptionsOverrides` fields found
   in Step 1 inside `launch_runtime`.

**Verify**: `cargo test -p luna angel --locked` → exit 0 (update the launch-
config tests to assert the new resolution, e.g.
`codex_launch_config_uses_codex_runtime_defaults`).

### Step 4: Apply the identical change to `job.rs`

Replace `job.rs`'s private `default_permission_mode` with the shared
`resolved_permission_mode()` from Step 2, and its `AllowForSession` elicitation
handling with the same `elicitation_grants_all` logic from Step 3. Do not leave
a second copy of the mapping behind — `job.rs` should call the config methods.

**Verify**: `cargo test -p luna job --locked` → exit 0;
`grep -n 'AllowForSession' crates/luna/src/job.rs` shows it only behind the
grants-all branch (or not at all if the helper is shared from `angel_runtime`).

### Step 5: Update scaffolding and docs to match reality

1. `init.rs` (~lines 355-370): update the scaffolded runner example to show
   `# permission_profile: workspace_write` alongside the existing commented
   sandbox fields, with a one-line comment that the default (no profile) is
   full-trust.
2. `README.md:45` / `README.CN.md` equivalent: keep the sentence, now true.
   Add one clause stating the default is high-trust and that `workspace_write`/
   `read_only` restrict the agent.
3. `crates/luna/AGENTS.md` "Permission profiles" row: point at
   `PermissionProfile` in `src/config.rs`.

**Verify**: `cargo test -p luna --locked` → exit 0 (init.rs has scaffold tests);
`grep -n 'permission_profile' README.md crates/luna/src/init.rs` → both hit.

## Test plan

New/updated tests (in the existing `#[cfg(test)]` modules of each file):

- `config.rs`: profile parses (`high_trust`, `workspace_write`, `read_only`);
  unknown profile string rejected; explicit `permission_mode` overrides
  profile; no-profile-no-mode resolves to today's defaults (`"never"` /
  `"bypassPermissions"`). Model after `rejects_permission_profile_on_runner`.
- `angel_runtime.rs`: launch-config test asserting `default_permission_mode`
  and `elicitation_grants_all` for each profile; a test asserting configured
  `thread_sandbox` reaches the `RuntimeOptionsOverrides` (this is the
  regression test for the original bug — the control silently not applying).
- `job.rs`: one test asserting the job runner resolves the same mode as the
  agent runner for the same config.

**Verification**: `cargo test -p luna --locked` → all pass including the new ones.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo test -p luna --locked` exits 0
- [ ] `cargo clippy -p luna --all-targets --no-deps` exits 0
- [ ] `grep -rn '"never".to_string()\|"bypassPermissions".to_string()' crates/luna/src/agent/angel_runtime.rs crates/luna/src/job.rs` returns no hardcoded-at-callsite matches (the literals may exist only inside the config resolution mapping)
- [ ] `grep -n 'approval_policy' crates/luna/src/agent/angel_runtime.rs` shows it passed into `RuntimeOptionsOverrides` (not only `None` in tests)
- [ ] A test exists asserting `thread_sandbox` config reaches launch options
- [ ] `git status` shows only in-scope files modified
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- Step 1 finds no `RuntimeOptionsOverrides` fields for approval/sandbox — the
  operator must choose: remove the inert config fields, or extend the vendored
  client (out of scope here).
- Step 1 finds no deny/decline `ElicitationResponse` variant — restricted
  profiles can't be honestly implemented; report the available variants.
- The permission-mode strings accepted per runtime kind can't be determined
  from the client source — guessing modes would ship a differently-broken
  control.
- Changing the `CodexRunner` struct breaks more than ~10 construction sites —
  reassess with the operator before a sweeping mechanical edit.

## Maintenance notes

- The **default remains fully permissive** — that's deliberate continuity with
  `07589e6`, not an oversight. A future decision to flip the default to
  `workspace_write` is a one-line change in `resolved_permission_mode` +
  README note; consider it once profiles have soaked.
- Reviewer: scrutinize the profile→mode mapping against the angel-engine
  client's accepted values (Step 1 quotes) — a typo'd mode string may be
  silently ignored by the runtime, recreating the original bug.
- `job.rs` duplicating the agent-session driver is a standing debt (audit
  finding DEBT-03); this plan de-duplicates only the permission logic. A fuller
  consolidation is deferred.
- Untrusted tracker content still flows into prompts unlabelled (audit
  SECURITY-05); profiles reduce blast radius but don't remove the injection
  channel. Deferred.
