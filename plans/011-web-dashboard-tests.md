# Plan 011: Give the Asahi web dashboard a test suite (starting with the sanitizer)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat 74ad45b..HEAD -- apps/asahi-web/ .github/workflows/ci.yml justfile`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: plans/001-ci-test-baseline.md (extends its CI workflow and `just test` recipe)
- **Category**: tests
- **Planned at**: commit `74ad45b`, 2026-07-03

## Why this matters

The dashboard is the highest-churn code in the repo (the top six files by
recent commit count are all `components/dashboard/*.tsx`) and has **zero**
tests — no test script, no test files — despite the toolchain (`vp test`, a
Vitest-compatible runner) already being vendored via `vite-plus`. The single
most safety-critical unit is `src/lib/sanitize.ts`: every
`dangerouslySetInnerHTML` in the app routes through it, and plan 006 bumps the
underlying DOMPurify — without tests, a sanitizer regression ships silently.

## Current state

- `apps/asahi-web/package.json` — scripts are only:

  ```json
  "scripts": { "dev": "vp dev", "build": "tsc && vp build", "preview": "vp preview" }
  ```

  `overrides` maps `vitest` → `npm:@voidzero-dev/vite-plus-test@latest`
  (pinned to an explicit version if plan 006 ran first). No `*.test.*` files
  exist under `src/` (verified). Per `apps/asahi-web/AGENTS.md`, the intended
  command set is `vp install && vp check && vp test` — `vp test` is the
  runner; local docs live at `node_modules/vite-plus/docs`.

