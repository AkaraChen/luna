# PROJECT KNOWLEDGE BASE

**Generated:** 2026-05-27
**Commit:** 2bd65af
**Branch:** master

## OVERVIEW

Luna is a Rust daemon that polls GitHub Projects (or the embedded Asahi tracker), dispatches coding agents into isolated workspaces, and reconciles board state. Asahi is the companion Rocket/SeaORM API + React dashboard for local issue tracking and wiki.

## STRUCTURE

```
luna/
├── crates/
│   ├── luna/              # CLI + orchestrator daemon
│   ├── asahi/             # Rocket REST API (issues, projects, wiki)
│   └── asahi-migration/   # SeaORM migrations for Asahi SQLite schema
├── apps/asahi-web/        # React dashboard (Vite+, shadcn, TanStack Query)
├── justfile               # Dev shortcuts (asahi-frontend, asahi-backend, install)
├── .agents/skills/        # Agent workflow skills (symlinked from .claude/skills/)
└── akrc-docs/             # Git submodule (external docs)
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Orchestrator loop, concurrency, retries | `crates/luna/src/orchestrator.rs` | Embeds Asahi when tracker unset |
| WORKFLOW.md parsing + hot reload | `crates/luna/src/workflow.rs` | YAML frontmatter + Jinja2 body |
| GitHub Project tracker | `crates/luna/src/tracker/github_project.rs` | Requires `gh` CLI |
| Asahi tracker client | `crates/luna/src/tracker/asahi.rs` | HTTP to embedded/local Asahi |
| Agent dispatch (Codex/ACP) | `crates/luna/src/agent/` | `run_agent_attempt`, permission profiles |
| Issue/project REST API | `crates/asahi/src/api/` | Rocket handlers |
| Domain logic | `crates/asahi/src/domain/` | Pure types + validation (garde) |
| DB entities | `crates/asahi/src/entity/` | SeaORM models |
| Schema migrations | `crates/asahi-migration/src/m*.rs` | Ordered SeaORM migrations |
| Dashboard UI | `apps/asahi-web/src/App.tsx` | wouter routes, React Query |
| Visual system | `apps/asahi-web/DESIGN.md` | "Studio Notebook" — read before UI work |

## CODE MAP

| Symbol | Type | Location | Role |
|--------|------|----------|------|
| `orchestrator::run` | fn | `crates/luna/src/orchestrator.rs` | Main daemon entry |
| `WorkflowStore` | struct | `crates/luna/src/workflow.rs` | Hot-reloads WORKFLOW.md |
| `build_tracker` | fn | `crates/luna/src/tracker/mod.rs` | github_project \| asahi \| linear |
| `run_agent_attempt` | fn | `crates/luna/src/agent/mod.rs` | Spawns agent in workspace |
| `WorkspaceManager` | struct | `crates/luna/src/workspace.rs` | Per-issue isolated dirs |
| `rocket_with_database_url_and_port` | fn | `crates/asahi/src/app.rs` | Embeddable Asahi server |
| `App` | component | `apps/asahi-web/src/App.tsx` | Dashboard shell + routing |

## CONVENTIONS

- Rust 2024 edition across all crates; workspace resolver `"3"`.
- Luna config lives in `WORKFLOW.md` (YAML frontmatter + Jinja2 prompt body). `.env.luna` beside it for secrets.
- Asahi uses SQLite via SeaORM; dev DB path is `asahi-dev.db` at repo root (see `justfile`).
- Bun workspaces: root `package.json` declares `"workspaces": ["apps/*"]`.
- Agent skills live under `.agents/skills/`; `.claude/skills/` symlinks into them.
- `.reference/` holds vendored Tiptap source for local reference — not part of the build.

## ANTI-PATTERNS (THIS PROJECT)

- Do not add a separate task database — GitHub Projects / Asahi tracker is source of truth.
- Do not share workspace state between concurrent agent runs.
- Do not use bold typography or display-scale headings in Asahi UI (see `DESIGN.md`).
- Do not edit `.reference/` — it is read-only reference material.

## UNIQUE STYLES

- Luna auto-starts embedded Asahi when `WORKFLOW.md` has no explicit `tracker` key.
- Asahi web rejects Linear-clone, SaaS-cream, and dark-dashboard aesthetics by design.
- Issue keys render in JetBrains Mono; everything else is Inter Variable 400/500.

## COMMANDS

```bash
# Luna CLI
cargo install --path ./crates/luna --force --locked   # or: just install
luna init                                              # scaffold WORKFLOW.md
luna                                                   # run orchestrator

# Asahi full-stack dev (port 49306, repo-local SQLite)
just asahi-frontend                                    # Vite+ dev server
just asahi-backend                                     # cargo-watch + Rocket

# Frontend (from apps/asahi-web)
vp install && vp check && vp test                      # Vite+ toolchain
bun run dev                                            # via package.json → vp dev
```

## NOTES

- Requires Codex and `gh` for GitHub Project workflows.
- macOS stable; Linux in progress (per README).
- `ASAHI_SKIP_WEB_BUILD=1` skips frontend embed during backend dev.
- Submodule `akrc-docs/` may be empty until `git submodule update --init`.
