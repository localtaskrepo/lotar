# LoTaR Documentation

## Project Status - August 2025

LoTaR is an actively developed task management tool with a CLI, web server, and source code integration. It includes a comprehensive test suite and aims for stable, predictable behavior.

## Quick Start

```bash
# Install and build
git clone https://github.com/mallox/lotar
cd lotar

# Build web UI (Vue + Vite) into target/web and the Rust binary
npm install
npm run build

# Create your first task
lotar add "Setup project" --project=myapp --priority=HIGH

# List tasks
lotar list --project=myapp

# Start web interface (serves static assets from target/web)
lotar serve -p 8080

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

### Sprint Management
- **Lifecycle Control**: Create, update, start, close, review, and reopen sprints directly from the CLI, REST API, MCP tools, and web UI.
- **Canonical Storage**: Sprint definitions live in `.tasks/@sprints/<number>.yml` with clearly separated `plan` and `actual` sections plus optional history entries.
- **Single Membership**: Tasks own a `sprints: []` field; helpers (`sprint add/move/remove/backlog`) enforce one active sprint per task unless `--force` is used.
- **Integrity & Cleanup**: Commands warn about overdue starts/closes, detect missing sprint files, and clean up dangling memberships with `--cleanup-missing` or `sprint cleanup-refs`.
- **Analytics Suite**: Built-in `sprint summary`, `sprint stats`, `sprint burndown`, `sprint calendar`, and `sprint velocity` surface progress, capacity, and scheduling data in both text and JSON formats.

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
- **Built-in Web Server** with embedded Vue frontend (built via Vite into `target/web`)
- **REST API** for all task operations
    - Multi-value filters for list: `status`, `priority`, `type`, `tags`
    - Proper 404 for unknown resources
    - CORS preflight support for `/api/*`
- **Configurable ports** (default 8080, use `-p` as a shorthand for `--port`)
- **Personalized chrome** — Preferences page lets each user choose system/light/dark themes and an optional custom accent color; browser chrome tint follows the active theme.
- **SSE**: realtime events with `retry` hint and periodic heartbeats
- **Productive task board UI**
    - Saved views persist locally and sync with the current URL so you can return to curated filters quickly.
    - Smart filter chips toggle common slices like Mine, Unassigned, High, In progress, Blocked, Due soon (7d), Overdue, and No estimate without touching the full filter bar.
    - `?` opens an in-app keyboard overlay listing navigation shortcuts (`g t`, `g b`, `g i`, `/` to focus search), with more shortcuts landing soon.

### Server flags and endpoints

- `lotar serve --host 0.0.0.0 --port 8080 --open`
    - `--host` controls the bind address (default: 127.0.0.1). Use `0.0.0.0` to listen on all interfaces.
    - `--open` opens the default browser to the server URL, but does not change bind address.
    - `-p` is accepted as a shorthand for `--port`; positional `lotar serve 8080` also continues to work for backward compatibility.
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
lotar serve -p 8080       # Start web server
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
cargo nextest run --all-features # Run all tests (preferred harness)
cargo clippy          # Linting
cargo fmt             # Formatting
```

### UI (local dev)

In one terminal:

```bash
npm run dev
```

This serves the UI on http://localhost:5173 for fast iteration. In another terminal, run the server:

```bash
cargo run -- serve -p 8080
```

Or build UI and run the server that serves embedded static files:

```bash
npm run build
cargo run -- serve -p 8080
```

Ready for production use with no known critical issues.

### Test harness

Install nextest once, then use it directly (or run `npm run test`, which wraps the same command):

```bash
cargo install cargo-nextest --locked
cargo nextest run --all-features
```

Nextest configuration is in `.config/nextest.toml` and sets:
- run-threads = num-cpus
- failure-output = immediate, status-level = fail, fail-fast = true
- a 90s per-test timeout as a guardrail

Need Rust doc tests? Run them explicitly:

```bash
cargo test --doc --all-features
```
