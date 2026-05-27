# Asahi Crate — Agent Knowledge

**Package:** `asahi` (Rocket API server)
**Parent:** [../../AGENTS.md](../../AGENTS.md)

## OVERVIEW

Local issue/project tracker REST API backed by SQLite + SeaORM. Serves the Asahi web dashboard and acts as Luna's embedded tracker when no GitHub Project is configured.

## STRUCTURE

```
src/
├── main.rs       # Standalone: rocket().launch()
├── app.rs        # Rocket config, DB connect, route mounting
├── lib.rs        # pub mod re-exports + rocket_with_database_url_and_port
├── api/          # HTTP handlers (issues, projects, wiki, notifications, health)
├── domain/       # Business types + garde validation
├── entity/       # SeaORM entity models
├── service/      # DB query/update layer
├── db.rs         # Connection setup
└── web.rs        # Static frontend embed (production builds)
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add REST endpoint | `src/api/mod.rs` + handler module | Follow existing error mapping in `api/error.rs` |
| Change issue schema | `entity/issue.rs` + migration in `asahi-migration` | Always add migration first |
| Project wiki | `api/wiki.rs`, `domain/wiki.rs`, `entity/wiki_*` | Versioned pages + audit trail |
| Notifications | `api/notifications.rs`, `domain/notification.rs` | Inbox feed for dashboard |
| Embeddable server | `app.rs::rocket_with_database_url_and_port` | Used by Luna orchestrator |

## CONVENTIONS

- Rocket 0.5 with JSON support; handlers return `Result<T, ApiError>`.
- Domain layer uses `garde` for validation — keep HTTP handlers thin.
- SeaORM entities in `entity/`, query logic in `service/`.
- Env vars: `ASAHI_DATABASE_URL`, `ASAHI_PORT`, `ROCKET_ADDRESS`, `ROCKET_PORT`.
- Dev: `ASAHI_SKIP_WEB_BUILD=1` skips embedding frontend assets.

## ANTI-PATTERNS

- Do not put business logic in API handlers — delegate to `service/` + `domain/`.
- Do not alter schema without a corresponding migration in `asahi-migration`.
- Do not assume a default project exists (migration `m20260501_000005` removed synthetic default).

## COMMANDS

```bash
# Dev with auto-reload (via justfile)
just asahi-backend

# Direct
ASAHI_DATABASE_URL='sqlite://asahi-dev.db?mode=rwc' cargo run -p asahi
cargo test -p asahi
```
