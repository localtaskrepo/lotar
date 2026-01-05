---
applyTo: "**/*.rs"
excludeAgent: ["code-review"]
---

# Backend instructions (LoTaR)

## Tech + structure

- Rust (edition 2024), single `lotar` binary: CLI + HTTP server.
- CLI commands: `src/cli/`
- API/server surface: `src/api_server.rs`, `src/web_server.rs`, `src/routes.rs`
- DTOs/contracts: `src/api_types.rs`
- Storage/domain: `src/storage/`, `src/project.rs`, `src/workspace.rs`

## Tests + lint (use repo scripts)

- Format: `cargo fmt --all`
- Lint: `npm run lint` (includes `cargo clippy --all-targets --all-features -- -D warnings`)
- Tests (preferred): `npm test`
- Rust-only: `npm run test:rust` (runs `cargo nextest run --cargo-profile ci`)

## Test runner policy

- Do NOT use `cargo test` (project policy: `cargo nextest`).

## Contract sync

- If you change REST inputs/outputs, update:
  - Rust DTOs: `src/api_types.rs`
  - UI DTOs: `view/api/types.ts`
  - OpenAPI: `docs/openapi.json`

## Change discipline

- Keep task YAML storage backwards compatible when practical (user-owned files).
- Prefer existing error/validation patterns (`thiserror`, `src/errors.rs`) over new ad-hoc types.
