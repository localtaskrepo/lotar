# lotar mcp

Start the MCP (Model Context Protocol) JSON-RPC server. The process reads from stdin, writes to stdout, and exposes the same task/project/config primitives that power the CLI. Tool definitions auto-refresh when `.tasks/` metadata changes thanks to the built-in watcher.


## Usage

```bash
lotar mcp
```

Environment:
- `LOTAR_TASKS_DIR` — override the workspace search path.
- `LOTAR_MCP_AUTORELOAD=0` — disable the default “restart when the binary changes” watchdog (useful when running under a supervisor that already restarts the process).

Transport:
- JSON-RPC 2.0 over stdio. Requests read from stdin, responses emitted to stdout.
- Two framing modes are supported:
  - `Content-Length: <n>\r\n\r\n<body>` (LSP/MCP-default). Required by VS Code, Cursor, and most AI hosts.
  - Single-line JSON (one object per line) for quick shell tests.
- All log output goes to stderr; stdout remains pure JSON.

## Tool Surface

The server registers these tools (see [MCP Tools Reference](./mcp-tools.md) for full schemas):

| Tool | Purpose |
|------|---------|
| `whoami` | Resolve the identity used for `@me` (with optional explain output). |
| `task_create` | Create a task with optional type/priority/status overrides and custom fields. |
| `task_get` | Fetch a task by id (project inferred or provided). |
| `task_update` | Patch mutable fields, relationships, and custom fields. |
| `task_comment_add` | Append a comment to a task. |
| `task_comment_update` | Update an existing comment on a task. |
| `task_bulk_update` | Patch multiple tasks in one call. |
| `task_bulk_comment_add` | Add the same comment to multiple tasks. |
| `task_bulk_reference_add` | Add the same reference to multiple tasks. |
| `task_bulk_reference_remove` | Remove the same reference from multiple tasks. |
| `task_delete` | Delete by id/project, returning `deleted=true/false`. |
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
| `schema_discover` | Return the current tool definitions + enum hints (optionally filtered by name). |

Host discovery/handshake methods include:
- initialize() -> { protocolVersion, capabilities: { tools: { list, call } } }
- tools/list -> { tools: [{ name, description }] }
- schema/discover(params?: { tool?: string }) -> same payload as tools/list but filtered
- tools/call (params: { name, arguments }) -> result of the named tool
- logging/setLevel(params: { level }) -> acknowledgement after setting tracing level

Notifications:
- `tools/listChanged` is pushed whenever `.tasks/` metadata or config changes alter enum hints (`hintCategories` identifies which values changed). Hosts should call `tools/list` again when they receive this event.

Notes
- All payloads use snake_case keys.
- Enum fields follow the same values as the REST API and CLI.
- Errors are JSON-RPC errors with code/message and optional data.
- People fields (reporter, assignee) accept the special value `@me`. It resolves to the current user based on merged config default_reporter → git user → system username; identical logic is used inside the CLI and REST server.

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

## Example (shell)

```bash
# Start the server (Ctrl+C to stop when finished)
lotar mcp &

# Send one newline-delimited request and pretty-print the embedded JSON payload
printf '{"jsonrpc":"2.0","id":1,"method":"task/list","params":{"project":"AUTH","limit":5}}\n' |
  lotar mcp |
  jq '.result.content[0].text | fromjson'
```

For hosts that insist on framed messages, prefix the payload with a `Content-Length` header and a blank line:

```bash
printf 'Content-Length: 109\r\n\r\n{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' |
  lotar mcp
```

## Integrations

- Generic JSON-RPC clients: connect via stdio; prefer Content-Length framing. For simple usage, one JSON object per line also works.
- AI tools (local agents/Copilot Chat): configure a custom tool provider that spawns `lotar mcp` and exchanges JSON-RPC messages. Map tool names 1:1 to the methods above.
- Long-running hosts should listen for `tools/listChanged` notifications and call `tools/list` again to pick up new enum hints when configs change.
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

- Ensure each request is a complete JSON document. For Content-Length framing the header must be followed by a blank line before the body; for newline framing end the payload with `\n`.
- The server restarts automatically when the `lotar` binary changes. Set `LOTAR_MCP_AUTORELOAD=0` if your host already manages restarts.
- If you see "Method not found", verify the `method` matches one of the tools (names accept either `task/list` or `task_list`).
- For payload validation issues, inspect `error.data.details` for enum hints or schema messages.
