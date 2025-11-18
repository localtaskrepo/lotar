# Developer Documentation

The files under `docs/developers/` explain _how_ LoTaR works: module ownership, data flow, and the tests that keep each surface stable. End-user walkthroughs now live exclusively in `docs/help/`; reference them whenever you need to describe behavior without diving into implementation details.

## Folder layout

- `main.md` – Architectural overview: crate layout, runtime layers, and build pipeline.
- `cli.md` – Command internals, including handler mappings, supporting services, and verifying tests.
- `config-reference.md`, `environment.md`, `precedence.md`, `task-model.md`, `identity.md` – Reference material for value resolution and schemas.
- `sse.md`, `mcp.md`, `mcp-tools.md`, `api-quick-reference.md` – Integration surfaces (HTTP, SSE, MCP, REST DTOs).
- `testing.md` – Build/lint/test checklist for CI parity.

Legacy per-command files (`add.md`, `status.md`, …) are intentionally slim pointers that keep old links working while the substantive content resides in `cli.md`.

## Working guidelines

1. When changing behavior, update the relevant developer doc at the same time. Call out modules (`src/...`) and tests (`tests/...` or `smoke/tests/...`) so future maintainers know where to look.
2. Summarize the user-facing impact separately under `docs/help/` and link back to the developer doc if engineers need more context.
3. If you introduce a brand-new subsystem, add a short section to `main.md` or `cli.md` and reference it from `index.md` so it’s discoverable.
4. Keep the docs current as part of your feature definition; reviewers should be able to rely on this folder as the single source of truth for implementation notes.

This split keeps the help pages approachable for users while giving maintainers a single, living knowledge base about the internals.
