# SSE Events

`lotar serve` exposes a Server-Sent Events stream at `/api/events` (alias: `/api/tasks/stream`). It forwards everything emitted by the task/config event bus plus project-change signals from the filesystem watcher.


## Connect

```bash
curl -N "http://localhost:8080/api/events?kinds=task_created,task_updated"
```

The server responds with `text/event-stream`, sends an initial `retry: 1000` hint, and keeps the TCP connection open until the client disconnects. Heartbeat comments (`:heartbeat`) are emitted every ~15 seconds (2s when `LOTAR_TEST_FAST_IO=1`) so intermediaries know the stream is still alive.

## Query parameters

| Parameter | Details |
|-----------|---------|
| `debounce_ms` | Debounce window (default 100 ms). Left blank, the server falls back to `LOTAR_SSE_DEBOUNCE_MS`. With `LOTAR_TEST_FAST_IO=1`, values above 20 ms are lowered to 20 ms. |
| `kinds` / `topic` | Comma-separated, case-insensitive list of kinds to keep. Valid kinds: `task_created`, `task_updated`, `task_deleted`, `task_error`, `task_order_changed`, `sprint_changed`, `release_changed`, `automation_failed`, `automation_skipped`, `config_updated`, `project_changed`, `sync_started`, `sync_progress`, `sync_completed`, `sync_failed`, `agent_job_started`, `agent_job_progress`, `agent_job_message`, `agent_job_input`, `agent_job_completed`, `agent_job_failed`, `agent_job_cancelled`. When omitted, all events flow through. `topic` is a legacy alias retained for compatibility. |
| `project` | Filter events to a specific project prefix. Task events match when the task ID (e.g., `TEST-42`) shares that prefix; `project_changed` matches its `name`; sync events match their top-level `project` (`null` never matches); `agent_job_*` events match their `ticket_id`; `config_updated` passes when it names that project or no project (workspace-wide config); `task_order_changed`, `sprint_changed` and `release_changed` always pass, since sprints and releases span projects. |
| `ready` | `true`/`1` requests a one-time `ready` event. Only honored when `LOTAR_SSE_READY=1` is set on the server. |

Combine filters freely: e.g., subscribe to only `task_created` from project `ENG` with a 50 ms debounce.

## Event kinds & payloads

- `task_created` / `task_updated` — Two shapes. Changes made through this server carry the full `TaskDTO` (with `id`, `title`, `status`, ..., plus `triggered_by` when identity resolution succeeds). Changes the file watcher sees (CLI in another process, an editor, `git pull`, sprint membership changes) carry only `{ "id": "<PROJECT-N>" }`; fetch the task if you need its fields. The watcher does not repeat changes this server already announced.
- `task_deleted` — `{ "id": "<PROJECT-N>", "moved_to"?: "<NEW-ID>", "triggered_by"?: string }`; `moved_to` is set when the task moved to another project.
- `task_error` — `{ "id": "<PROJECT-N>", "message": "..." }`; a task file changed but no longer parses. A `task_updated` with the same id follows so views refresh.
- `automation_failed` — `{ "id": "<PROJECT-N>", "rule": "<rule name>", "message": "..." }`; an automation rule action failed (later rules still ran).
- `automation_skipped` — `{ "id": "<PROJECT-N>", "rule": "<rule name>", "message": "..." }`; a repository rule's `run` command was skipped because the repository isn't [trusted](./trust.md) (or changed since); the rule's other actions applied, and the ticket gets a comment saying so.
- `task_order_changed` — `{ "sprint_id": <N>|null, "tasks": ["<PROJECT-N>", ...] }`; the backlog (`null`) or a sprint was reordered or changed membership.
- `sprint_changed` — `{ "id": <N>, "deleted": true|false }`; a sprint file was created, edited or deleted. Tasks that joined or left the sprint also get an id-only `task_updated`.
- `release_changed` — `{ "id": <N>, "deleted": true|false }`; a [release](./releases.md) file was created, edited or deleted. Tickets joining or leaving a release change their own files, so they come as `task_updated`.
- `config_updated` — `{ "project"?: "<PREFIX>", "triggered_by"?: string }`; emitted after REST config writes, and by the watcher when `config.yml` or `automation.yml` changes (workspace-wide without `project`, or in a project folder with it), so `lotar config set` from another terminal is seen too.
- `project_changed` — `{ "name": "<PROJECT>" }`; raised by the watcher once per batch of changes inside a project folder. Root files, `@` folders and dot-files (locks, temp files, transaction journals) never produce it.
- `sync_started` — `{ "run_id": "sync-...", "direction": "pull|push", "provider": "jira|github", "remote": "<remote>", "project": "<PREFIX>", "dry_run": true|false, "started_at": "..." }`.
- `sync_progress` — `{ "run_id": "sync-...", "summary": {"created":0,"updated":0,"skipped":0,"failed":0}, "entry": {"status":"created|updated|skipped|failed", "task_id": "PREFIX-1", "reference": "owner/repo#123", "message": "...", "at": "..."}, "project": "<PREFIX>", "remote": "<remote>", "direction": "pull|push" }`.
- `sync_completed` — `{ "run_id": "sync-...", "project": "<PREFIX>"|null, "report": { "id": "...", "summary": {"created":0,"updated":0,"skipped":0,"failed":0}, "stored_path": "sync-...json", ... }, "finished_at": "..." }`. `project` mirrors the report's resolved execution project (runs without an explicit project fall back to the configured default project) and is `null` when none resolves; `null` never matches a `project` filter.
- `sync_failed` — `{ "run_id": "sync-...", "error": "...", "finished_at": "...", "project": "<PREFIX>", "remote": "<remote>", "direction": "pull|push" }`.
- `agent_job_started` — `{ "id": "job-...", "ticket_id": "<PROJECT-N>", "runner": "copilot|codex|claude|gemini|command", "status": "running", "created_at": "...", "started_at": "...", "sequence": 1, "at": "..." }`.
- `agent_job_progress` — `{ "id": "job-...", "ticket_id": "<PROJECT-N>", "status": "running", "message": "...", "sequence": 2, "at": "..." }`.
- `agent_job_message` — `{ "id": "job-...", "ticket_id": "<PROJECT-N>", "status": "running", "message": "...", "sequence": 3, "at": "..." }`.
- `agent_job_input` — `{ "id": "job-...", "ticket_id": "<PROJECT-N>", "status": "running", "message": "...", "sequence": 4, "at": "..." }`.
- `agent_job_completed` — `{ "id": "job-...", "ticket_id": "<PROJECT-N>", "status": "completed", "finished_at": "...", "exit_code": 0, "sequence": 7, "at": "..." }`.
- `agent_job_failed` — `{ "id": "job-...", "ticket_id": "<PROJECT-N>", "status": "failed", "finished_at": "...", "exit_code": 1, "sequence": 7, "at": "..." }`.
- `agent_job_cancelled` — `{ "id": "job-...", "ticket_id": "<PROJECT-N>", "status": "cancelled", "finished_at": "...", "sequence": 5, "at": "..." }`.

