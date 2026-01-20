# lotar serve

Launch the bundled HTTP server that serves the SPA from `target/web` and exposes the REST + SSE APIs used by the CLI.


## Usage

```bash
lotar serve [OPTIONS]
```

## Examples

```bash
# Start server on default port (8080)
lotar serve

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

- `--port <PORT>` - Port to bind server to (default: 8080). The short `-p` flag is reserved for the global `--project` option, so always use the long form when setting the port.
- `--host <HOST>` - Host address to bind to (default: localhost)
- `--open` - Automatically open browser after starting server
- `--web-ui-path <PATH>` - Path to a directory containing custom web UI assets. When set and the directory exists, files are served from here first, falling back to the bundled UI if not found.
- `--web-ui-embedded` - Force serving only the embedded/bundled UI assets, ignoring any custom web UI path. Useful for CI testing to ensure the bundled UI works correctly.
- `--format <FORMAT>` - Output format: text, table, json, markdown
- `--verbose` - Enable verbose output
- `--tasks-dir <PATH>` - Override tasks directory resolution

> Tip: `lotar serve` ignores the `--project/-p` global flag on purpose—project defaults are resolved dynamically per request inside the REST handlers—so passing `-p` before the command only changes the CLI project context, not the server port.

## Environment Variables
- `LOTAR_TASKS_DIR` - Default tasks directory location
- `LOTAR_WEB_UI_PATH` - Path to custom web UI assets directory (same as `--web-ui-path`)
- `LOTAR_WEB_UI_EMBEDDED` - When set to `1`, force embedded UI only (same as `--web-ui-embedded`)
- `LOTAR_SSE_DEBOUNCE_MS` - Default debounce window for `/api/events` and `/api/tasks/stream` (overridden by the `debounce_ms` query parameter).
- `LOTAR_SSE_READY` / `LOTAR_TEST_FAST_IO` - Testing hooks that control synthetic readiness events and heartbeat cadence.

### Web Interface
- **Task Dashboard** - Overview panes powered by the `/api/tasks/list` endpoint.
- **Task Management** - Creation, editing, effort/status changes, and comments all call the same REST endpoints used by the CLI.
- **Project Views** - Per-project filters share logic with CLI list filters; preferences are stored client-side (see `docs/help/preferences.md`).
- **Search & Filtering** - Advanced filtering mirrors `lotar list` options (`assignee`, `tags`, `status`, unified filters, etc.).
- **Insights** - Metrics components consume `/api/stats/*` and `/api/sprints/*` endpoints.
- **Personalization** - Preferences view interacts solely with browser storage; no server-side config is modified.

### API Endpoints
- `POST /api/tasks/add` - Create new task (body: TaskCreate; supports `@me` for people fields; auto-set reporter if enabled)
- `GET /api/tasks/list` - List tasks
	- Query params:
		- `project` (prefix)
		- `status` (CSV; validated against config)
		- `priority` (CSV; validated against config)
		- `type` (CSV; validated against config)
		- `assignee` (supports `@me` to filter to current user)
		- `tags` (CSV)
		- `q` (free-text search)
	- Notes:
		- Invalid values for `status`, `priority`, or `type` return HTTP 400
		- Any additional query key is treated as a property filter. Declared custom fields can be used directly (e.g., `?sprint=W35`). Multiple values allowed via CSV; matching is case- and separator-insensitive.
- `GET /api/tasks/get?id=...` - Get task by id (returns HTTP 404 if not found)
- `POST /api/tasks/update` - Update task (body: TaskUpdateRequest: flat fields with `id` + optional properties; supports `@me` for reporter/assignee)
- `POST /api/tasks/delete` - Delete task (body: { id })
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

- CORS is permissive by default for local development
- Preflight: `OPTIONS /api/*` returns `204 No Content` with headers:
	- `Access-Control-Allow-Origin: *`
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
- API handlers expect JSON bodies and respond with JSON envelopes that mirror the CLI output (`{status,message,data}`); CORS headers are always added for local development.
- Use `--host=0.0.0.0` to allow external connections
