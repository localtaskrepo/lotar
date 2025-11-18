# SSE Events

`lotar serve` exposes a Server-Sent Events stream at `/api/events` (alias: `/api/tasks/stream`). It forwards everything emitted by the task/config event bus plus project-change signals from the filesystem watcher.


## Connect

```bash
curl -N "http://localhost:4183/api/events?kinds=task_created,task_updated"
```

The server responds with `text/event-stream`, sends an initial `retry: 1000` hint, and keeps the TCP connection open until the client disconnects. Heartbeat comments (`:heartbeat`) are emitted every ~15 seconds (2s when `LOTAR_TEST_FAST_IO=1`) so intermediaries know the stream is still alive.

## Query parameters

| Parameter | Details |
|-----------|---------|
| `debounce_ms` | Debounce window (default 100 ms). Left blank, the server falls back to `LOTAR_SSE_DEBOUNCE_MS`. Values below 20 ms are clamped when fast-IO mode is enabled. |
| `kinds` / `topic` | Comma-separated, case-insensitive list of kinds to keep. Valid kinds: `task_created`, `task_updated`, `task_deleted`, `config_updated`, `project_changed`. When omitted, all events flow through. `topic` is a legacy alias retained for compatibility. |
| `project` | Filter events to a specific project prefix. Task events match when the task ID (e.g., `TEST-42`) shares that prefix; filesystem events match on their `{ "name": "<PROJECT>" }` payload. |
| `ready` | `true`/`1` requests a one-time `ready` event. Only honored when `LOTAR_SSE_READY=1` is set on the server. |

Combine filters freely: e.g., subscribe to only `task_created` from project `ENG` with a 50 ms debounce.

## Event kinds & payloads

- `task_created` / `task_updated` — Full `TaskDTO` JSON. The payload always includes `id`, `title`, `status`, etc., and adds `triggered_by` when identity resolution succeeds.
- `task_deleted` — `{ "id": "<PROJECT-N>", "triggered_by"?: string }`.
- `config_updated` — `{ "triggered_by"?: string }`; emitted after `lotar config set`, REST config writes, or other config-mutating actions.
- `project_changed` — `{ "name": "<PROJECT>" }`; raised by the `.tasks` watcher whenever YAML files are created, modified, or removed under that project.
- `ready` — `{}`; only emitted when both `LOTAR_SSE_READY=1` and `ready=1` are in effect.

Every event is written as:

```
event: <kind>
data: <JSON payload>

```

## Attribution (`triggered_by`)

Handlers such as `task add`, `task update`, and config endpoints pass the resolved actor into `api_events::emit_*`. Identity is derived via the standard precedence (config `default.reporter` → explicit CLI/REST overrides → git user → system user). When resolution fails, `triggered_by` is omitted, so clients should treat it as optional.

## Debounce & delivery guarantees

The connection buffers events and flushes them after the configured debounce window or when the connection has been idle long enough for a heartbeat. This reduces chatty bursts (e.g., bulk edits) without dropping events: each buffered event is still delivered exactly once, and the buffer drains on timeout or disconnect. Use `debounce_ms=0` if you prefer immediate delivery.

## Filesystem watcher & snapshots

- The embedded watcher starts automatically when `lotar serve` finds a `.tasks` directory near the working directory. It is read-only and best-effort; failures merely disable project-change events.
- A `project_changed` event is emitted for each project that has a file created, modified, or removed under `.tasks/<PROJECT>/...`.
- Set `LOTAR_SSE_READY=1` and pass `ready=1` on the connection to receive a synthetic `ready` event. If you also include `project=<PREFIX>` and request `project_changed` in `kinds`, the server immediately emits a snapshot `{ "name": "<PREFIX>" }` (if that project directory already exists) to avoid watcher races. The snapshot is written inline and also forwarded through the normal event bus for other subscribers.

## Environment toggles

- `LOTAR_SSE_DEBOUNCE_MS` — Default debounce window when clients omit `debounce_ms`.
- `LOTAR_SSE_READY` — Enables the opt-in `ready`/snapshot behavior for test harnesses and automation.
- `LOTAR_TEST_FAST_IO` — Shortens debounce/heartbeat intervals; used by the integration suite to keep runs snappy.

## Related references

- [OpenAPI spec](../openapi.json) — `/api/events` parameters and response docs.
- [serve.md](serve.md) — broader server configuration (host/port/env vars).
