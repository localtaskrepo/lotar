# SSE Events

Realtime events via Server-Sent Events at /api/events (alias: /api/tasks/stream).

## Kinds

## Payloads
 task_created/updated: TaskDTO as JSON with optional `triggered_by`
 task_deleted: { id: string, triggered_by?: string }
 config_updated: { triggered_by?: string }

## Filters

## Protocol
 Attribution
 - triggered_by: actor attribution for events (identity rules apply). Resolved via config default_reporter → git user → system user.

Planned
- triggered_by: include actor attribution for events (identity rules apply)

See also: [OpenAPI spec](../openapi.json).
