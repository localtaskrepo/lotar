# lotar mcp internals

`lotar mcp` spawns the JSON-RPC/stdio server described in `src/mcp/server.rs`. The process reuses the same services as the CLI and REST API, restarts itself when the binary changes, and notifies hosts whenever configuration tweaks alter enum hints. User-facing instructions live in [../help/mcp.md](../help/mcp.md); this file focuses on how the subsystem is wired.

## Components

- `src/mcp/server.rs` – main loop, JSON-RPC framing, request dispatcher, and auto-reload watchdog.
- `src/mcp/server/watchers.rs` – fs + config watchers that emit `tools/listChanged` whenever `.tasks/**` updates change enum hints (projects, statuses, priorities, tags, sprints, templates).
- `src/mcp/server/tools.rs` – registry that maps method names to handler functions, provides schema metadata, and feeds `schema_discover`.
- `src/mcp/server/handlers/*` – business logic layers that call into `services::*` and emit pretty-printed payloads for MCP hosts.
- `src/mcp/server/context.rs` – shared helpers for resolving workspaces, renderer selection, and identity.

## Request lifecycle

1. `lotar mcp` inherits stdin/stdout/stderr from the host. All JSON-RPC payloads flow through stdout; tracing/log output stays on stderr so host adapters can parse responses safely.
2. `Server::serve` reads frames either through MCP/LSP `Content-Length` headers or newline-delimited JSON (for quick tests). Both modes funnel into the same `dispatch` function.
3. The dispatcher routes `initialize`, `tools/list`, `schema/discover`, `tools/call`, and `logging/setLevel` requests. Tool invocations are turned into handler structs that mirror the CLI args (see `mcp-tools.md`).
4. Each handler builds a `Context` (project + config resolver, renderer, validator), calls the corresponding service, and formats the response into MCP `content` entries containing pretty JSON.
5. On validation failure the handler raises a JSON-RPC error with `error.data.details` so hosts can surface enum hints to users.

## Tool graph & schema

- Every tool is defined once in `tools.rs` and bound to a handler module. The same definition feeds both the dispatcher and the schema/enum metadata returned by `schema_discover`.
- When project config changes (e.g., adding a status) the watcher recomputes enum hints and pushes `tools/listChanged { hintCategories: [...] }`. Hosts should respond by calling `tools/list` or `schema/discover` to refresh their cached UI.
- See [MCP Tools Reference](./mcp-tools.md) for per-tool parameters, validation notes, and payload examples. The CLI’s `lotar mcp` entry in [CLI Internals](./cli.md#lotar-mcp) lists the high-level behaviors/tests that protect the server.

## Hot reload + watchers

- `auto_reload::Watchdog` (inside `server.rs`) tracks the `lotar` binary path. When the file changes, the server exits with a restart code so supervising hosts can relaunch without manual intervention. Set `LOTAR_MCP_AUTORELOAD=0` to disable this in environments that already handle restarts.
- File-system watchers observe `.tasks/**` and `.git` metadata. When project dirs are added or removed the watcher emits `project_changed` hints so enum caches update immediately.
- Config reloads reuse the same precedence chain described in `precedence.md`. The server keeps a cached `ResolvedConfig`, invalidates it when the watcher fires, and rebuilds it on the next request.

## Testing & diagnostics

- Unit coverage: `tests/mcp_server_unit_test.rs` focuses on framing, error propagation, and watcher notifications.
- Smoke tests: `smoke/tests/mcp.*.smoke.spec.ts` spawn the server via CLI, run real JSON-RPC calls, and assert responses/enum hints.
- Enable `LOTAR_DEBUG=1` for verbose logs (written to `/tmp/lotar_mcp_debug.log`) when triaging host issues. `logging/setLevel` lets hosts bump the tracing level dynamically without restarting the process.

## Related docs

- [CLI Internals > lotar mcp](./cli.md#lotar-mcp)
- [MCP Tools Reference](./mcp-tools.md)
- [../help/mcp.md](../help/mcp.md) (transport/usage from the host’s perspective)

