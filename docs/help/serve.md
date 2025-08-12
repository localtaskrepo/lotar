# lotar serve

Start the web interface server for task management.

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
```

## Options

- `--port <PORT>` - Port to bind server to (default: 8080)
- `--host <HOST>` - Host address to bind to (default: localhost)
- `--open` - Automatically open browser after starting server

## Global Options

- `--format <FORMAT>` - Output format: text, table, json, markdown
- `--verbose` - Enable verbose output
- `--tasks-dir <PATH>` - Custom tasks directory (overrides environment/config)

## Environment Variables

- `LOTAR_TASKS_DIR` - Default tasks directory location

## Features

### Web Interface
- **Task Dashboard** - Overview of all tasks and projects
- **Task Management** - Create, edit, and update tasks
- **Project Views** - Project-specific task organization
- **Search & Filtering** - Advanced task filtering and search
- **Statistics** - Task completion and project metrics

### API Endpoints
- `POST /api/tasks/add` - Create new task (body: TaskCreate)
- `GET /api/tasks/list` - List tasks
	- Query params:
		- `project` (prefix)
		- `status` (CSV; validated against config)
		- `priority` (CSV; validated against config)
		- `type` (CSV; validated against config)
		- `tags` (CSV)
		- `category`
		- `q` (free-text search)
	- Notes: Invalid values for `status`, `priority`, or `type` return HTTP 400
- `GET /api/tasks/get?id=...` - Get task by id (returns HTTP 404 if not found)
- `POST /api/tasks/update` - Update task (body: { id, patch })
- `POST /api/tasks/delete` - Delete task (body: { id })
- `GET /api/projects/list` - List projects
- `GET /api/projects/stats?project=PREFIX` - Project stats

### Real-time Updates
- Server-Sent Events (SSE)
	- `GET /api/events` — stream of events: `task_created`, `task_updated`, `task_deleted`, `config_updated`
	- Alias: `GET /api/tasks/stream`
	- Optional query params:
		- `debounce_ms` — debounce window in ms (default 100; env fallback `LOTAR_SSE_DEBOUNCE_MS`)
		- `kinds` — CSV list of event kinds to include
		- `project` — project prefix filter
	- Behavior & reliability:
		- Sends `retry: 1000` on connect to advise client reconnection delay
		- Emits `:heartbeat` comments periodically when idle to keep connections alive

## Access URLs

Once started, the server provides:
- **Web Interface**: `http://localhost:8080`
- **API Base**: `http://localhost:8080/api`

## File Watching (planned)

- Filesystem watcher integration will broadcast updates to connected clients
- Debounce defaults to ~100ms; configuration TBD (env + CLI)

## Development Notes

- CORS is permissive by default for local development
- Preflight: `OPTIONS /api/*` returns `204 No Content` with headers:
	- `Access-Control-Allow-Origin: *`
	- `Access-Control-Allow-Methods: GET,POST,OPTIONS`
	- `Access-Control-Allow-Headers: Content-Type`
- Static files are served from `target/web`

## Notes

- Server runs until interrupted (Ctrl+C)
- Web interface works with all modern browsers
- API supports both JSON and form data
- File changes are reflected immediately with `--watch`
- Use `--host=0.0.0.0` to allow external connections
