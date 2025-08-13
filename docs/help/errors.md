# Error Model

Standard errors across CLI, REST, and MCP.

## REST
- 400 Bad Request: { error: { code: "INVALID_ARGUMENT", message, details? } }
- 404 Not Found: { error: { code: "NOT_FOUND", message } }
- 500 Internal Server Error: { error: { code: "INTERNAL", message } }

## CLI
- Non-zero exit codes; human-readable messages; `--format=json` may include structured fields.

## MCP
- JSON-RPC errors: { code, message, data? }

Notes
- Validation errors map to INVALID_ARGUMENT (400).
- Missing resources map to NOT_FOUND (404).
- Conflicts (409) are reserved for future use (e.g., edit races).

See also: [API Quick Reference](./api-quick-reference.md).
