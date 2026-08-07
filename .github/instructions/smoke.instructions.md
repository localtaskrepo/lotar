---
applyTo: "smoke/**"
excludeAgent: ["code-review"]
---

# Smoke/E2E instructions (LoTaR)

## What smoke tests cover

- End-to-end flows across the built Rust binary + embedded web assets.
- Specs live under `smoke/tests/`, run via the Vitest config at `smoke/vitest.config.ts`.
- Tests use isolated temp workspaces (`smoke/helpers/workspace.ts`) — they never touch the repo `.tasks/`.

## Commands

- Full smoke (builds first): `npm run smoke`
- Quick smoke (no rebuild): `npm run test:smoke:quick`
- Target by name: append `-- -t "<substring>"` (see `testing-strategy`)
- Install browsers: `npm run playwright:install`

If you hit environment/binary/server/port issues, switch to the `smoke-suite-debugging` skill.

## CI parity notes

- Smoke expects a built binary and web assets (release binary + `target/web`).
- CI may set `LOTAR_BINARY_PATH` to point at `target/release/lotar`.
