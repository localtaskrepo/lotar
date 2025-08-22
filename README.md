# LoTaR - Local Task Repository

> A git-integrated task management system that lives in your repository.

[![Production Ready](https://img.shields.io/badge/status-production%20ready-brightgreen)](docs/README.md)
[![Tests](https://github.com/localtaskrepo/lotar/actions/workflows/test.yml/badge.svg)](https://github.com/localtaskrepo/lotar/actions/workflows/test.yml)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://www.rust-lang.org/)

## 🚀 Quick Start

```bash
# Clone and build
git clone https://github.com/mallox/lotar
cd lotar
cargo build --release

# Create your first task (automatically initializes with smart defaults)
lotar add "Setup authentication system" --priority=high

# List all tasks (auto-detects the single project)
lotar list

# Change task status 
lotar status AUTH-001 in_progress

# Add assignee to a task
lotar assignee AUTH-001 john.doe@company.com

# Different output formats
lotar list --format=table
lotar list --format=json

# Start web interface
lotar serve --host=0.0.0.0 --port=8080 --open
```

## ✨ What is LoTaR?

LoTaR is a **production-ready task management system** designed for developers who want their task tracking to live alongside their code. Instead of external tools that get out of sync, LoTaR stores tasks as human-readable YAML files in your repository.

### Key Benefits
- 🔒 **Git-native**: Tasks are version-controlled with your code
- 📝 **Human-readable**: YAML files you can edit manually
- 🚀 **Fast**: Sub-100ms operations with direct file operations
- 🔍 **Integrated**: Scan source code for TODO comments
- 🌐 **Complete**: CLI, web interface, and REST API
- 🛡️ **Secure**: Project isolation and input validation
- ⚡ **Zero-config**: Auto-initializes projects with sensible defaults
- 🧠 **Smart**: Intelligent project resolution and auto-detection

## � Multi-Project Workflows

For teams managing multiple projects in one repository or across different directories:

```bash
# Explicit project specification
lotar add "Setup API auth" --project=backend --priority=high
lotar add "Design login UI" --project=frontend --priority=medium

# List tasks by project (supports full names or auto-generated prefixes)
lotar list --project=backend      # Full name
lotar list --project=BACK         # Auto-generated prefix

# Search across projects with context
lotar list --search="auth" --project=backend
# → [BACK-001] Setup API auth - BACKEND (Priority: HIGH)

# Custom tasks directory for different environments
export LOTAR_TASKS_DIR=/shared/project-tasks
lotar add "Integration test" --project=testing
# OR use command-line override
lotar add "Deploy script" --tasks-dir=/ops/tasks --project=deployment

# Advanced configuration per project
lotar config init --template=agile --project=backend
lotar config set issue_states TODO,IN_PROGRESS,REVIEW,DONE --project=backend
```
> **[📖 Smart Project Management Guide](docs/smart-project-management.md)** - Detailed documentation on intelligent project resolution, auto-detection, and flexible naming

## 🎯 Core Features

### Task Management
```bash
# Full CRUD operations with formatted IDs
lotar add "OAuth Implementation" --type=feature --priority=high
lotar status PROJ-001 in_progress
lotar assignee PROJ-001 john.doe@company.com
lotar list --priority=high
```

### Environment Variables & Global Options
```bash
# Environment variable support (applies to all commands)
export LOTAR_TASKS_DIR=/project/tasks
export LOTAR_DEFAULT_ASSIGNEE=john.doe@company.com
lotar add "Environment-configured task"  # Uses environment settings

# Global options work with ALL commands
lotar add "Task" --tasks-dir=/custom/path
lotar list --tasks-dir=/custom/path
lotar config show --tasks-dir=/custom/path

# Output format control
lotar list --format=table     # Terminal table
lotar list --format=json      # JSON for scripting  
lotar list --format=markdown  # Markdown output
```

### Source Code Integration
```bash
# Scan for TODOs in 25+ programming languages
lotar scan ./src
```

### Git-derived Stats (read-only)
```bash
# Tickets changed in the last 14 days (project by default)
lotar stats changed --since 14d

# Highest churn (commits per ticket) across all projects in the last 30 days
lotar stats churn --since 30d --global

# Top authors touching tasks in the last 90 days
lotar stats authors --since 90d --global

# Activity grouped by day (or author|week|project) in the last 60 days
lotar stats activity --since 60d --group-by day

# JSON output for scripting
lotar --format json stats changed --since 7d
```

### Web Interface
```bash
# Built-in web server with React frontend
lotar serve --host 127.0.0.1 --port 8080
```

### Task History (read-only, from git)
```bash
# Show commit history for a task
lotar task history PROJ-123

# Show raw diff for the latest commit touching the task (or specify --commit)
lotar task diff PROJ-123
lotar task diff PROJ-123 --commit abcdef1

# Show the task file snapshot at a specific commit
lotar task at PROJ-123 abcdef1
```

## 📁 How It Works

LoTaR creates a `.tasks/` directory in your repository:

```
```
.tasks/
├── config.yml               # Global configuration
├── BACKEND/                 # Project folders
│   ├── config.yml          # Project-specific configuration (optional)
│   ├── 001.yml             # Individual task files
│   └── 002.yml
└── FRONTEND/
    └── 001.yml
```
```

Each task is stored as a readable YAML file with structured data:
```yaml
title: "Implement OAuth Authentication"
status: "IN_PROGRESS"
priority: "HIGH"
task_type: "feature"
assignee: "john.doe@company.com"
created: "2025-07-30T10:00:00Z"
```

## ⚙️ Configuration

### Zero-Configuration Start
For most users, **no configuration is needed**! LoTaR automatically initializes projects with sensible defaults:

```bash
# This automatically creates default configuration
lotar add "First task" --project=myproject
```

### Configuration Commands
```bash
# View current configuration
lotar config show

# Manual initialization (only for custom templates)
lotar config init --template=agile --project=myapp

# Set global/project settings
lotar config set server_port 9000
lotar config set issue_states TODO,WORKING,REVIEW,DONE --project=myapp

# List available templates (default, agile, kanban, simple)
lotar config templates
# See also: docs/help/templates.md for details
```

### Configuration Hierarchy
1. Built-in defaults
2. Global config (`.tasks/config.yml`)
3. Home config (`~/.lotar`) 
4. Project config (`.tasks/{project}/config.yml`)
5. Environment variables (`LOTAR_TASKS_DIR`, `LOTAR_DEFAULT_ASSIGNEE`)
6. Command-line flags (highest priority)

<details>
<summary>📋 Complete Configuration Reference</summary>

### Environment Variables
- `LOTAR_TASKS_DIR`: Override tasks directory (absolute: `/project/tasks` or relative: `.issues`)
- `LOTAR_DEFAULT_ASSIGNEE`: Set default assignee for all tasks

### Available Templates
- **simple**: Minimal workflow (TODO/IN_PROGRESS/DONE)
- **agile**: Full agile workflow with epics, sprints, and stories
- **kanban**: Continuous flow; uses simple defaults
- **default**: Basic configuration using global defaults

### Configurable Fields
**Global Settings:**
- `server_port`: Web interface port (default: 8080)
- `default_project`: Default project name
- `tasks_dir_name`: Task storage directory name

**Project Settings:**
- `issue_states`: Valid task statuses (TODO, IN_PROGRESS, DONE, etc.)
- `issue_types`: Task types (feature, bug, chore, epic, etc.)
- `issue_priorities`: Priority levels (LOW, MEDIUM, HIGH, etc.)
- `categories`: Organizational categories (wildcard by default)
- `tags`: Task tags (wildcard by default)
- `default_assignee`: Default task assignee
- `default_priority`: Default priority level

</details>

## 🧪 Production Ready

- ✅ **Comprehensive test suite** with continuous integration
- ✅ **Memory safe** with Rust's ownership system
- ✅ **Performance optimized** for large task sets
- ✅ **Security validated** with project isolation

## 🤝 Use Cases

- **Development Teams**: Track features, bugs, and technical debt alongside code
- **Solo Developers**: Keep tasks organized without external dependencies
- **Code Reviews**: See task context in git history and diffs
- **Compliance**: Immutable audit trail of decisions and changes
- **Documentation**: Requirements that evolve with your codebase

## 📖 Documentation

**Getting Started:**
- [📚 Complete Documentation](docs/README.md) - Features, commands, and usage
- [📇 Help Index](docs/help/index.md) - Central links to command help and references
- [⚖️ Resolution & Precedence](docs/help/precedence.md) - Config/identity/path source order
- [🧠 Smart Project Management](docs/smart-project-management.md) - Intelligent project resolution and auto-detection
- [🕓 Git-based History & Stats](docs/mcp-web-foundation-plan.md) - Read-only history design and analytics overview
- [🏗️ Architecture & Technical Reference](docs/architecture-decisions.md) - System design and file formats

**Advanced:**
- [🔮 Future Features](docs/mcp-integration-specification.md) - Planned AI agent integration

## 📝 Example Workflow

```bash
# Start a new feature (auto-initializes with defaults)
lotar add "Add user authentication" --type=feature --priority=high --project=auth

# Scan for TODOs in your code
lotar scan ./src

# Update status as you progress
lotar status AUTH-001 in_progress

# Add related tasks (smart project resolution)
lotar add "Add password reset" --project=auth
lotar add "Add 2FA support" --project=authentication  # Full name also works

# Filter and search
lotar list --search="auth" --status=todo

# Complete and track in git
lotar status AUTH-001 done
git add .tasks/ && git commit -m "Complete user authentication feature"
```

## 🛠️ Installation

### Prerequisites
- Rust (stable toolchain)
- Git

### Build from Source
```bash
git clone https://github.com/mallox/lotar
cd lotar
cargo build --release

# Optional: Add to PATH
export PATH="$PATH:$(pwd)/target/release"
```

### Development
```bash
cargo test      # Run tests
cargo build     # Development build
cargo clippy    # Code quality
```

### Testing (faster local runs)
```bash
# Standard
cargo test --all-features

# Faster harness (optional)
cargo nextest run --all-features
```
Nextest uses a more efficient test harness and parallelism; see `.config/nextest.toml`.

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🌟 Why LoTaR?

Unlike external task trackers that become outdated and disconnected from your code, LoTaR keeps your task management **in sync with your development workflow**. Every requirement change, status update, and decision is version-controlled alongside the code it affects.

With **zero-configuration setup** and **intelligent project management**, you can start tracking tasks immediately without any upfront configuration. LoTaR automatically creates sensible defaults and intelligently resolves project names, but still gives you full control to customize your workflow when needed.

Perfect for teams who want the benefits of structured task management without losing the simplicity and reliability of git-based workflows.
