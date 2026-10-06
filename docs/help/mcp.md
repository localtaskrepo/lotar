# lotar mcp

Start the MCP (Model Context Protocol) JSON-RPC server. The process reads from stdin, writes to stdout, and exposes the same task/project/config primitives that power the CLI. Tool definitions auto-refresh when `.tasks/` metadata changes thanks to the built-in watcher.


## Usage

```bash
lotar mcp
```

Environment:
- `LOTAR_TASKS_DIR` — override the workspace search path.
- `LOTAR_MCP_AUTORELOAD=0` — disable the default binary-change watcher that exits the process when the `lotar` executable is replaced (useful when running under a supervisor that already restarts the process).

Transport:
- JSON-RPC 2.0 over stdio, following the MCP `2025-06-18` standard transport.
- The standard framing is newline-delimited JSON (NDJSON): one complete JSON-RPC object per line, terminated by `\n`, with no embedded newlines. Requests and notifications are read from stdin; responses are emitted to stdout, one object per line. This is the framing MCP hosts such as VS Code and Cursor use.
- NDJSON payloads are capped at 10 MiB, excluding the LF or CRLF terminator. An oversized line is drained through its terminator and answered with `-32700`; the next message can still be read. A final line at EOF need not have a terminator.
- `Content-Length: <n>\r\n\r\n<body>` framing (an LSP-style header block) is supported as an explicit, custom compatibility mode for older integrations. It is not the MCP stdio default and is not required by VS Code or other hosts. Once the first framed message is received, that session's responses use the same framing for the rest of the session — the output mode is sticky, not switched per message. Framed bodies are capped at 10 MiB (oversized frames are answered with a `-32700` parse error).
- Messages without an `id` are notifications and never receive a response.
- All log output goes to stderr; stdout remains pure JSON.

## Lifecycle

Sessions follow the MCP initialize → initialized handshake:

1. The client sends an `initialize` request with `protocolVersion`, `capabilities`, and `clientInfo` (`name`/`version`):

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-06-18",
    "capabilities": {},
    "clientInfo": { "name": "example-client", "version": "1.0.0" }
  }
}
```

2. The server responds with its capabilities and identity (the `initialize` result advertises the `tools.listChanged` capability under `capabilities.tools`):

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2025-06-18",
    "capabilities": {
      "tools": { "listChanged": true },
      "logging": {}
    },
    "serverInfo": { "name": "lotar-mcp", "version": "0.8.0" },
    "instructions": "Lotar MCP server exposes task, project, config, and agent tools."
  }
}
```

3. The client sends the `notifications/initialized` notification (`{"jsonrpc":"2.0","method":"notifications/initialized"}`). Operational calls — `tools/list`, `tools/call`, direct tool methods — are allowed only after that; requests arriving earlier are rejected with JSON-RPC error `-32002` (`Server not initialized`) before any dispatch or side effects. Only this notification, after `initialize`, makes the session ready: an `initialized` arriving before `initialize`, repeated copies, or any other notification never grants readiness. Notifications never receive replies and never execute tools.

Notes:
- Strict initialize fields: `protocolVersion` (string), `capabilities` (object), and `clientInfo` (object with `name`/`version` strings) are validated. A missing or non-string `protocolVersion` — or any other malformed initialize shape — is rejected with `-32602` Invalid params; `error.data.tool` and `error.data.issues` describe what failed.
- Version negotiation: the server supports `2025-06-18`. An `initialize` request naming an unsupported version *string* still receives `2025-06-18` in the result; per the spec, a client that cannot use that version should disconnect.
- `ping` is lifecycle-exempt: it is answered with an empty result `{}` both before and after the session is ready.

## Tool Surface

The registry binds all 38 advertised tools to their handlers; `tools/list` returns exactly this set (see [MCP Tools Reference](./mcp-tools.md) for full schemas):

