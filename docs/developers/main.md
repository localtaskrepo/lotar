# Architecture Overview

LoTaR is a Rust workspace with a Vue front-end bundle. The CLI, REST server, MCP process, and scanners all share the same storage + configuration primitives under `src/`. This document sketches the moving parts so you know where to look when changing behavior.

## Repository layout (high level)

| Path | Role |
| --- | --- |
| `src/cli` | Clap command graph, argument structs, preprocessors, and per-command handlers. |
| `src/services` | Business logic (task CRUD, sprint ops, analytics, code reference management, audit helpers). |
| `src/storage` | YAML persistence plus higher-level managers. `storage/manager::Storage` handles reads/writes under `.tasks`. |
| `src/workspace.rs` | Tasks directory resolution, search heuristics, and config loading. |
| `src/web_server.rs`, `src/api_server.rs`, `src/routes/` | HTTP transport, request routing, REST handlers, SSE, and static asset serving. |
| `src/mcp` | JSON-RPC server shared with IDEs or agents. |
| `view/` | SPA source consumed by Vite. Built assets land in `target/web/` via `npm run build:web`. |
| `docs/help/` | End-user guidance. `docs/developers/` (this folder) now focuses on internals only. |

## Runtime layers

1. **CLI entry (`src/main.rs`)** – Normalizes arguments with `cli::preprocess::normalize_args`, parses them with Clap, and dispatches to `cli::handlers`. `output::OutputRenderer` produces text or JSON; table/Markdown options currently alias text.
2. **Project + config resolution** – `workspace::TasksDirectoryResolver` locates `.tasks`; `config::resolution` is the shared configuration merge engine (see `precedence.md`). Identity-based automation uses `utils::identity*`.
3. **Services + storage** – Handlers call `services::*` and `storage::manager::Storage`. Persisted tasks live in `storage::task`; API DTOs live in `api_types.rs`. Task mutations use `task_service.rs`, though some reference and attachment paths still mutate storage directly.
4. **API + SSE** – `web_server.rs` implements HTTP over `TcpListener`, with request dispatch in `api_server.rs` and handlers under `routes/`; it does not use Axum. Services and `api_events.rs` supply events for SSE, alongside filesystem watchers.
5. **MCP + tooling** – `mcp/server.rs` exposes the same mutations over JSON-RPC/stdio. Tool definitions live in `mcp/server/tools.rs` and map directly to service calls.
6. **Front-end** – Vite compiles `view/`. `lotar serve` serves embedded assets by default or a configured external UI directory, and handles `/api/*` requests in the same server.

## Data flow cheatsheet

1. CLI parses args → `cli::handlers::<command>` builds a context (project, renderer, validator).
2. Handler calls a service (`task_service`, `sprint_service`, etc.).
3. Service loads YAML via `Storage`, mutates it, and persists using `serde_yaml`.
4. Changes emit structured events (`api_events`) that feed SSE, MCP notifications, and the audit log.
5. `output::OutputRenderer` converts CLI results into text or JSON using `output::{text,json}`.

## Build & release

- Front-end: `npm ci && npm run build:web` compiles the SPA into `target/web`. Smoke tests (`npm run smoke`) depend on this build.
- Backend: `npm run lint` runs typechecking, Clippy, and formatting checks; `npm test` runs Nextest with Cargo profile `ci`, then Vitest.
- Release: `npm run build` builds and compresses the SPA, then runs `cargo build --release`. The binary embeds `target/web-embed` with `include_dir!` (see `src/web_server.rs`).
- Smoke: `npm run smoke` rebuilds via `npm run build:smoke`, producing `target/smoke/lotar`, then executes the smoke suite. No separate release build is needed.

## Watchers & automation

- Filesystem watchers (notify-based) live in `web_server.rs` and `mcp/server/watchers.rs`. They monitor `.tasks/*/*.yml` for change broadcasts.
- Scan automation (`scanner.rs`) rewrites references and applies inline metadata. CLI flags configure it via `ScanHandler`.
- Branch/tag inference utilities live under `utils::task_intel`. They’re invoked during add/status flows and wired through config toggles under `auto.*`.

## Where to start when…

- **Adding a command** – define args in `src/cli/args`, add handler logic under `src/cli/handlers`, hook it up in `cli::Commands`, then document the internals in `cli.md`.
- **Changing workspace discovery** – update `workspace.rs` and `cli::project`, plus refresh `environment.md` and `precedence.md`.
- **Modifying REST/MCP behavior** – adjust `src/routes/` or `src/mcp/server/` and check `src/api_types.rs`. REST shape changes also require `view/api/types.ts`, `docs/openapi.json`, and the relevant help pages.
- **Tweaking automation** – look at `services/task_service.rs`, `utils/identity.rs`, and `config::*`. Tests usually live under `tests/auto_*` or `tests/cli_*`.

For command-specific details, jump to [CLI Internals](./cli.md). For schemas and precedence, see the references listed in `index.md`.
