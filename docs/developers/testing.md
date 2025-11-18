# Testing & Verification

These commands keep CI and local development aligned. Treat them as the required checklist whenever a change touches executable code.

## Required checklist

| Step | Command | Purpose |
| --- | --- | --- |
| Format Rust | `cargo fmt --all` | Ensures tree-wide formatting before lint/tests fan out. `npm test` will fail fast if formatting drifts, but it is cheaper to run rustfmt directly while iterating. |
| Lint front-end + back-end | `npm run lint` | Executes `vue-tsc` for the SPA and `cargo clippy --all-targets --all-features -D warnings` for Rust, matching CI. |
| Test suites | `npm test` | Wrapper that runs `cargo nextest run --release` and Vitest (`view/vitest.config.ts`). Always run this after code changes. |
| Smoke tests | `npm run smoke` | Builds the SPA + release binary (`npm run build`) and executes the smoke specs under `smoke/tests/`. This satisfies the “build without warnings” requirement baked into the project’s automation. |

> Policy recap: any PR that changes code must complete the full `npm test` + `npm run smoke` cycle locally before it is considered done.

## Targeted runs

- `npm run test:rust` – Rust-only suite via `cargo nextest`, useful while iterating on CLI/service changes.
- `npm run test:ui` – Vitest unit tests for `view/` components.
- `npm run test:smoke:quick` – Executes smoke specs without rebuilding (`vite` + `cargo`) first. Handy when you already have fresh artifacts from a prior `npm run build`.
- `npm run build:web` – Rebuilds just the SPA bundle into `target/web` when tweaking front-end assets without touching Rust.

## Troubleshooting tips

- Use `RUST_LOG=debug` or `LOTAR_DEBUG=1` when a failing test needs more context; most suites respect these env vars.
- Smoke tests assume Playwright dependencies are installed (`npm run playwright:install` installs Chromium + system deps).
- Keep an eye on `target/nextest` cache size. `cargo nextest run --release --archive-file` can prune artifacts if disk pressure becomes an issue.

Related docs: [Architecture overview](./main.md) for the build pipeline and [Environment variables](./environment.md) for relevant toggles.

