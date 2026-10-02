# lotar mcp internals

`lotar mcp` spawns the JSON-RPC/stdio server described in `src/mcp/server.rs`. The process reuses the same services as the CLI and REST API, exits for a supervising host to relaunch when the binary changes, and notifies hosts whenever configuration tweaks alter enum hints. User-facing instructions live in [../help/mcp.md](../help/mcp.md); this file focuses on how the subsystem is wired.

## Components

- `src/mcp/server.rs` – stdio main loop (`run_stdio_server`), message routing, request dispatcher, and the binary-change auto-restart watcher thread.
- `src/mcp/server/transport.rs` – bounded stdio frame reader: NDJSON lines are primary, and a message whose first line is a `Content-Length:` header is read as an LSP-style framed compatibility body (same 10 MiB ceiling).
- NDJSON limits exclude line terminators (LF or CRLF). Oversized lines are drained without unbounded accumulation, return a parse error, and leave the following message intact.
- `src/mcp/server/session.rs` – initialize → awaiting-initialized → ready state machine. Only `notifications/initialized` after `initialize` grants readiness; deferred tool-list changes coalesce here until the session is ready.
- `src/mcp/server/registry.rs` – the single tool registry governing all dispatch: resolves direct methods and `tools/call` names (control-plane names never resolve), validates arguments against advertised schemas (fail-closed on unenforceable schemas), and converts recoverable domain failures into `isError` tool results. A test enforces registry parity with the 37 `tools/list` entries.
- `src/mcp/server/schema.rs` – the fail-closed schema validator (required, types, nullability, enum, oneOf, nested arrays/maps, numeric bounds); any keyword outside the enforced set rejects the schema rather than running unvalidated.
- `src/mcp/server/watchers.rs` – best-effort kernel file watcher plus a 2-second polling fallback; diffs enum hints and emits `notifications/tools/list_changed` whenever `.tasks/**` updates change enum hint categories (projects, statuses, priorities, types, members, tags, custom fields).
- `src/mcp/server/tools.rs` – tool definition/schema builder that feeds `tools/list`, `schema_discover`, and the registry's argument validation.
- `src/mcp/server/hints.rs` – gathers the current enum hints from the resolved workspace configuration.
- `src/mcp/server/handlers/*` – business logic layers that call into `services::*` and emit pretty-printed payloads for MCP hosts.

## Request lifecycle

1. `lotar mcp` inherits stdin/stdout/stderr from the host. All JSON-RPC payloads flow through stdout; tracing/log output stays on stderr so host adapters can parse responses safely.
2. `run_stdio_server` reads newline-delimited JSON — the MCP `2025-06-18` stdio standard. A line beginning with `Content-Length:` switches that session into the LSP-style framed compatibility mode; the output mode is sticky for the rest of the session, and both modes funnel into the same routing/dispatch path. Envelopes are validated before routing: malformed JSON answers `-32700` with `id: null`, and valid JSON that is not a request envelope (top-level array/string, bad `jsonrpc`/`method`, invalid `id`, non-object `params`) answers `-32600` Invalid Request.
3. `initialize` and `ping` answer in every session state. `initialize` validates its params strictly (`protocolVersion` string, `capabilities` object, `clientInfo` with `name`/`version` strings): a missing or non-string `protocolVersion` — or any malformed shape — returns `-32602` with `data.tool`/`data.issues`, while unsupported version *strings* negotiate to the supported `2025-06-18`. `ping` returns an empty `{}` result before and after ready.
4. Every other request requires a ready session: arrivals before the client's `notifications/initialized` are rejected with `-32002` (`Server not initialized`) before any dispatch or side effects. Only that notification, after `initialize`, flips the session to ready — an `initialized` before `initialize`, repeated copies, or any other notification never does. Notifications never receive replies and never execute tools.
5. Ready requests route through the single registry in `registry.rs`: `tools/list`, `schema/discover`, `tools/call`, and `logging/setLevel` as control methods, plus tool methods by name. `tools/call` resolves names through the same registry (control-plane and unknown names fail with `-32602 Unknown tool` — no recursive dispatch); unknown direct methods return `-32601`.
6. Arguments are schema-validated before any handler runs: missing required arguments, unknown fields, and type violations return `-32602` with `error.data.tool` + `error.data.issues`; schemas using keywords the validator cannot enforce fail closed. Handler errors outside the reserved JSON-RPC range become tool-execution results (`result.isError: true`, message/data preserved in the content text); `tools/call` adds the `functionResponse` compatibility wrapper, direct methods omit it.
7. Each handler resolves the workspace/config context for the call, invokes the corresponding service, and formats the response into MCP `content` entries containing pretty JSON. Validation failures raise a JSON-RPC error with `error.data.details` so hosts can surface enum hints to users.

