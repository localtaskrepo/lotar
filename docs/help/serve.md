# lotar serve

Launch the bundled HTTP server that serves the SPA from `target/web` and exposes the REST + SSE APIs used by the CLI.


## Usage

```bash
lotar serve [OPTIONS]
```

## Examples

```bash
# Start server; prefers the built-in default port 8080 and, when nothing
# configured a port anywhere, moves to an OS-assigned port if 8080 is busy
lotar serve

# Pin a specific port via the environment (never falls back)
LOTAR_PORT=3000 lotar serve

# Start on custom port
lotar serve --port=3000

# Start with specific host binding
lotar serve --host=0.0.0.0 --port=8080

# Open browser automatically
lotar serve --open

# Custom tasks directory
lotar serve --tasks-dir=/custom/path --port=8080

# Environment variable usage
export LOTAR_TASKS_DIR=/project/tasks
lotar serve  # Uses environment-configured directory

# Custom web UI
lotar serve --web-ui-path=/path/to/custom/ui

# Force embedded UI (useful for testing)
lotar serve --web-ui-embedded
```

## Options

- `-p <PORT>`, `--port <PORT>` - Port to bind server to. When the flag (or a bare positional port) is omitted, the port is resolved in the documented order: `--config server.port=...`, then `LOTAR_PORT`/`LOTAR_SERVER_PORT`, then the config file's `server.port` (`~/.lotar`, then `.tasks/config.yml`), then the built-in default `8080`; `--port` beats everything. Each layer's request is honored exactly — including a request for `8080` itself — and config-set ports are validated to 1–65535 by the existing `--config` validation, so an OS-assigned port via config uses the file spelling (`server.port: 0`) or `--port 0`. Pass `0` to bind an OS-assigned ephemeral port; the startup banner then reports the actual bound port. `serve` is the deliberate exception where `-p` means the port; for every other command — and before the `serve` subcommand itself (`lotar -p web serve`) — `-p` remains the short form of the global `--project` option.
- `--host <HOST>` - Host address to bind to (default: localhost)
- `--open` - Automatically open browser after starting server
- `--web-ui-path <PATH>` - Path to a directory containing custom web UI assets. When set and the directory exists, files are served from here first, falling back to the bundled UI if not found.
- `--web-ui-embedded` - Force serving only the embedded/bundled UI assets, ignoring any custom web UI path. Useful for CI testing to ensure the bundled UI works correctly.

Assets are served with compression: clients that send `Accept-Encoding: gzip` receive pre-compressed variants (~70% smaller transfer), and content-hashed asset files carry `Cache-Control: immutable` so repeat visits load from browser cache. The HTML entry points always revalidate (`no-cache`).
- `--format <FORMAT>` - Output format: text, table, json, markdown
- `--verbose` - Enable verbose output
- `--tasks-dir <PATH>` - Override tasks directory resolution

> Tip: `lotar serve` ignores the `--project/-p` global flag on purpose—project defaults are resolved dynamically per request inside the REST handlers—so passing `-p` before the command only changes the CLI project context, not the server port.

### Readiness and bind semantics

