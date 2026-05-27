<!--VITE PLUS START-->

# Using Vite+, the Unified Toolchain for the Web

This project is using Vite+, a unified toolchain built on top of Vite, Rolldown, Vitest, tsdown, Oxlint, Oxfmt, and Vite Task. Vite+ wraps runtime management, package management, and frontend tooling in a single global CLI called `vp`. Vite+ is distinct from Vite, and it invokes Vite through `vp dev` and `vp build`. Run `vp help` to print a list of commands and `vp <command> --help` for information about a specific command.

Docs are local at `node_modules/vite-plus/docs` or online at https://viteplus.dev/guide/.

## Review Checklist

- [ ] Run `vp install` after pulling remote changes and before getting started.
- [ ] Run `vp check` and `vp test` to format, lint, type check and test changes.
- [ ] Check if there are `vite.config.ts` tasks or `package.json` scripts necessary for validation, run via `vp run <script>`.

<!--VITE PLUS END-->

# Asahi Web — Agent Knowledge

**Package:** `asahi-web` (React dashboard)
**Parent:** [../../AGENTS.md](../../AGENTS.md)

## OVERVIEW

React 19 dashboard for the Asahi tracker API. Routes via wouter, data via TanStack Query, UI via shadcn/Radix. Visual system is "Studio Notebook" — see `DESIGN.md` before any UI change.

## STRUCTURE

```
src/
├── main.tsx                          # Entry
├── App.tsx                           # Dashboard shell, wouter routes
├── api/asahi.ts                      # Fetch wrappers for Rocket API
├── components/
│   ├── dashboard/                    # Feature views (issues, projects, inbox, wiki)
│   └── ui/                           # shadcn primitives (button, sheet, tabs, …)
├── lib/                              # utils, sanitize, query-refresh
└── style.css                         # Design tokens + semantic colour rules
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Routing | `App.tsx` | `/issues`, `/issues/:id`, `/projects/:locator`, `/inbox` |
| Issue list + filters | `App.tsx` IssuesView, `components/dashboard/issue-list.tsx` | keepPreviousData avoids skeleton flash |
| Issue detail + comments | `components/dashboard/issue-details.tsx` | Tiptap editor for descriptions |
| Project wiki | `components/dashboard/project-wiki.tsx` | Renders wiki tree from API |
| API client | `api/asahi.ts` | Base URL from env / vite proxy |
| Design tokens | `style.css`, `DESIGN.md` | Semantic-only colour rule |

## CONVENTIONS

- Path alias `@/` → `src/` (see `tsconfig.json`).
- shadcn components in `components/ui/` — extend via `components.json`.
- Status colours are semantic (Done=green, In Progress=amber) — no decorative tinting.
- Typography: Inter Variable 400/500 only; JetBrains Mono for issue keys only.
- No bold, no display-scale headings, flat elevation except `.asahi-pill-lift`.

## ANTI-PATTERNS

- Do not introduce Linear-clone filter rails, SaaS-cream warmth, or dark-dashboard defaults.
- Do not add shadows to new components — redesign if elevation seems needed.
- Do not use `useSuspenseQuery` for filter-driven lists (causes skeleton flash — see IssuesView comment).

## COMMANDS

```bash
vp install && vp check && vp test     # format, lint, typecheck, test
bun run dev                           # dev server (proxies to Asahi backend)
just asahi-frontend                   # from repo root, port 49306
bun run build                         # tsc && vp build
```
