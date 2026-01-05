# Copilot instructions (LoTaR)

Keep this file short and evergreen.

## Repo quick facts

- Rust CLI + HTTP server, with file-backed YAML storage.
- Vue 3 + TypeScript SPA under `view/`, bundled by Vite into `target/web` and served by the Rust binary.
- Node.js: `24.x` (see `package.json` engines).
- Path-scoped rules: `.github/instructions/`.
- Runbooks (“skills”): `.github/skills/`.
- Docs: `docs/help/*`, `docs/developers/*`, and the REST schema in `docs/openapi.json`.

## Build, test, lint (commands that CI runs)

- Install: `npm ci`
- Format (Rust, CI check): `cargo fmt --all --check` (local: `cargo fmt --all`)
- Lint (frontend + backend): `npm run lint`
- Unit/integration tests: `npm test`
- Smoke suite (builds artifacts first): `npm run smoke`

## Targeted runs (prefer targeted first, then widen)

- Rust-only tests: `npm run test:rust` (uses `cargo nextest`)
- UI unit tests: `npm run test:ui`
- Smoke without rebuilding: `npm run test:smoke:quick` (requires fresh artifacts)

## Agent-friendly commands (reduced output noise)

- Lint (no ANSI): `npm run lint:agent`
- Unit/integration tests (dot reporter, no ANSI): `npm run test:agent`
- Rust-only tests: `npm run test:rust:agent`
- UI unit tests: `npm run test:ui:agent`
- Smoke (builds first): `npm run test:smoke:agent` (or quick: `npm run test:smoke:quick:agent`)

## Engineering expectations

- Add/adjust tests for behavior changes.
- Don’t log or paste secrets/PII (tokens, auth headers, cookies). Treat `.env*` and credential files as sensitive.
- If unrelated changes exist in the working tree, ignore them unless the user explicitly asks you to coordinate.
- Don’t “chase” unrelated failures; start with targeted checks for your scoped changes.
- Git is user-controlled: don’t commit/stage/revert; use `.history/` for recovery.
- Keep working until the feature is complete and ready for review, or you need user input.
- Before starting non-trivial work, open the relevant runbooks under `.github/skills/` (start with `.github/skills/skill-discovery/SKILL.md`).

## Contract sync (Rust ↔ UI ↔ OpenAPI)

- Rust API DTOs: `src/api_types.rs`
- UI DTOs: `view/api/types.ts`
- REST schema: `docs/openapi.json`

If you change an endpoint’s request/response shape, keep all three aligned and update any relevant docs in `docs/help/*`.

## Build & assets notes

- Static files are served from `target/web` (built via `npm run build:web` or `npm run build`). Release builds embed these assets.
- Prefer `npm run smoke` as the “did everything still work” validation: it rebuilds and runs the browser-driven smoke suite.

## Common CLI gotcha

- `lotar serve` uses `--port <n>` for the server port. The short `-p` flag is reserved for the global `--project` option (see `docs/help/serve.md`).
