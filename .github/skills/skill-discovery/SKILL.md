---
name: skill-discovery
description: Use this at the start of non-trivial work to load the right runbooks (skills + path-scoped instructions) before making changes.
---

## Goal

Avoid "missing context" by making skill discovery a deliberate first step. The canonical policy is **AGENTS.md**; skills only add extra context for specific situations.

## Delivery (how this reaches you)

- **Kilo:** skills live in `.github/skills/` and are discoverable via the `skill` tool (wired in repo-root `kilo.jsonc`). Load the 1–3 most relevant on demand. Path-scoped rules (below) are **not** auto-applied in Kilo — read the matching `.github/instructions/*.instructions.md` file when you work in that area.
- **Copilot:** the `.github/instructions/*.instructions.md` files apply automatically per file via their `applyTo` frontmatter. Copilot does **not** auto-load `.github/skills/`; workflow runbooks are referenced from `.github/copilot-instructions.md` (open the relevant one when the task matches).

## Procedure

1. **Identify the domain(s)** you'll touch:
   - Rust backend: CLI (`src/cli/`), server/API (`src/api_server.rs`, `src/web_server.rs`, `src/routes.rs`), storage (`src/storage/`), config, scanner.
   - Frontend: Vue UI under `view/`.
   - Smoke: end-to-end harness under `smoke/`.
   - Docs/contracts: `docs/openapi.json`, `docs/help/*`.

2. **Load path-scoped rules** (Copilot: automatic; Kilo: read manually):
   - Rust → `.github/instructions/backend.instructions.md`
   - UI → `.github/instructions/frontend.instructions.md`
   - Smoke → `.github/instructions/smoke.instructions.md`

3. **Load the 1–3 most relevant skills** (keyword → skill):
   - "task lifecycle", "worktree", "rebase", "merge", "staged" → `development-workflow`
   - "handoff", "review", "definition of done", "verify UI", "screenshot", "ui diff" → `review-handoff`
   - "nextest", "vitest", "test failing", "run one test", "no ANSI" → `testing-strategy`
   - "smoke", "Playwright", "E2E", "serve lifecycle" → `smoke-suite-debugging`
   - "endpoint", "REST", "DTO", "OpenAPI", "schema" → `api-contract-change-end-to-end`
   - "local dev", "Vite", "lotar serve", "ports", "SSE" → `local-dev-serve-troubleshooting`
   - "track work", ".tasks", "LoTaR task", "plan doc" → `lotar-dev-tracking`

4. **Verify before you assume** — confirm current behavior (tests/repro) before changing semantics; search for existing helpers before introducing new ones.

## When something's missing

If guidance you need isn't here, that's a signal to improve this file or add a narrowly-scoped skill (see AGENTS.md "Self-improvement loop"). Keep skills task-focused; link to docs instead of duplicating them.
