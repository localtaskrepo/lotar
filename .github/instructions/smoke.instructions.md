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

- `npm run smoke` builds profile `smoke` and embeds compressed SPA assets from `target/web-embed`.
- The harness prefers `target/smoke/lotar`, then `target/release/lotar`; `LOTAR_BINARY_PATH` can override the binary.
