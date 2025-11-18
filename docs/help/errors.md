# Error Model

LoTaR surfaces consistent error envelopes across the REST API, CLI, and MCP stdio server so that automation can react the same way everywhere.


## REST API

Every REST response is either `{"data": ...}` (success) or `{"error": { ... }}`.

```json
HTTP/1.1 400 Bad Request
{
	"error": {
		"code": "INVALID_ARGUMENT",
		"message": "priority must exist in config",
		"details": {"field": "priority"}
	}
}
```

| HTTP status | `error.code` | When it is returned | Source |
| --- | --- | --- | --- |
| `400` | `INVALID_ARGUMENT` | Request validation failures: missing parameters, malformed JSON bodies, invalid filters, priority/type parsing errors, invalid task IDs, or git preconditions. |
| `404` | `NOT_FOUND` | Task or sprint lookups that miss (e.g., comment edits, sprint lifecycle updates). |
| `500` | `INTERNAL` | Anything that bubbles up from storage/config IO, YAML parsing, or service errors. The message mirrors the underlying error for easier debugging. |
| `500` | `SERIALIZE` | Only emitted when the server fails to encode the JSON payload. Indicates a bug; retrying usually succeeds once the underlying issue is fixed. |

`details` is optional; it appears when a handler adds structured context to the `internal(..)` payload. Conflicts (`409`) remain unused today—they are reserved for future optimistic concurrency features.

## CLI

- Commands exit with `0` on success and `1` on any error path. The CLI exits explicitly after logging the failure.
- Errors are rendered through the same output renderer used by the rest of the CLI:
	- Text mode writes `❌ message` to `stderr`.
	- `--format=json` still writes to `stderr`, but the payload becomes `{"status":"error","message":"..."}` so scripts can parse it. Standard output stays clean for successful JSON payloads.
- Warnings and validation hints also go to `stderr`, so automation should treat `stdout` as the canonical data channel.

## MCP (Model Context Protocol)

The MCP server implements JSON-RPC 2.0 and mirrors the spec’s error structure:

```json
{
	"jsonrpc": "2.0",
	"id": 42,
	"error": {
		"code": -32602,
		"message": "Missing id",
		"data": {"field": "id"}
	}
}
```

Standard JSON-RPC codes used by the MCP server:

| Code | Meaning |
| --- | --- |
| `-32700` | Parse error while reading the request stream.
| `-32601` | Method/tool not found.
| `-32602` | Invalid or missing params (used extensively across handlers).
| `-32603` | Internal error (resolver/storage failures, serialization issues).

Domain-specific codes introduced by the MCP handlers:

| Code | Meaning |
| --- | --- |
| `-32000` | Task creation failed (`TaskService::create`).
| `-32001` | Config read/show error.
| `-32002` | Config write/set failure (validation errors are surfaced through `error.data` warnings).
| `-32004` | Resource missing (tasks or sprints not found).
| `-32005` | Task update failed.
| `-32006` | Task delete failed.

Each handler populates `error.data.message` with the underlying `TaskService` or `SprintService` error string so the client can present the exact cause.

See also: [API Quick Reference](./api-quick-reference.md) for endpoint-specific success payloads.
