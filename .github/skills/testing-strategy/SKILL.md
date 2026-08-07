---
name: testing-strategy
description: Use this to run tests quickly and target a subset, then widen to CI parity. Covers Rust (nextest), UI (vitest), and smoke (Playwright), plus low-noise agent variants.
---

## Strategy

- **Start as narrow as possible** (one failing test / one file), then **widen** to the full suite.
- After fixing an issue, run the full gates before declaring done: `npm test` (Rust + UI) and `npm run smoke`.
- Repo policy: **do NOT use `cargo test`** — use `cargo nextest` via the `npm` scripts below.

## Rust (nextest)

- Full (CI-like profile): `npm run test:rust`
- Filter by substring: `npm run test:rust -- <substring>`
- List exact test names: `cargo nextest list --cargo-profile ci`
- Pass-through test-binary args (e.g. `--nocapture`): `cargo nextest run --cargo-profile ci <filter> -- --nocapture`
- Fast clippy-only loop (lib+bins, no `--all-targets`/`--all-features`): `npm run lint:backend:fast`

## UI unit tests (vitest)

- Full: `npm run test:ui`
- By name: `npm run test:ui -- -t "<substring>"`
- Single file: `npm run test:ui -- view/<path>/<file>.test.ts`
- Typecheck/lint frontend: `npm run lint:frontend`

## Smoke (Playwright + vitest harness)

- Full (builds first): `npm run smoke`
- Quick (no rebuild — assumes fresh artifacts): `npm run test:smoke:quick`
- By name: `npm run test:smoke:quick -- -t "<substring>"`
- By file: `npm run test:smoke:quick -- smoke/tests/<suite>.smoke.spec.ts`
- Tight in-process loop: `npx vitest watch --config smoke/vitest.config.ts --runInBand`
- Install browsers if missing: `npm run playwright:install`

If smoke fails on environment/binary/server issues, switch to the `smoke-suite-debugging` skill.

## Low-noise output (agent / CI logs)

Same commands with ANSI disabled and compact reporters:

- Lint: `npm run lint:agent`
- Rust + UI tests: `npm run test:agent`
- Rust-only: `npm run test:rust:agent`
- UI-only: `npm run test:ui:agent`
- Smoke (builds): `npm run test:smoke:agent` · quick: `npm run test:smoke:quick:agent`

These are still the same checks; the completion bar is `npm run lint`, `npm test`, `npm run smoke` (see AGENTS.md).