| Tool | Purpose |
|------|---------|
| `whoami` | Resolve the identity used for `@me` (with optional explain output). |
| `task_create` | Create a task with optional type/priority/status overrides and custom fields. |
| `task_get` | Fetch a task by id (project inferred or provided). |
| `task_update` | Patch mutable fields, relationships, and custom fields. |
| `task_comment_add` | Append a comment to a task. |
| `task_comment_update` | Update an existing comment on a task. |
| `task_reference_add` | Attach one reference (link/file/code/jira/github/attachment) to a task. |
| `task_reference_remove` | Remove one reference from a task. |
| `task_bulk_update` | Patch multiple tasks in one call. |
| `task_bulk_comment_add` | Add the same comment to multiple tasks. |
| `task_bulk_reference_add` | Add the same reference to multiple tasks. |
| `task_bulk_reference_remove` | Remove the same reference from multiple tasks. |
| `task_delete` | Soft-delete by default; explicit `hard` removes the file and returns retained-attachment/incoming-relationship warnings. |
| `task_restore` | Restore a soft-deleted task without changing its content-modification timestamp. |
| `task_list` | Filtered, paginated listing (limit default 50, max 200) with enum hints. |
| `sprint_list` | List sprints with pagination + integrity hints. |
| `sprint_get` | Fetch one sprint by id. |
| `sprint_create` | Create a new sprint record. |
| `sprint_update` | Update sprint plan/actual metadata. |
| `sprint_summary` | Sprint summary report (metrics + timeline). |
| `sprint_burndown` | Sprint burndown report (series). |
| `sprint_velocity` | Sprint velocity report (rolling window). |
| `sprint_add` | Assign tasks to a sprint with optional cleanup + force flags. |
| `sprint_remove` | Remove tasks from a sprint (optionally scoped). |
| `sprint_delete` | Delete a sprint by id and optionally clean dangling references. |
| `sprint_backlog` | Return ranked backlog tasks with pagination and hints. |
| `project_list` | Enumerate known projects. |
| `project_stats` | Aggregate stats for a single project. |
| `config_show` | Render merged config (global or per project). |
| `config_set` | Persist settings and echo validation warnings/info. |
| `sync_pull` | Pull tasks from a configured sync remote (supports `dry_run`). |
| `sync_push` | Push tasks to a configured sync remote (supports `dry_run`). |
| `schema_discover` | Return the current tool definitions + enum hints (optionally filtered by name). |
| `agent_run` | Start an agent job for a ticket (runner or agent profile required). |
| `agent_status` | Get an agent job's status by id. |
| `agent_list_jobs` | List agent jobs with queue statistics. |
| `agent_cancel` | Cancel a running or queued agent job. |
| `agent_send_message` | Send a follow-up stdin message to a running agent job. |

Handshake/control methods include:
- initialize(params: { protocolVersion, capabilities, clientInfo }) -> { protocolVersion, capabilities, serverInfo, instructions }; malformed params return `-32602` Invalid params
- notifications/initialized (notification, no response) — completes the handshake; operational calls are allowed afterwards
- ping -> `{}` (empty result; answered before and after ready)
- tools/list -> { tools: [{ name, description, inputSchema }] }
- schema/discover(params?: { tool?: string }) -> same payload as tools/list but filtered
- tools/call (params: { name, arguments }) -> result of the named tool; unknown names — including control-plane methods — return `-32602` `Unknown tool: <name>`
- logging/setLevel(params: { level }) -> acknowledgement after recording the requested level

Notifications:
- `notifications/tools/list_changed` is emitted whenever `.tasks/` metadata or config changes alter enum hints (`hintCategories` identifies which values changed). The notification method is `notifications/tools/list_changed`; the capability key advertised in the `initialize` response remains `tools.listChanged`. Hosts should call `tools/list` again when they receive this event.
- Notifications can arrive while the connection is otherwise idle: the server watches the filesystem itself (best-effort kernel watcher with a 2-second polling fallback). Changes detected before the session is ready are deferred and coalesced into a single notification delivered right after `notifications/initialized`.

