# lotar mcp

Run the MCP (Model Context Protocol) JSON-RPC server over stdio, exposing tools for tasks, projects, and config.

## Usage

```bash
lotar mcp
```

The server reads JSON-RPC 2.0 requests from stdin and writes responses to stdout.
It supports two framings:
- Content-Length framed messages (LSP-style), required by many hosts (VS Code, MCP)
- Line-delimited JSON, useful for simple shell testing

## Tools (Methods)

- task/create(params: TaskCreate) -> { task }
- task/get(params: { id: string, project?: string }) -> { task }
- task/update(params: { id: string, patch: TaskUpdate }) -> { task }
- task/delete(params: { id: string, project?: string }) -> { deleted: bool }
- task/list(params: TaskListFilter) -> { tasks: TaskDTO[] }
- project/list(params: {}) -> { projects: ProjectDTO[] }
- project/stats(params: { name: string }) -> { stats: ProjectStatsDTO }
- config/show(params: { global?: boolean, project?: string }) -> { config: object }
- config/set(params: { global?: boolean, project?: string, values: Record<string,string> }) -> { updated: boolean }

Host discovery/handshake methods:
- initialize() -> { protocolVersion, capabilities: { tools: { list, call } } }
- tools/list -> { tools: [{ name, description }] }
- tools/call (params: { name, arguments }) -> result of the named tool

Notes
- All payloads use snake_case keys.
- Enum fields follow the same values as the REST API and CLI.
- Errors are JSON-RPC errors with code/message and optional data.

## Example (shell)

```bash
# Start MCP server
lotar mcp &
# Send one request (using jq for pretty output)
printf '{"jsonrpc":"2.0","id":1,"method":"task/create","params":{"title":"Test via MCP","project":"DEMO","priority":"High","tags":[]}}\n' \
  | lotar mcp | jq .
```

## Integrations

- Generic JSON-RPC clients: connect via stdio; prefer Content-Length framing. For simple usage, one JSON object per line also works.
- AI tools (local agents/Copilot Chat): configure a custom tool provider that spawns `lotar mcp` and exchanges JSON-RPC messages. Map tool names 1:1 to the methods above.
- No streaming over MCP. For realtime, use REST SSE at `/api/events`.
  - SSE sends an initial `retry: 1000` hint and periodic `:heartbeat` comments to keep the connection healthy.

## Configuration

- Uses the same tasks directory resolution. To target a specific repo:
```bash
export LOTAR_TASKS_DIR=/path/to/your/.tasks
lotar mcp
```

## Troubleshooting

- Ensure each request is a complete JSON line (newline-terminated).
- If you see "Method not found", verify the `method` name matches one of the tools.
- For payload validation issues, check error `data.details` for parsing hints.
