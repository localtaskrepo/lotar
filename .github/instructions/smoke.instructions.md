---
applyTo: "smoke/**"
excludeAgent: ["code-review"]
---

# Smoke/E2E instructions (LoTaR)

## What smoke tests cover

- End-to-end flows across the built Rust binary + embedded web assets.
- Specs live under `smoke/tests/` and run via Vitest config `smoke/vitest.config.ts`.

## Commands

- Full smoke (builds first): `npm run smoke` (same as `npm run test:smoke`)
- Quick smoke (no rebuild): `npm run test:smoke:quick`
- Install browsers (if needed): `npm run playwright:install`

## CI parity notes

- Smoke tests expect a built binary and web assets (release binary + `target/web`).
- CI may set `LOTAR_BINARY_PATH` to point at `target/release/lotar`.