Notes
- All payloads use snake_case keys.
- Enum fields follow the same values as the REST API and CLI.
- Failures follow the split described in [Errors](#errors): protocol problems are JSON-RPC errors; domain failures are `isError` tool results.
- People fields (reporter, assignee) accept the special value `@me`. It resolves to the current user based on merged config default_reporter → git user → system username → project manifest author (last resort); identical logic is used inside the CLI and REST server. When `task_list` filters with `assignee: "@me"` and the identity cannot be resolved, the call fails closed as a tool-execution error (`isError` result) instead of returning an unfiltered list.

## Response Envelope

Every successful call returns a JSON-RPC result that wraps the human-readable payload inside `content` items (matching the current MCP spec). Example:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\n  \"status\": \"ok\",\n  \"tasks\": [],\n  \"enumHints\": {...}\n}"
      }
    ]
  }
}
```

- Tool handlers pretty-print JSON into the `text` field. Some sprint handlers prepend a short summary line followed by a pretty JSON blob.
- Hosts that expect structured data can parse the stringified JSON inside `content[*].text`.

## Errors

Failures are split into protocol errors and tool-execution failures.

Protocol errors are JSON-RPC error responses (`error.code`, `error.message`, optional `error.data`):

| Code | Meaning |
|------|---------|
| `-32700` | Parse error — text that is not valid JSON, bad framing, or an oversized frame. Answered with `id: null`. |
| `-32600` | Invalid Request — valid JSON that is not a valid JSON-RPC request envelope: a top-level array or string, wrong `jsonrpc`/`method`, an invalid `id` (null, bool, object, array, or fractional number), or non-object `params`. The `id` is echoed when it is valid. |
| `-32602` | Invalid params. Covers malformed `initialize` params (`error.data.tool: "initialize"`), tool arguments that fail the tool's advertised schema — missing required arguments, unknown fields, type/enum violations — reported with `error.data.tool` and `error.data.issues`, and unknown tool names under `tools/call` (`Unknown tool: <name>`). Control-plane methods such as `initialize` and `tools/list` are not callable as tools. |
| `-32601` | Method not found — unknown direct method. |
| `-32002` | Server not initialized — an operational request arrived before `notifications/initialized`; it is rejected before any dispatch or side effects. |

Tool-execution failures (semantic/domain errors — unknown task ids, unresolvable `@me`, membership failures, sync and agent-job failures, and similar) are successful JSON-RPC responses with `result.isError: true` and no root `error` object. The existing explanation/data envelope (for example the `Task update failed` message with its `data` payload) is carried in the `content[0].text` payload. Bulk tools keep partial outcomes in their `updated[]`/`failed[]` data instead of failing the whole call.

`tools/call` results additionally retain the `functionResponse` compatibility wrapper (successes and `isError` conversions alike); direct method calls omit that wrapper.

## Example (shell)

Complete the NDJSON handshake and one tool call in a single pipe (messages are processed in order):

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"shell","version":"1.0.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"task/list","params":{"project":"AUTH","limit":5}}' |
  lotar mcp |
  jq 'select(.id == 2) | .result.content[0].text | fromjson'
```

For older hosts that require framed messages, compute the `Content-Length` header from the payload instead of hard-coding a byte count:

```bash
payload='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"shell","version":"1.0.0"}}}'
printf 'Content-Length: %s\r\n\r\n%s' "$(printf '%s' "$payload" | wc -c | tr -d ' ')" "$payload" |
  lotar mcp
```

## Integrations

- Generic JSON-RPC clients: connect via stdio using newline-delimited JSON and complete the initialize/initialized handshake first. Use `Content-Length` framing only for older integrations that already speak it.
- AI tools (local agents/Copilot Chat): configure a custom tool provider that spawns `lotar mcp` and exchanges JSON-RPC messages. Map tool names 1:1 to the methods above.
- Long-running hosts should listen for `notifications/tools/list_changed` and call `tools/list` again to pick up new enum hints when configs change.
- No streaming over MCP. For realtime, use REST SSE at `/api/events`.
  - SSE sends an initial `retry: 1000` hint and periodic `:heartbeat` comments to keep the connection healthy.

## Configuration

- Uses the same tasks directory resolution. To target a specific repo:
```bash
export LOTAR_TASKS_DIR=/path/to/your/.tasks
lotar mcp
```
- `LOTAR_MCP_AUTORELOAD=0` keeps long-running hosts alive by preventing the auto-restart watcher from exiting the process when the binary changes.

## Troubleshooting

- Ensure each request is a complete JSON document on a single line; newline-framed payloads must end with `\n`. In `Content-Length` compatibility mode the header must be followed by a blank line and the body must be exactly the announced number of bytes.
- Operational requests rejected with `-32002` mean the handshake has not completed: send `initialize`, wait for the result, then send `notifications/initialized` before calling tools.
- The server exits (code 0) when the `lotar` binary changes so a supervising host can relaunch it. Set `LOTAR_MCP_AUTORELOAD=0` if your host already manages restarts.
- If you see "Method not found", verify the `method` matches one of the tools (names accept either `task/list` or `task_list`).
- For payload validation issues, inspect `error.data.details` for enum hints or schema messages.
