# MCP Tools Reference (Developer Notes)

For the user-facing/authoritative tool reference (parameters, payloads, examples), see [../help/mcp-tools.md](../help/mcp-tools.md).

This page focuses on where the tool surface is defined and how it is tested.

## Where things live

- Tool definitions/schemas: `crates/lotar-lib/src/mcp/server/tools.rs`
- Request dispatcher + MCP framing: `crates/lotar-lib/src/mcp/server.rs`
- Tool handlers: `crates/lotar-lib/src/mcp/server/handlers/*`
- Smoke coverage: `smoke/tests/mcp.*.smoke.spec.ts`

## Test entrypoints

- Unit coverage: `crates/lotar-lib/tests/mcp_server_unit_test.rs`
- End-to-end MCP smoke: `smoke/tests/mcp.*.smoke.spec.ts`

See also: [lotar mcp internals](./mcp.md)
