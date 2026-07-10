# Luna Crate — Agent Knowledge

**Package:** `luna` (binary + library)
**Parent:** [../../AGENTS.md](../../AGENTS.md)

## OVERVIEW

Long-running coding-agent orchestrator. Parses `WORKFLOW.md`, polls a tracker, manages isolated workspaces, dispatches agents, handles retries/stalls/concurrency.

## STRUCTURE

```
src/
├── main.rs           # CLI: init, comment, show, move, wiki, default → orchestrator
├── orchestrator.rs   # Daemon loop (poll, dispatch, reconcile, shutdown)
├── workflow.rs       # WORKFLOW.md load + hot reload
├── config.rs         # ServiceConfig from YAML frontmatter
├── agent/            # Codex + ACP agent runners
├── tracker/          # github_project, asahi, linear backends + CLI commands
├── workspace.rs      # Per-issue directory lifecycle
├── wiki/             # Virtual shell over project wiki filesystem
├── prompt.rs         # Jinja2 template rendering
├── model.rs          # Issue, WorkflowDefinition, TokenTotals
└── init.rs           # `luna init` scaffolding
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add tracker backend | `src/tracker/mod.rs` + new module | Implement `Tracker` trait |
| Change poll/dispatch logic | `src/orchestrator.rs` | Also embeds Asahi Rocket when needed |
| Permission profiles | `src/config.rs` (`PermissionProfile`) | `high_trust`, `workspace_write`, `read_only` |
| Agent lifecycle hooks | `src/config.rs` + `src/shell_command.rs` | Shell commands at workspace events |
| Stall detection | `src/orchestrator.rs` | Kills unresponsive agents, reschedules |
| Wiki virtual FS | `src/wiki/fs.rs`, `src/wiki/shell.rs` | Used by `luna wiki` command |

## CONVENTIONS

- Errors via `LunaError` in `error.rs`; propagate with `Result<T>`.
- Tracker resolution: explicit `--issue` flag → `LUNA_ISSUE_*` env → workspace context.
- `.env.luna` loaded from workflow directory (see `main.rs::load_dotenv_file`).
- Depends on `asahi` crate for embedded tracker server — not a circular dep at runtime (orchestrator spawns Rocket).

## ANTI-PATTERNS

- Do not bypass `WorkspaceManager` for agent working directories.
- Do not hardcode tracker state names — they come from WORKFLOW.md config.
- Do not block the orchestrator loop on agent I/O — use tokio tasks + channels.

## COMMANDS

```bash
cargo run -p luna                          # run orchestrator (needs WORKFLOW.md)
cargo run -p luna -- init                  # scaffold workflow
cargo test -p luna
cargo install --path . --force --locked    # install `luna` binary
```
