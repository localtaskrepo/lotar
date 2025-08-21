# SSE Events

Realtime events via Server-Sent Events at /api/events (alias: /api/tasks/stream).

## Kinds

## Payloads
 task_created/updated: TaskDTO as JSON with optional `triggered_by`
 task_deleted: { id: string, triggered_by?: string }
 config_updated: { triggered_by?: string }
 project_changed: { name: string }

## Filters

## Protocol
 Attribution
 - triggered_by: actor attribution for events (identity rules apply). Resolved via config default_reporter → git user → system user.

Planned
- triggered_by: include actor attribution for events (identity rules apply)

See also: [OpenAPI spec](../openapi.json).

## Filesystem watcher

When the server is running, a lightweight, best-effort filesystem watcher observes the nearest `.tasks` directory.

- On YAML file create/modify/remove under `.tasks/<PROJECT>/...`, the server emits `project_changed` with `{ name: <PROJECT> }`.
- No files are written; this is notify-only and read-only.
- Debounce: you can tune the debounce window via the environment variable `LOTAR_SSE_DEBOUNCE_MS` (default: 250ms).
- Ready event: set `LOTAR_SSE_READY=1` to emit a one-time `ready` event on new connections, useful for tests and clients.
- Snapshot on ready: when `ready=1` is sent in the query, if you also provide `project=<PREFIX>` and include `project_changed` in `kinds`, the server will immediately emit a `project_changed` event for that project (if it exists under `.tasks`). This helps clients avoid races with the filesystem watcher during startup.
