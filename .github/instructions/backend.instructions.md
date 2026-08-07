---
applyTo: "**/*.rs"
excludeAgent: ["code-review"]
---

# Backend instructions (LoTaR)

## Tech + structure

- Rust (edition 2024), single `lotar` binary: CLI + HTTP server.
- CLI commands: `src/cli/` (args under `src/cli/args/`, handlers under `src/cli/handlers/`).
- API/server surface: `src/api_server.rs`, `src/web_server.rs`, `src/routes.rs`.
- DTOs/contracts: `src/api_types.rs`.
- Storage/domain: `src/storage/`, `src/project.rs`, `src/workspace.rs`.
- Services: `src/services/` (task, sprint, sync, agent, automation, …).
- MCP server (stdio): `src/mcp/`.

## Tests + lint

- Use the repo scripts (see the `testing-strategy` skill): `npm run lint`, `npm run test:rust`, `npm test`.
- **Do NOT use `cargo test`** — use `cargo nextest` via the scripts.

## Contract sync

Changing a REST input/output shape means updating **all three** together:
`src/api_types.rs`, `view/api/types.ts`, `docs/openapi.json`. See the
`api-contract-change-end-to-end` skill.

## Change discipline

- Keep task YAML storage backwards compatible (user-owned files under `.tasks/`).
- Prefer existing error/validation patterns (`thiserror`, `src/errors.rs`) over new ad-hoc types.
