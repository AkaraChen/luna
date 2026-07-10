# Luna

> This project is under heavy construction.

**Your GitHub Project backlog, worked autonomously.**

[中文版本](./README.CN.md)

---

## Why Luna?

You have a backlog. Issues sit in "Todo" for weeks — not because they're hard, but because there are only so many hours in a day. You want help, but you don't want to babysit a script or hand-hold an AI through every step.

**Luna is built on a simple idea: a coding agent should work like a good async teammate — pick up a ticket, do the work, open the PR, and move on to the next one, without being asked.**

## The Philosophy

### Your backlog is already the source of truth
Luna integrates directly with GitHub Projects. No new database, no migration, no "import your tasks here." The Status field you already use drives everything. Move a card to "In Progress" and Luna notices. Move it to "Done" and Luna stops.

### Autonomous means continuous
Luna runs as a long-running daemon, not a one-shot command. It polls your project on a configurable interval, dispatches agents to fresh workspaces, watches for stalls, retries on failure, and reconciles running work against the real state of your board — all without you watching it.

### One file to rule the workflow
Your entire automation lives in a single `WORKFLOW.md`. YAML frontmatter configures the infrastructure — concurrency limits, retry policy, permission profiles, lifecycle hooks. The body is a Jinja2 template that becomes the agent's prompt. Change the file and Luna picks it up without a restart.

### Isolation over cleverness
Each issue gets its own workspace — a fresh directory the agent works in from scratch. No shared state, no cross-contamination between concurrent tasks. When work is done, the workspace is cleaned up.

## What You Can Do

**Set it and forget it**
- Run `luna` against a WORKFLOW.md and walk away
- Agents pick up Todo and In-Progress items automatically, respecting priority order
- Completed items are skipped; canceled items stop their agents immediately

**Run N agents at once**
- Configure global and per-state concurrency limits
- Agents spin up in parallel, each isolated in their own workspace
- Rate limits and stall detection prevent runaway costs

**Control how agents behave**
- Write the agent prompt directly in WORKFLOW.md — inject issue title, description, URL, priority, and blocked-by relationships with template variables
- Choose a permission profile (`high_trust`, `workspace_write`, `read_only`) or set the runner permission mode directly. The default is high-trust.
- Wire lifecycle hooks to run shell commands after workspace creation, before/after each run, and on cleanup

**Fail gracefully**
- Exponential backoff retry on agent failure or timeout
- Stall detection kills unresponsive agents and reschedules
- On startup, stale workspaces from previous sessions are cleaned up automatically

## What's In The Box

**Trackers**
- GitHub Projects: poll a Project board through `gh`.
- Asahi: embedded local tracker with issues, projects, wiki, and notifications; Luna can auto-start it when `WORKFLOW.md` omits an explicit tracker.
- Linear: tracker backend for Linear issue queues.

**Asahi surface**
- Rocket REST API backed by SQLite and SeaORM
- React dashboard for issues, projects, wiki pages, and notifications
- Asahi Desktop, a Tauri shell that bundles a `luna` sidecar

**Runners**
- Codex: the default runner, driven through the vendored `angel-engine` client
- opencode
- ACP-compatible agents

**CLI**

| Command | Description |
|---------|-------------|
| `luna init` | Initialize a default `WORKFLOW.md` in a directory |
| `luna comment` | Post a comment to the current tracker item |
| `luna show` | Show the current tracker item |
| `luna move` | Move the current tracker item to a new state |
| `luna wiki` | Browse the current issue's project wiki via a virtual shell |
| `luna job` | Run a one-off Angel Engine job and stream TurnRunEvent JSONL |
| `luna asahi-desktop` | Start the Asahi desktop backend without running agent jobs |

## Who It's For

Luna is for developers who:
- Manage their work in GitHub Projects
- Are already using or exploring autonomous coding agents
- Want a production-grade daemon, not a weekend script
- Would rather write a clear ticket than babysit a code generation session

## Getting Started

**Prerequisites**

- Rust toolchain with edition 2024 support
- Bun 1.3.13
- `just` for the repo recipes
- Codex CLI for the default runner
- GitHub CLI (`gh`) for GitHub Project workflows
- Initialized submodules: `git submodule update --init --recursive`

`crates/luna` path-depends on `angel-engine/crates/angel-engine-client`, so a fresh clone cannot build Luna until submodules are initialized.

**Install**

```bash
just install
# or:
cargo install --path ./crates/luna --force --locked
```

**Secrets**

```bash
cp .env.luna.example .env.luna
```

Place `.env.luna` next to the `WORKFLOW.md` you run; Luna loads it automatically.

**Dashboard dev**

```bash
just asahi-backend   # Rocket + SQLite on 127.0.0.1:49306
just asahi-frontend  # Vite+ dashboard pointed at the same port
```

**CLI**

```bash
# Initialize a WORKFLOW.md in the current directory
luna init

# Post a tracker comment from the current issue workspace
luna comment "Started implementation, validating tests next."

# Run the orchestrator
luna
```

Luna uses the vendored `angel-engine` Rust client to drive the configured agent runtime. The default runtime command is `codex app-server`.

## The Vision

Software backlogs exist because there aren't enough hours. Luna is the first step toward a future where the backlog is a live queue — items flow in from issues and requirements, agents flow in from available compute, and working software flows out the other end. The human stays in the loop at the level that matters: deciding what to build, reviewing what was built.

---

*Built in Rust. macOS support is stable; Linux support is in progress.*
