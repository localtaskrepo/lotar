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
- `GET /api/tasks` - List all tasks
- `POST /api/tasks` - Create new task
- `GET /api/tasks/{id}` - Get specific task
- `PUT /api/tasks/{id}` - Update task
- `DELETE /api/tasks/{id}` - Delete task
- `GET /api/projects` - List projects
- `GET /api/stats` - Get statistics

### Real-time Updates
- WebSocket support for live updates
- Automatic refresh when files change (with `--watch`)
- Cross-browser synchronization

## Access URLs

Once started, the server provides:
- **Web Interface**: `http://localhost:8080`
- **API Documentation**: `http://localhost:8080/api/docs`
- **Health Check**: `http://localhost:8080/health`

## File Watching

When `--watch` is enabled:
- Monitors task files for changes
- Automatically reloads data when files are modified
- Broadcasts updates to connected clients
- Detects new projects and tasks

## Development Mode

With `--dev` flag:
- Enhanced error messages
- Request/response logging
- Auto-reload on code changes
- Development-friendly CORS settings

## Notes

- Server runs until interrupted (Ctrl+C)
- Web interface works with all modern browsers
- API supports both JSON and form data
- File changes are reflected immediately with `--watch`
- Use `--host=0.0.0.0` to allow external connections