## Tool graph & schema

- Every tool is defined once in `tools.rs` and bound to a handler in `registry.rs`; a test keeps the registry and the 37 `tools/list` entries in lockstep. The same definitions feed the dispatcher, `schema_discover`, and the registry's argument validation. Global enum hints stay pure annotations (never constraints); project-aware enum validation with suggestions remains in the handlers as `-32602`. Handler errors with custom codes (`-32000..-32006`, e.g. agent job-not-found) convert to `isError` tool results, while reserved protocol codes stay root errors.
- Tests also keep hinted and hint-less `inputSchema` values identical. Expected agent/sprint service failures use domain codes; genuine I/O and serialization faults retain `-32603` rather than being blanket-converted into domain results.
- When project config changes (e.g., adding a status) the watcher recomputes enum hints and pushes `notifications/tools/list_changed { hintCategories: [...] }` (the `initialize` capability key remains `tools.listChanged`). Hosts should respond by calling `tools/list` or `schema/discover` to refresh their cached UI.
- Pagination: list-style tools accept `limit` and `cursor` (0-based offset; `offset` alias is commonly supported) and return `hasMore` plus `nextCursor` for fetching the next page.
- See [MCP Tools Reference](./mcp-tools.md) for per-tool parameters, validation notes, and payload examples. The CLI’s `lotar mcp` entry in [CLI Internals](./cli.md#lotar-mcp) lists the high-level behaviors/tests that protect the server.

## Hot reload + watchers

- The binary-change watcher is a background thread inside `run_stdio_server` that polls the `lotar` executable’s modification time every 2 seconds and exits the process (exit code 0) when it changes, so a supervising host can relaunch the updated binary. Set `LOTAR_MCP_AUTORELOAD=0` to disable this in environments that already handle restarts.
- The tools watcher observes the tasks directory with a best-effort recursive kernel watcher plus a 2-second polling fallback (bursts are debounced), so changes are detected even where kernel file-watching is blocked. Events are filtered to the tasks root, its direct children, and `config.yml`; when the recomputed enum hints differ from the last snapshot, one notification is emitted with the changed categories. Changes observed before the session is ready are deferred and coalesced in `session.rs` into a single notification flushed right after `notifications/initialized`; afterwards they can fire while the connection is otherwise idle.
- Enum hints are re-resolved fresh from the workspace configuration (readonly, same precedence chain as `precedence.md`) on each `tools/list`/watcher pass, so config changes are reflected on the next request or notification even if a watcher event is missed.

## Testing & diagnostics

- Unit coverage: `tests/mcp_server_unit_test.rs` focuses on framing, error propagation, and watcher notifications; `src/mcp/server/mcp_server_tests.rs` covers workspace-aware server behavior in-tree.
- Smoke tests: `smoke/tests/mcp.*.smoke.spec.ts` spawn the server via CLI, run real JSON-RPC calls over stdio, and assert responses/enum hints.
- `logging/setLevel` accepts a level name, validates it, and records it for the session; unknown levels fall back to `info`. Log output stays on stderr.

## Related docs

- [CLI Internals > lotar mcp](./cli.md#lotar-mcp)
- [MCP Tools Reference](./mcp-tools.md)
- [../help/mcp.md](../help/mcp.md) (transport/usage from the host’s perspective)
