# Asahi Desktop — Agent Knowledge

**Package:** `asahi-desktop` (Tauri shell)
**Parent:** [../../AGENTS.md](../../AGENTS.md)

## OVERVIEW

Tauri desktop app for Asahi. It prepares a `luna` sidecar binary, starts `luna asahi-desktop --port 49306 --db <app-data>/asahi.db`, then points the main webview at the local dashboard when the backend is ready.

## STRUCTURE

```
apps/asahi-desktop/
├── src-tauri/              # Tauri Rust shell, config, icons, capabilities
├── ui/                     # Static loading page before local Asahi is ready
├── scripts/prepare-sidecar.mjs
├── package.json            # Bun scripts for dev/build
└── icon.svg
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Sidecar startup and shutdown | `src-tauri/src/main.rs` | Spawns `luna asahi-desktop`; kills it on window close |
| Bundle configuration | `src-tauri/tauri.conf.json` | External bin is `binaries/luna`; frontendDist is `../ui` |
| Build sidecar binary | `scripts/prepare-sidecar.mjs` | Runs `cargo build -p luna`, copies target binary into `src-tauri/binaries/` |
| Desktop release | `.github/workflows/asahi-desktop-release.yml` | Triggered by `asahi-v*` or `v*asahi*` tags |

## CONVENTIONS

- The desktop backend listens on `127.0.0.1:49306`.
- The SQLite DB lives under Tauri app data as `asahi.db`.
- Generated sidecar binaries live in `src-tauri/binaries/` and are gitignored.
- Release builds call `prepare:sidecar -- --release --target <target>` before `tauri build`.

## ANTI-PATTERNS

- Do not hardcode another dashboard port without updating the sidecar args and readiness URL together.
- Do not commit generated binaries under `src-tauri/binaries/`.
- Do not bypass `prepare:sidecar`; Tauri expects the target-suffixed sidecar filename.

## COMMANDS

```bash
bun run --cwd apps/asahi-desktop dev
bun run --cwd apps/asahi-desktop build
bun run --cwd apps/asahi-desktop prepare:sidecar
```
