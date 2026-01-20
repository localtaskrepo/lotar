# Developer Index

Use this page to jump into the implementation notes you need while building LoTaR.

## Architecture & build

- [Architecture overview](./main.md): crate layout, services, watchers, and build pipeline.
- [Testing & verification](./testing.md): formatting/lint/test expectations shared with CI.

## CLI internals

- [CLI internals](./cli.md): execution pipeline plus sections for each command (`lotar add`, `list`, `status`, `scan`, `serve`, `mcp`, etc.).
- Command-specific files (e.g., `add.md`, `status.md`) now point at the relevant anchors inside `cli.md` to keep old links alive.

## Configuration & data

- [Configuration reference](./config-reference.md)
- [Environment overrides](./environment.md)
- [Resolution & precedence](./precedence.md)
- [Identity & users](./identity.md)
- [Task model](./task-model.md)
- [Templates](./templates.md)

## Integrations

- [Serve/MCP/web server](./mcp.md) and [MCP tools](./mcp-tools.md)
- [Manual sync (push/pull)](./sync.md)
- [SSE events](./sse.md)
- [API quick reference](./api-quick-reference.md) + `docs/openapi.json`
- [Preferences](./preferences.md) for browser-only state management

Need end-user guidance? See `docs/help/` and link back to these files when you need to cite modules or tests.
