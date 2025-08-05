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

# Start with file watching enabled
lotar serve --watch

# Start in development mode
lotar serve --dev
```

## Options

- `--port <PORT>` - Port to bind server to (default: 8080)
- `--host <HOST>` - Host address to bind to (default: localhost)
- `--watch` - Enable automatic file watching and reload
- `--dev` - Development mode with enhanced debugging
- `--no-browser` - Don't automatically open browser
- `--timeout <SECONDS>` - Server shutdown timeout

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
