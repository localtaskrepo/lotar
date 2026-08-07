# Copilot instructions (LoTaR)

This file is a **thin Copilot overlay** — it intentionally contains no policy or
command lists of its own. Everything canonical lives in the shared files below and is
loaded the same way by Kilo, so there is nothing to keep in sync here.

## Canonical sources (read these, don't duplicate them)

- **`AGENTS.md`** (repo root) — the canonical policy: workflow, git rules, quality gates,
  contract sync, storage discipline, safety. Copilot reads it automatically (repo-wide +
  agent). Defer to it on everything.
- **`.github/instructions/*.instructions.md`** — per-area structure & conventions.
  Copilot applies these automatically per file via `applyTo`:
  - `backend.instructions.md` → `**/*.rs`
  - `frontend.instructions.md` → `view/**`
  - `smoke.instructions.md` → `smoke/**`

## Workflow runbooks (skills)

These on-demand runbooks live in `.github/skills/<name>/SKILL.md`. Open the relevant one
when the task matches (Kilo loads them via its `skill` tool; in Copilot, reference the path):

- `development-workflow` — task lifecycle, worktree/rebase rules, "staged = reviewed".
- `review-handoff` — definition of done, handoff summary, browser/vision UI verification.
- `testing-strategy` — run/target tests (nextest, vitest, smoke) + low-noise variants.
- `smoke-suite-debugging` — smoke failures (binary, ports, Playwright, server lifecycle).
- `local-dev-serve-troubleshooting` — Vite dev server, `lotar serve`, ports, SSE.
- `api-contract-change-end-to-end` — keep `src/api_types.rs` / `view/api/types.ts` / `docs/openapi.json` in sync.
- `lotar-dev-tracking` — track work in the committed `.tasks/` backlog (dogfooding).
- `skill-discovery` — maps "what you're editing" → which runbook to load.

For a quick command reference, see the `testing-strategy` skill rather than duplicating it here.
