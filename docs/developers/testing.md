# Testing & Verification

These commands keep CI and local development aligned. Treat them as the required checklist whenever a change touches executable code.

## Required checklist

| Step | Command | Purpose |
| --- | --- | --- |
| Format code | `npm run fmt` | Applies `cargo fmt` and Prettier (TypeScript, Vue, smoke tests, scripts). The format checks are part of `npm run lint`, not `npm test`. |
| Lint front-end + back-end | `npm run lint` | Runs `vue-tsc` and `tsc` on the smoke suite (`npm run lint:frontend`), oxlint and ESLint (`npm run lint:js`), the Prettier check (`npm run fmt:js:check`), `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo fmt --all --check`, matching CI. `npm run lint:agent` runs the same with short output. |
| Dependency policy | `npm run check:deps` | Runs `cargo deny check` (RustSec advisories, licenses, banned crates, sources) against `deny.toml`; CI runs it in the Security Audit job. Needs `cargo install cargo-deny --locked`. |
| Test suites | `npm test` | Runs `cargo nextest run --cargo-profile local` (CI runs nextest with the `ci` profile), Vitest (`view/vitest.config.ts`), the release-page tests and the git hook tests (`npm run test:hooks`). It does not run lint or formatting checks. |
| Smoke tests | `npm run smoke` | Runs `npm run build:smoke` (SPA plus Rust profile `smoke`) before the specs under `smoke/tests/`. The binary is `target/smoke/lotar`. Check build output for warnings as well as test failures. |

> Policy recap: code changes require `npm run lint`, `npm test`, and `npm run smoke`. Use `cargo nextest`, never plain `cargo test`.

## Code quality tools

| Area | Tooling | Where it runs |
| --- | --- | --- |
| Rust style and lints | `rustfmt` (edition 2024), Clippy with `-D warnings`; lint levels in `Cargo.toml` `[lints]` (the owner's standard, including `unwrap_used`: outside tests use `?` with context or `expect("why it cannot fail")`; `clippy.toml` allows unwrap in tests and sets lotar's `large-error-threshold`); suppress with `#[expect(lint, reason = "...")]` so stale suppressions are reported | `npm run lint`, pre-push hook (short output), CI |
| Frontend types, lints and style | `vue-tsc` (view) and `tsc` (smoke); oxlint with type-aware rules for all TypeScript and JavaScript (`.oxlintrc.json` extends `oxlintrc.standard.json`, an unchanged copy of the owner's shared standard; `!` non-null assertions are banned outside tests: use the throwing lookups in `view/utils/lookup.ts`, also in templates, which oxlint does not read); ESLint for Vue templates only (`eslint.config.mjs`, eslint-plugin-vue); Prettier (`.prettierrc.json`). oxlint's type checker reads no `.vue` files, so their `<script>` blocks get only the rules that need no types and `vue-tsc` stays the type check; shared types live in `.ts` files | `npm run lint`, pre-commit (staged files) and pre-push hooks, CI |
| Tests | nextest (manifest vs JUnit check in CI), Vitest, Playwright smoke, hook and release-page Node tests | `npm test`, `npm run smoke`, CI |
| Dependencies | `cargo deny check` (`deny.toml`), Dependabot alerts, a monthly dependency pass (cargo, npm, GitHub Actions), SHA-pinned workflow actions | `npm run check:deps`, CI Security Audit job |
| Code scanning | CodeQL for Rust, TypeScript/Vue and workflows | CI on push, pull request and weekly |
| Coverage (report only) | `npm run coverage:rust` (cargo-llvm-cov), `npm run coverage:ui` (Vitest v8); reports under `target/coverage/` | Locally, and the Coverage workflow after pushes to `main` |

Git hooks are opt-in (`npm run hooks:install`) and take seconds; see [.githooks/SETUP.md](../../.githooks/SETUP.md). Editor defaults are in `.editorconfig`.

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