- `src/lib/sanitize.ts` — the unit under test:

  ```ts
  import DOMPurify from "dompurify";
  /**
   * Sanitize HTML coming from the server before it lands in a
   * dangerouslySetInnerHTML call. Allowlist matches the Tiptap StarterKit
   * surface: structural prose elements only, no scripts, no inline event
   * handlers, no data-URIs in attributes.
   */
  const ALLOWED_TAGS = [ "p", "br", "strong", "em", "u", "s", "code", "pre", ... ];
  export function sanitizeRichText(html: string | null | undefined): string { ... }
  ```

  (Read the whole file before writing tests — the allowlist continues beyond
  the excerpt and there are more exports/constants; test what's there.)

- Component landscape (for the smoke tests): views under
  `src/components/dashboard/` (`issue-list.tsx`, `notifications-view.tsx`,
  `issue-details.tsx`, `project-details.tsx`, `project-wiki.tsx`,
  `issue-composer.tsx`); data flows through TanStack Query
  (`@tanstack/react-query`) with fetch wrappers in `src/api/asahi.ts`; routing
  via wouter; path alias `@/` → `src/` (see `tsconfig.json`). React 19.
- CI: `.github/workflows/ci.yml` `asahi-web` job runs `vp check` only. Plan
  001 added a `just test` recipe (Rust-only) — this plan extends both.
- Design doc constraint (irrelevant to tests but stated to prevent scope
  creep): do not "fix" any UI while here; `DESIGN.md` governs visual changes.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Install | `bun install` (repo root) | exit 0 |
| Tests | `bun run --cwd apps/asahi-web test` (after Step 1) | exit 0, all pass |
| Check | `bun run --cwd apps/asahi-web vp check src AGENTS.md DESIGN.md` | exit 0 |
| Runner docs | `ls apps/asahi-web/node_modules/vite-plus/docs` | files to consult for test config |

## Scope

**In scope**:
- `apps/asahi-web/package.json` — `test` script + test-only devDependencies
- `apps/asahi-web/vite.config.ts` (or wherever vp reads test config — consult
  the vite-plus docs; possibly a `test` field in the existing config)
- `apps/asahi-web/src/**/*.test.ts(x)` (create)
- `apps/asahi-web/src/test/` setup file if needed (create)
- `.github/workflows/ci.yml` — add `vp test` to the asahi-web job
- `justfile` — extend the `test` recipe from plan 001

**Out of scope**:
- Any production `.ts`/`.tsx` change. Exception: adding a `data-testid` or
  exporting an existing private pure function for testability is allowed;
  anything else (including bugfixes found by tests) is report-only.
- `DESIGN.md`, styles, UI behavior.
- E2E/browser tests — unit + component only.

## Git workflow

- Branch: `advisor/011-web-dashboard-tests`
- Commit style: conventional commits, matching repo history. Suggested:
  `test: add asahi web dashboard tests`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Wire the test runner

1. Add `"test": "vp test"` to `apps/asahi-web/package.json` scripts.
2. Consult `node_modules/vite-plus/docs` for how `vp test` picks up config;
   configure a jsdom (or vite-plus's DOM equivalent) environment for
   `*.test.tsx` files. Add the minimum devDependencies it prescribes —
   expected: `jsdom` (or built-in), `@testing-library/react`,
   `@testing-library/jest-dom` (or vite-plus equivalents; follow ITS docs, not
   generic Vitest habit).
3. Create a trivial `src/lib/smoke.test.ts` (`expect(1+1).toBe(2)`) to prove
   the pipeline, then delete it once real tests exist (or keep the first real
   test as the prover).

**Verify**: `bun run --cwd apps/asahi-web test` → runs, exit 0.

### Step 2: Sanitizer tests — `src/lib/sanitize.test.ts`

Read `src/lib/sanitize.ts` fully, then cover `sanitizeRichText`:

1. null / undefined / empty string → `""` (or documented current behavior).
2. Each allowlisted tag survives (loop over the exported/visible ALLOWED_TAGS
   if exported; otherwise a representative set incl. `p`, `pre`, `code`,
   lists, headings if present).
3. `<script>` is stripped; `onerror`/`onclick` attributes stripped;
   `javascript:` hrefs neutralized; `data:` URI per the file's stated policy;
   `<img>`/`<iframe>`/`<style>` (not in allowlist) removed.
4. Nested/malformed markup doesn't throw and returns sanitized output.
5. Links: whatever attribute policy exists for `href`/`target`/`rel` — encode
   current behavior (note: the audit observed `target` allowed without forced
   `rel="noopener"`; if you confirm, encode it with a `// NOTE:` comment
   rather than changing the allowlist).

These run in jsdom (DOMPurify needs a DOM).

**Verify**: `bun run --cwd apps/asahi-web test` → sanitize tests pass.

### Step 3: Component smoke tests for the two highest-churn views

Using Testing Library, with a fresh `QueryClient` +
`QueryClientProvider` wrapper and `fetch` mocked (stub `global.fetch` or mock
the wrappers in `src/api/asahi.ts` via the runner's module-mock facility —
read `asahi.ts` first and pick the seam that needs zero production changes):

1. `issue-list.test.tsx` — renders a provided list of issues (2-3 fixture
   issues with identifier/title/state/priority); asserts identifiers and
   titles appear; empty list renders its empty state.
2. `notifications-view.test.tsx` — renders fixture notifications; asserts
   unread indicators/labels appear; a read/archive interaction fires the
   expected API call (assert on the mocked fetch).

Keep fixtures minimal and local to each test file. If a component proves
untestable without refactoring (deep context requirements), swap it for the
next view on the churn list (`issue-details`, `project-details`) and say so —
two view tests are the bar, not those two specifically.

**Verify**: `bun run --cwd apps/asahi-web test` → all pass.

### Step 4: Wire into CI and `just test`

1. `.github/workflows/ci.yml` `asahi-web` job: add a step
   `run: bun run --cwd apps/asahi-web test` after the `vp check` step.
2. `justfile`: extend the `test` recipe (from plan 001) to also run
   `bun run --cwd apps/asahi-web test`.

**Verify**: `just test` → Rust suites + web tests all run, exit 0; YAML parses.

## Test plan

This plan IS the test plan: ≥1 sanitize suite (≥10 cases) + 2 component smoke
suites. No existing tests to model on in this package — the structural
reference is the vite-plus docs' own testing example plus standard Testing
Library patterns.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `bun run --cwd apps/asahi-web test` exits 0
- [ ] `find apps/asahi-web/src -name '*.test.*' | wc -l` ≥ 3
- [ ] Sanitize suite covers script-strip, event-handler-strip, and null input (grep the test file)
- [ ] `bun run --cwd apps/asahi-web vp check src AGENTS.md DESIGN.md` exits 0
- [ ] `.github/workflows/ci.yml` asahi-web job runs the web tests; `just test` runs both toolchains
- [ ] `git diff --stat` shows no production `src/**` changes beyond the permitted testability exceptions (list any in the report)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- `vp test` cannot be configured for a DOM environment from its own docs
  within ~30 minutes of reading — report what the docs actually support; do
  NOT bolt vanilla Vitest alongside vp (two runners is a decision for the
  operator).
- A sanitize test reveals the sanitizer passing something the file's comment
  says it blocks (a real vulnerability) — write the test to pin the *intended*
  behavior, mark `.skip`/`.todo` (report-only; fixing the allowlist is plan
  006 territory or a new finding).
- Component testing requires modifying production components beyond the
  stated exceptions.

## Maintenance notes

- The churn leaders (`project-wiki.tsx`, `issue-details.tsx`,
  `project-details.tsx`) remain untested after this plan — the harness now
  exists; extending coverage is incremental follow-up.
- Plan 006's dompurify bump and this plan are mutually reinforcing — whichever
  lands second gets a free regression check (`vp test` in 006's gates).
- Reviewer: check the fetch-mocking seam — if tests mock `src/api/asahi.ts`
  wholesale, they won't catch contract drift with the Rocket API; that's
  accepted for smoke tests, but don't let anyone call them integration tests.