The `Host:`/`Port:`/`URL:` banner is printed only after a bind attempt succeeds, and `--port 0` advertisements always show the actual bound port. Automated clients can treat that banner as the readiness signal: when no attempted bind succeeds — a busy port that was explicitly configured (including a request for `8080` itself), or any other bind error such as permissions or an unknown host — the command prints the bind error, exits with a non-zero status, and never prints the banner. The single exception is the implicit default: when nothing configured a port and the built-in `8080` is busy, the server binds an OS-assigned port instead and the banner reports that actual port (see [Default-port fallback](#default-port-fallback) below). This is the contract the smoke harness relies on (see `smoke/helpers/server.ts`).

#### Default-port fallback

Exactly one case does not fail on a busy port: when **no** source configured a port — no `--port`/`-p`/positional, no `--config server.port=...`, no `LOTAR_PORT`/`LOTAR_SERVER_PORT`, and no `server.port` in the config files — the built-in default `8080` is preferred, and if it is already in use on the bind host the server binds an OS-assigned port on the same host instead. The command then prints a warning naming the preferred default and the actual port (`Default port 8080 is already in use on <host>; serving on port <N> instead`), the `Port:`/`URL:` banner reports the actual port, and `--open` uses it. Any explicitly configured port is strict: `--port 8080`, `--config server.port=8080`, `LOTAR_PORT=8080`, or a config file setting `server.port: 8080` all fail fast with the usual bind error instead of moving, because the requester asked for that exact port. Bind failures for other reasons (permissions, unknown host) never trigger the fallback. The `URL:` line always stays plain text for scripts; the `Port:` line is the only banner line that may be styled (bold on an interactive terminal, and bold yellow when the fallback moved the port; `NO_COLOR`, JSON output, and piped streams always stay plain).

## Environment Variables
- `LOTAR_PORT`, `LOTAR_SERVER_PORT` - Serve port when `--port` is omitted (`LOTAR_PORT` wins if both are set; `--config server.port` and `--port` outrank them). Locks the server to that exact port: no fallback when it is busy, and an invalid value aborts the command before binding. Resolved through the config precedence chain (see [precedence](precedence.md)).
- `LOTAR_TASKS_DIR` - Default tasks directory location
- `LOTAR_WEB_UI_PATH` - Path to custom web UI assets directory (same as `--web-ui-path`)
- `LOTAR_WEB_UI_EMBEDDED` - When set to `1`, force embedded UI only (same as `--web-ui-embedded`)
- `LOTAR_SSE_DEBOUNCE_MS` - Default debounce window for `/api/events` and `/api/tasks/stream` (overridden by the `debounce_ms` query parameter).
- `LOTAR_SSE_READY` / `LOTAR_TEST_FAST_IO` - Testing hooks that control synthetic readiness events and heartbeat cadence.

### Web Interface
- **Task Dashboard** - Overview panes powered by the `/api/tasks/list` endpoint.
- **Task Management** - Creation, editing, effort/status changes, and comments all call the same REST endpoints used by the CLI.
- **Project Views** - Per-project filters share logic with CLI list filters; preferences are stored client-side (see `docs/help/preferences.md`).
- **Boards** - Choose a project to open its board. Tasks whose statuses are not configured for that board remain visible in a presentation-only Other column, including assignee/priority/type swimlanes, with the same query membership and ordering as regular columns. Other paginates in the flat view; it is not a new workflow status or a drag/drop status target. Configured statuses named `Other` or `__other__` remain real workflow columns. Completion policy, saved Done-column visibility settings, and WIP limits remain separate.
- **Search & Filtering** - Advanced filtering mirrors `lotar list` options (`assignee`, `tags`, `status`, unified filters, etc.). The toolbar search box also accepts `key:value` tokens (`status:todo priority:high sprint:2 tag:ci "multi word"`); plain words stay free-text, unknown tokens pass through as custom field filters, and suggestions appear as you type (arrow keys + Enter or Tab to apply).
- **Insights** - Metrics components consume `/api/stats/*` and `/api/sprints/*` endpoints.
- **Personalization** - Preferences view interacts solely with browser storage; no server-side config is modified.

### API Endpoints
- `POST /api/tasks/add` - Create new task (body: TaskCreate; supports `@me` for people fields; auto-set reporter if enabled; accepts atomic initial `status`, `custom_fields` map, and `acceptance_criteria`, all validated against the target project's config)
- `GET /api/tasks/list` - List tasks
	- Query params:
		- `project` (prefix)
		- `status` (CSV; validated against config)
		- `priority` (CSV; validated against config)
		- `type` (CSV; validated against config)
		- `assignee` (supports `@me` to filter to current user)
		- `tags` (CSV)
		- `q` (free-text search)
		- `deletion=active|deleted|all` (default `active`; invalid/blank values return HTTP 400)
	- Notes:
		- Invalid values for `status`, `priority`, or `type` return HTTP 400
		- Any additional query key is treated as a property filter. Declared custom fields can be used directly (e.g., `?sprint=W35`). Multiple values allowed via CSV; matching is case- and separator-insensitive.
- `GET /api/tasks/get?id=...` - Get task by id (deleted tasks are hidden; opt in with `include_deleted=true`)
- `POST /api/tasks/update` - Update task (body: TaskUpdateRequest: flat fields with `id` + optional properties; supports `@me` for reporter/assignee; `status`/`priority`/`type` are validated against the task's project config, `null` clears clearable fields, and list/map patches replace the whole value)
- `POST /api/tasks/delete` - Soft-delete task (body: { id, hard?: bool }); hard deletion retains attachment blobs and returns warnings listing attachments and incoming task relationships
- `POST /api/tasks/restore` - Restore a soft-deleted task (body: { id }); delete/restore do not change `modified`
- List/export accept `deletion=active|deleted|all` (default `active`); deleted tasks retain their workflow status and are excluded from normal statistics
- `GET /api/projects/list` - List projects
- `GET /api/projects/stats?project=PREFIX` - Project stats
- `GET /api/whoami` - Resolve the identity that auto-populates reporter/assignee fields.
- Sprint endpoints (`/api/sprints/*`) expose creation, listing, metrics, and cleanup flows (see `docs/help/sprints.md` for the full matrix).

### Real-time Updates
- Server-Sent Events (SSE)
	- `GET /api/events` — stream of events: `task_created`, `task_updated`, `task_deleted`, `config_updated`, `sync_started`, `sync_progress`, `sync_completed`, `sync_failed`
	- Alias: `GET /api/tasks/stream`
	- Optional query params:
		- `debounce_ms` — debounce window in ms (default 100; env fallback `LOTAR_SSE_DEBOUNCE_MS`)
		- `kinds` — CSV list of event kinds to include
		- `project` — project prefix filter
	- Behavior & reliability:
		- Sends `retry: 1000` on connect to advise client reconnection delay
		- Emits `:heartbeat` comments periodically when idle to keep connections alive
		- A filesystem watcher monitors `.tasks/**` and emits `project_changed` events whenever YAML files are added/modified/removed, ensuring the UI refreshes even when tasks are edited outside the browser.
	- Testing aids: set `LOTAR_SSE_READY=1` and pass `?ready=1` to receive a one-time `ready` event when the connection is established (used by the smoke suite).

## Access URLs

Once started, the server provides:
- **Web Interface**: `http://<host>:<port>`
- **API Base**: `http://<host>:<port>/api`

## File Watching

- `notify` watches `.tasks` recursively when the server starts. Any create/modify/remove event under a project directory emits a `project_changed` SSE payload with `{ "name": "<PREFIX>" }` so browsers refresh caches or task lists.
- Debounce is handled client-side through the SSE `debounce_ms` parameter or the `LOTAR_SSE_DEBOUNCE_MS` fallback.

## Development Notes

- Request limits: HTTP bodies are capped at 16 MiB (`413` beyond) and idle sockets time out after 30 seconds; MCP stdio frames are capped at 10 MiB. Static file requests containing `.`/`..` path segments are rejected.
- Cross-process write safety: task and sprint writes take an exclusive advisory lock (`.task.lock` / `.sprints.lock` dot-files inside the affected directory) and are written atomically via temp-file + rename. Locks are descriptor-based, so they release automatically if a process crashes; leftover zero-byte lock files are inert and safe to gitignore. An interrupted atomic write can leave an orphaned `.<name>.tmp-*` dot-file behind; it is never swept automatically (another writer may still own it) and can be deleted manually once no writer is running.
- Transactional task/sprint mutations: task creation, task updates with sprint changes, sprint assignment/removal, and missing-sprint cleanup stage all of their file writes (task files, sprint files, auto-populated project config) and publish them under the coordinated locks (sprints lock first, then per-project task locks; the lock set also covers every project referenced by a pending journal, so recovery itself never touches unlocked files). A validation or write failure — including a failed directory sync or a failed journal unlink after a fully published batch — rolls every affected file back to its exact previous bytes before the error is returned, so an error means unchanged files and retries cannot orphan tasks or duplicate memberships. The single exception is fail-closed by design: if the automatic rollback itself cannot complete, the journal is retained, participating mutations refuse to proceed, and affected files may remain mixed until the next participating mutation finishes the rollback or the files are reconciled manually; retries stay duplicate-safe either way. A `.txn-pending.json` journal at the tasks root lets the next such mutation recover a crashed publish the same way; if the journal is corrupt or a file was changed by someone else in the meantime, participating mutations fail closed and ask for manual reconciliation instead of overwriting. While a journal is pending, other lock-taking writers (task comments, direct sprint edits, scan-driven creation) refuse to run with a pointer to the journal rather than risk changes a rollback would clobber; lock-less config edits remain outside this guard. Acknowledgement semantics: a committed transaction is acknowledged once its journal unlink succeeds — if that unlink cannot complete, the published writes are rolled back immediately under the still-held locks and the caller sees an error (a retained journal then matches the already-restored state, and recovery merely re-verifies and retries its removal), so a retry re-applies instead of duplicating; an error is never returned together with a removed journal. Limits: recovery runs on the next participating mutation rather than at startup, readers may observe intermediate states (atomic per file, never torn), plain config edits made outside these commands are not lock-coordinated, rollback restores files but not directories or lock files, and NO power-loss durability is claimed on any platform: the Unix fsyncs are best-effort hardening, not a proven durable-commit protocol, and after an acknowledged commit a power failure can in principle resurrect the unlinked journal entry, which recovery would then roll back.
- The API is same-origin only: no `Access-Control-Allow-Origin` header is emitted, and mutating requests (`POST`/`PUT`/`PATCH`/`DELETE`) carrying an `Origin` that does not match the server's `Host` are rejected with `403 Forbidden`. Non-browser clients (CLI, scripts, MCP) send no `Origin` and are unaffected.
- Preflight: `OPTIONS /api/*` returns `204 No Content` with headers:
	- `Access-Control-Allow-Methods: GET,POST,OPTIONS`
	- `Access-Control-Allow-Headers: Content-Type`
- Static files are served with the following priority:
	1. **Custom UI path** (if `--web-ui-path` or `LOTAR_WEB_UI_PATH` is set and the directory exists)
	2. **Embedded assets** (bundled at compile time via `include_dir!`)
	3. **Filesystem fallback** (`target/web/`, only when no custom path is configured)
- Use `--web-ui-embedded` or `LOTAR_WEB_UI_EMBEDDED=1` to skip the custom path and only serve embedded assets.

### Custom Web UI

You can serve a custom or development UI by pointing to a directory containing web assets:

```bash
# Serve from a local development build
lotar serve --web-ui-path=./my-custom-ui/dist

# Or set in your global config (~/.lotar/config.yml or .tasks/config.yml)
web_ui_path: /path/to/custom/ui
```

The custom UI path takes precedence over embedded assets. If a requested file is not found in the custom path, the server falls back to the bundled UI. This allows partial overrides or testing new UI builds without recompiling the Rust binary.

## Notes

- Server runs until interrupted (Ctrl+C)
- Web interface works with all modern browsers
- API handlers expect JSON bodies and respond with JSON envelopes that mirror the CLI output (`{status,message,data}`).
- Use `--host=0.0.0.0` to allow external connections
