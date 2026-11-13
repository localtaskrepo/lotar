---
applyTo: "**/*.rs"
excludeAgent: ["code-review"]
---

# Backend instructions (LoTaR)

## Tech + structure

- Rust (edition 2024), single `lotar` binary: CLI + HTTP server.
- CLI commands: `crates/lotar-lib/src/cli/` (args under `crates/lotar-lib/src/cli/args/`, handlers under `crates/lotar-lib/src/cli/handlers/`).
- API/server surface: `crates/lotar-lib/src/api_server.rs`, `crates/lotar-lib/src/web_server.rs`, `crates/lotar-lib/src/routes/` (custom HTTP transport, not Axum).
- DTOs/contracts: `crates/lotar-lib/src/api_types.rs`.
- Storage/domain: `crates/lotar-lib/src/storage/`, `crates/lotar-lib/src/project.rs`, `crates/lotar-lib/src/workspace.rs`.
- Services: `crates/lotar-lib/src/services/` (task, sprint, sync, agent, automation, …).
- MCP server (stdio): `crates/lotar-lib/src/mcp/`.

## Tests + lint

- Use the repo scripts (see the `testing-strategy` skill): `npm run lint`, `npm run test:rust`, `npm test`.
- **Do NOT use `cargo test`** — use `cargo nextest` via the scripts.

## Contract sync

Changing a REST input/output shape means updating **all three** together:
`crates/lotar-lib/src/api_types.rs`, `view/api/types.ts`, `docs/openapi.json`. See the
`api-contract-change-end-to-end` skill.

## Change discipline

- Keep task YAML storage backwards compatible (user-owned files under `.tasks/`).
- Prefer existing error/validation patterns (`thiserror`, `crates/lotar-lib/src/errors.rs`) over new ad-hoc types.
