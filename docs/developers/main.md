# Architecture Overview

LoTaR is a Rust workspace with a Vue front-end bundle. The CLI, REST server, MCP process, and scanners all share the same storage + configuration primitives under `src/`. This document sketches the moving parts so you know where to look when changing behavior.

## Repository layout (high level)

| Path | Role |
| --- | --- |
| `src/cli` | Clap command graph, argument structs, preprocessors, and per-command handlers. |
| `src/services` | Business logic (task CRUD, sprint ops, analytics, code reference management, audit helpers). |
| `src/storage` | YAML persistence plus higher-level managers. `storage/manager::Storage` handles reads/writes under `.tasks`. |
| `src/workspace.rs` | Tasks directory resolution, search heuristics, and config loading. |
| `src/web_server.rs` + `src/routes.rs` | HTTP server, SSE endpoints, REST router, and static asset serving. |
| `src/mcp` | JSON-RPC server shared with IDEs or agents. |
| `view/` | SPA source consumed by Vite. Built assets land in `target/web/` via `npm run build:web`. |
| `docs/help/` | End-user guidance. `docs/developers/` (this folder) now focuses on internals only. |

## Runtime layers

1. **CLI entry (`src/main.rs`)** – Parses args via Clap, runs `cli::preprocess::apply_globals`, and dispatches to the handler defined in `cli::handlers`. Output flows through `output::OutputRenderer` so JSON/Text/Markdown stay consistent.
2. **Project + config resolution** – `workspace::TasksDirectoryResolver` locates `.tasks`. `cli::project::ProjectResolver` merges config from global/project scopes, applying precedence (see `precedence.md`). All identity-based automation lives in `utils::identity*`.
3. **Services + storage** – Handlers call into `services::*` which wrap storage operations in `storage::manager::Storage` and the DTOs in `storage::task`. Most write paths round-trip through `task_service.rs` so field validation, history, references, and automation remain centralized.
4. **API + SSE** – `web_server.rs` embeds Axum for REST and SSE. Routes in `routes.rs` call the same services the CLI uses, so behavior stays in sync. SSE emits from `api_events.rs` and file watchers started in `web_server.rs`.
5. **MCP + tooling** – `mcp/server.rs` exposes the same mutations over JSON-RPC/stdio. Tool definitions live in `mcp/server/tools.rs` and map directly to service calls.
6. **Front-end** – `view/` is compiled by Vite. The CLI’s `lotar serve` handler spawns the server, mounts static assets from `target/web`, and proxies API calls to the Axum router.

## Data flow cheatsheet

1. CLI parses args → `cli::handlers::<command>` builds a context (project, renderer, validator).
2. Handler calls a service (`task_service`, `sprint_service`, etc.).
3. Service loads YAML via `Storage`, mutates it, and persists using `serde_yaml`.
4. Changes emit structured events (`api_events`) that feed SSE, MCP notifications, and the audit log.
5. Renderers convert the result into text/json/markdown using `output::renderers::*`.

## Build & release

- Front-end: `npm ci && npm run build:web` compiles the SPA into `target/web`. Smoke tests (`npm run smoke`) depend on this build.
- Backend: `cargo fmt`, `cargo clippy`, and `cargo nextest run --release` match CI. Use `npm test` to fan out into Nextest + Vitest per repo policy.
- Release builds run `vite build && cargo build --release` (mirrors `npm run build`). The resulting binary embeds static assets from `target/web` using `include_dir!` (see `src/web_server.rs`).

## Watchers & automation

- Filesystem watchers (notify-based) live in `web_server.rs` and `mcp/server/watchers.rs`. They monitor `.tasks/*/*.yml` for change broadcasts.
- Scan automation (`scanner.rs`) rewrites references and applies inline metadata. CLI flags configure it via `ScanHandler`.
- Branch/tag inference utilities live under `utils::task_intel`. They’re invoked during add/status flows and wired through config toggles under `auto.*`.

## Where to start when…

- **Adding a command** – define args in `src/cli/args`, add handler logic under `src/cli/handlers`, hook it up in `cli::Commands`, then document the internals in `cli.md`.
- **Changing workspace discovery** – update `workspace.rs` and `cli::project`, plus refresh `environment.md` and `precedence.md`.
- **Modifying REST/MCP behavior** – adjust `routes.rs` or `mcp/server/*.rs` and double-check the shared DTOs in `api_types.rs`. Update `api-quick-reference.md` or `mcp-tools.md` as needed.
- **Tweaking automation** – look at `services/task_service.rs`, `utils/identity.rs`, and `config::*`. Tests usually live under `tests/auto_*` or `tests/cli_*`.

For command-specific details, jump to [CLI Internals](./cli.md). For schemas and precedence, see the references listed in `index.md`.