Every `agent_job_*` SSE event carries `sequence` and `at` taken from the exact
same log entry served by `GET /api/jobs/logs`: `sequence` is the stable per-job
identity (positive, starts at 1, strictly monotonic for the job's lifetime, and
never reused after in-memory log truncation), so a client can join its live SSE
cache with a REST snapshot losslessly by `(id, sequence)`. The push-only kinds
`agent_job_raw`, `agent_job_init`, and `agent_job_result` consume sequence
numbers and appear in the REST log, but are never streamed over SSE.
- `ready` — `{}`; only emitted when both `LOTAR_SSE_READY=1` and `ready=1` are in effect.

Every event is written as:

```
event: <kind>
data: <JSON payload>

```

## Attribution (`triggered_by`)

Handlers such as `task add`, `task update`, and config endpoints pass the resolved actor into `api_events::emit_*`. Identity is derived via the standard precedence (config `default.reporter` → explicit CLI/REST overrides → git user → system user). When resolution fails, `triggered_by` is omitted, so clients should treat it as optional.

## Debounce & delivery guarantees

The connection buffers events and flushes them at most `debounce_ms` after the first buffered event, however many events follow, so a steady stream is still delivered every window. Repeated `task_updated` events for the same task and `project_changed` events for the same project within one window are merged (an id-only event never replaces a full DTO that is waiting); other events are delivered once each, in order. A buffer that reaches 500 events is flushed at once. Use `debounce_ms=0` if you prefer immediate delivery.

## Filesystem watcher & snapshots

- The embedded watcher starts automatically when `lotar serve` finds a `.tasks` directory near the working directory. It is read-only and best-effort; failures merely disable project-change events.
- The watcher collects changes for a short moment (up to 250 ms) and then announces each changed task, project, sprint and config file once. A file whose content did not change since it was last announced is skipped.
- Set `LOTAR_SSE_READY=1` and pass `ready=1` on the connection to receive a synthetic `ready` event. If you also include `project=<PREFIX>` and request `project_changed` in `kinds`, the server immediately emits a snapshot `{ "name": "<PREFIX>" }` (if that project directory already exists) to avoid watcher races. The snapshot is written inline and also forwarded through the normal event bus for other subscribers.

## Environment toggles

- `LOTAR_SSE_DEBOUNCE_MS` — Default debounce window when clients omit `debounce_ms`.
- `LOTAR_SSE_READY` — Enables the opt-in `ready`/snapshot behavior for test harnesses and automation.
- `LOTAR_TEST_FAST_IO` — Shortens debounce/heartbeat intervals; used by the integration suite to keep runs snappy.

## Related references

- [OpenAPI spec](../openapi.json) — `/api/events` parameters and response docs.
- [serve.md](serve.md) — broader server configuration (host/port/env vars).
