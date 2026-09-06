# Testing & Verification

These commands keep CI and local development aligned. Treat them as the required checklist whenever a change touches executable code.

## Required checklist

| Step | Command | Purpose |
| --- | --- | --- |
| Format Rust | `cargo fmt --all` | Applies Rust formatting. The format check is part of `npm run lint`, not `npm test`. |
| Lint front-end + back-end | `npm run lint` | Runs `vue-tsc`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo fmt --all --check`, matching CI. |
| Test suites | `npm test` | Runs `cargo nextest run --cargo-profile ci` followed by Vitest (`view/vitest.config.ts`). It does not run lint or formatting checks. |
| Smoke tests | `npm run smoke` | Runs `npm run build:smoke` (SPA plus Rust profile `smoke`) before the specs under `smoke/tests/`. The binary is `target/smoke/lotar`. Check build output for warnings as well as test failures. |

> Policy recap: code changes require `npm run lint`, `npm test`, and `npm run smoke`. Use `cargo nextest`, never plain `cargo test`.

## Targeted runs

- `npm run test:rust` – Rust-only suite via `cargo nextest`, useful while iterating on CLI/service changes.
- `npm run test:ui` – Vitest unit tests for `view/` components.
- `npm run test:smoke:quick` – Executes smoke specs without rebuilding. Use only with fresh artifacts from `npm run build:smoke`; the harness prefers the smoke binary over a release binary.
- `npm run build:web` – Rebuilds the SPA into `target/web` and compressed embedding assets into `target/web-embed`. A fresh worktree needs these before Rust compilation; rebuild Rust after asset changes to refresh the embedded UI.

## Troubleshooting tips

- Use `RUST_LOG=debug` or `LOTAR_DEBUG=1` when a failing test needs more context; most suites respect these env vars.
- Smoke tests assume Playwright dependencies are installed (`npm run playwright:install` installs Chromium + system deps).
- Nextest reports live under `target/nextest`; compiled artifacts live in Cargo's target directories. Nextest archives package test binaries for reuse, not cache pruning.

Related docs: [Architecture overview](./main.md) for the build pipeline and [Environment variables](./environment.md) for relevant toggles.
