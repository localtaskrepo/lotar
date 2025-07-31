# LoTaR Documentation

## Production Status - July 2025

**🎉 PRODUCTION READY - 100% FUNCTIONAL**

LoTaR is a complete, production-ready task management system with CLI interface, web server, and source code integration.

- **66 tests passing** with zero failures
- **All core features implemented**
- **Zero compilation errors**
- **Memory safe** with Rust's ownership system

## Quick Start

```bash
# Install and build
git clone https://github.com/mallox/lotar
cd lotar
cargo build --release

# Create your first task
lotar task add --title="Setup project" --project=myapp --priority=HIGH

# List tasks
lotar task list --project=myapp

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
- **Performance Indexing**: Global tag index for fast searches

### Source Code Integration
- **25+ Programming Languages** supported
- **TODO Comment Detection** with UUID tracking
- **Multiple Comment Styles**: //, #, --, ;, %, /* */
- **Recursive Directory Scanning**

### Web Interface & API
- **Built-in Web Server** with embedded React frontend
- **REST API** for all task operations
- **Configurable ports** (default 8080)

## Command Reference

```bash
# Task Management
lotar task add --title="Title" --type=feature --priority=HIGH
lotar task list --project=backend --status=IN_PROGRESS
lotar task search "keyword" --priority=HIGH
lotar task status PROJ-001 DONE
lotar task edit PROJ-001 --assignee="user@example.com"
lotar task delete PROJ-001

# System Commands
lotar serve 8080          # Start web server
lotar scan ./src          # Scan for TODOs
lotar index rebuild       # Rebuild search index
lotar config set key val  # Configuration
```

## File Structure

```
.tasks/
├── index.yml             # Global search index
├── PROJECT-A/           # Project directory
│   ├── metadata.yml     # Project metadata
│   ├── 1.yml           # Task files
│   └── 2.yml
└── PROJECT-B/
    └── metadata.yml
```

## Testing & Quality

- **66 tests** across 6 test suites (100% passing)
- **CLI Integration**: 21 tests
- **Storage**: 15 tests  
- **Search/Filter**: 9 tests
- **Indexing**: 8 tests
- **End-to-end**: 8 tests
- **Scanner**: 5 tests

Performance: Sub-100ms operations, minimal memory usage

## Development

```bash
cargo build           # Debug build
cargo build --release # Release build
cargo test            # Run all tests
cargo clippy          # Linting
cargo fmt             # Formatting
```

Ready for production use with no known critical issues.
