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
- Choose a permission profile (`high_trust`, `workspace_write`, `read_only`) or fine-tune sandbox and approval policies directly
- Wire lifecycle hooks to run shell commands after workspace creation, before/after each run, and on cleanup

**Fail gracefully**
- Exponential backoff retry on agent failure or timeout
- Stall detection kills unresponsive agents and reschedules
- On startup, stale workspaces from previous sessions are cleaned up automatically

## Who It's For

Luna is for developers who:
- Manage their work in GitHub Projects
- Are already using or exploring autonomous coding agents
- Want a production-grade daemon, not a weekend script
- Would rather write a clear ticket than babysit a code generation session

## Getting Started

```bash
# Initialize a WORKFLOW.md in the current directory
luna init

# Post a tracker comment from the current issue workspace
luna comment "Started implementation, validating tests next."

# Run the orchestrator
luna
```

Luna requires [Codex](https://github.com/openai/codex) and the [GitHub CLI](https://cli.github.com/) (`gh`) to be installed and authenticated.

## The Vision

Software backlogs exist because there aren't enough hours. Luna is the first step toward a future where the backlog is a live queue — items flow in from issues and requirements, agents flow in from available compute, and working software flows out the other end. The human stays in the loop at the level that matters: deciding what to build, reviewing what was built.

---

*Built in Rust. macOS support is stable; Linux support is in progress.*
