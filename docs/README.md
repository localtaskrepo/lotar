# LoTaR Documentation

## Project Status - August 2025

LoTaR is an actively developed task management tool with a CLI, web server, and source code integration. It includes a comprehensive test suite and aims for stable, predictable behavior.

## Quick Start

```bash
# Install and build
git clone https://github.com/mallox/lotar
cd lotar
cargo build --release

# Create your first task
lotar add "Setup project" --project=myapp --priority=HIGH

# List tasks
lotar list --project=myapp

# Start web interface
lotar serve 8080

# Scan source code for TODOs
lotar scan ./src
```

## Core Features

### Task Management
- **Full CRUD Operations**: Create, read, update, delete via CLI
- **Status System**: TODO, IN_PROGRESS, VERIFY, BLOCKED, DONE
- **Priority Levels**: LOW, MEDIUM, HIGH, CRITICAL
- **Task Types**: Feature, Bug, Epic, Spike, Chore
- **Formatted IDs**: PROJ-1, PROJ-2 format with 4-character prefixes

### Storage & Organization
- **YAML Format**: Human-readable `.yml` files
- **Project Isolation**: Each project gets its own directory
- **Git-friendly**: All files are version-controllable
- **Direct File Operations**: Fast task filtering without indexing overhead
- **Read-only Git Analytics**: Stats derived from repo history (no git writes)

### Source Code Integration
- 25+ languages supported
- TODO comment detection with optional ticket key extraction
- Multiple comment styles: //, #, --, ;, %, /* */ and <!-- -->
- Recursive directory scanning with .lotarignore and .gitignore support
- When creating a task from a TODO, LoTaR writes back the task key into the comment and stores a minimal code anchor under `references` in the task (no code snippets are stored). If a TODO already has a key, LoTaR ensures a code anchor exists and prunes older anchors for the same file, keeping only the latest line for that file. Use `lotar scan --reanchor` to prune cross-file anchors and keep only the newest anchor. On subsequent scans, existing anchors are automatically re-anchored when code moves (nearby-window search) and are updated across simple git renames using `git status` information.

### Web Interface & API
- **Built-in Web Server** with embedded React frontend
- **REST API** for all task operations
    - Multi-value filters for list: `status`, `priority`, `type`, `tags`
    - Proper 404 for unknown resources
    - CORS preflight support for `/api/*`
- **Configurable ports** (default 8080)
 - **SSE**: realtime events with `retry` hint and periodic heartbeats

### Server flags and endpoints

- `lotar serve --host 0.0.0.0 --port 8080 --open`
    - `--host` controls the bind address (default: 127.0.0.1). Use `0.0.0.0` to listen on all interfaces.
    - `--open` opens the default browser to the server URL, but does not change bind address.
- Shutdown endpoint: `GET /shutdown` cleanly stops the server. For tests, `/__test/stop` remains available as an alias.

## Command Reference

```bash
# Task Management
lotar add "Title" --type=feature --priority=HIGH
lotar list --project=backend --status=IN_PROGRESS
lotar list --search="keyword" --priority=HIGH
lotar status PROJ-001 DONE
lotar assignee PROJ-001 user@example.com

# System Commands
lotar serve 8080          # Start web server
lotar scan ./src          # Scan for TODOs
lotar stats changed --since 14d  # Tickets changed in a window
lotar stats churn --since 30d    # Churn: commits per ticket (sorted)
lotar stats authors --since 90d  # Top authors by commits touching tasks
lotar stats activity --since 60d --group-by day  # Activity by day (author|week|project also)
lotar config set key val  # Configuration

# Task history (read-only)
lotar task history PROJ-1
lotar task diff PROJ-1 --commit <sha>
lotar task at PROJ-1 <sha>
```

## File Structure

```
.tasks/
├── config.yml           # Global configuration
├── PROJECT-A/           # Project directory
│   ├── config.yml       # Project-specific config (optional)
│   ├── 1.yml           # Task files
│   └── 2.yml
└── PROJECT-B/
    ├── config.yml       # Project configuration
    └── 1.yml
```

## Testing & Quality

- **Comprehensive test suite** across all components (see [CI status](https://github.com/localtaskrepo/lotar/actions/workflows/test.yml))
- **Handler Unit Tests**: CLI command handlers
- **CLI Integration**: End-to-end workflows
- **Experimental CLI**: Real command execution
- **Storage Systems**: CRUD operations and validation
- **Configuration**: Config management and templates
- **Project Management**: Smart project resolution
- **Performance Tests**: Operation timing
- **Home Config**: User directory handling
- **And many more**: Comprehensive coverage across all components

**Quality Metrics:**
- Zero compilation warnings
- 100% test pass rate
- Memory-safe with Rust ownership
- Sub-100ms operation performance
- Production-ready code quality

## Development

```bash
cargo build           # Debug build
cargo build --release # Release build
cargo test            # Run all tests
cargo clippy          # Linting
cargo fmt             # Formatting
```

Ready for production use with no known critical issues.
