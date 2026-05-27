# Asahi Migration Crate — Agent Knowledge

**Package:** `asahi-migration` (SeaORM migrations)
**Parent:** [../../AGENTS.md](../../AGENTS.md)

## OVERVIEW

Ordered SeaORM migrations for the Asahi SQLite schema. Consumed by the `asahi` crate at startup — not a standalone binary.

## STRUCTURE

```
src/
├── lib.rs                              # Migrator trait impl, migration registry
├── m20260430_000001_create_asahi_schema.rs
├── m20260430_000002_create_projects.rs
├── m20260501_000003_backfill_projects.rs
├── m20260501_000004_create_project_wiki.rs
└── m20260501_000005_remove_synthetic_default_project.rs
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add new migration | New `m{YYYYMMDD}_{seq}_{name}.rs` | Register in `lib.rs` `migrations()` vec |
| Schema for entity X | Match entity in `crates/asahi/src/entity/` | Keep column names in sync |
| Wiki tables | `m20260501_000004_create_project_wiki.rs` | wiki_node, wiki_page_version, wiki_audit |

## CONVENTIONS

- Filename pattern: `m{YYYYMMDD}_{NNNNNN}_{snake_case_description}.rs`.
- Append new migrations to the end of the vec in `lib.rs` — never reorder existing ones.
- Use SeaORM migration DSL (`Table::create`, `ColumnDef`, etc.) — no raw SQL unless necessary.
- Test migrations against fresh DB and against existing `asahi-dev.db` state.

## ANTI-PATTERNS

- Do not edit already-shipped migrations — add a new forward migration instead.
- Do not remove migrations from the vec (breaks existing databases).
- Do not add entity fields in `asahi` without a migration here.

## COMMANDS

```bash
cargo test -p asahi-migration
cargo test -p asahi    # migrations run as part of asahi startup
```
