# LoTaR - Local Task Repository

> A git-integrated task management system that lives in your repository.

[![Production Ready](https://img.shields.io/badge/status-production%20ready-brightgreen)](docs/README.md)
[![Tests](https://img.shields.io/badge/tests-129%20passing-brightgreen)](docs/README.md#testing--quality)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://www.rust-lang.org/)

## 🚀 Quick Start

```bash
# Clone and build
git clone https://github.com/mallox/lotar
cd lotar
cargo build --release

# Create your first task (auto-initializes project with defaults)
lotar task add --title="Setup project" --project=myapp --priority=HIGH

# List tasks
lotar task list --project=myapp

# Start web interface
lotar serve 8080

# Customize your workflow (optional)
lotar config init --template=agile  # Only needed for custom settings
lotar config set issue_states TODO,IN_PROGRESS,REVIEW,DONE
```

## ✨ What is LoTaR?

LoTaR is a **production-ready task management system** designed for developers who want their task tracking to live alongside their code. Instead of external tools that get out of sync, LoTaR stores tasks as human-readable YAML files in your repository.

### Key Benefits
- 🔒 **Git-native**: Tasks are version-controlled with your code
- 📝 **Human-readable**: YAML files you can edit manually
- 🚀 **Fast**: Sub-100ms operations with smart indexing
- 🔍 **Integrated**: Scan source code for TODO comments
- 🌐 **Complete**: CLI, web interface, and REST API
- 🛡️ **Secure**: Project isolation and input validation
- ⚡ **Zero-config**: Auto-initializes projects with sensible defaults

## 🎯 Core Features

### Task Management
```bash
# Full CRUD operations with formatted IDs
# Auto-initializes project configs with sensible defaults
lotar task add --title="OAuth Implementation" --type=feature --priority=HIGH
lotar task status PROJ-001 IN_PROGRESS
lotar task search "authentication" --priority=HIGH
```

### Source Code Integration
```bash
# Scan for TODOs in 25+ programming languages
lotar scan ./src
```

### Web Interface
```bash
# Built-in web server with React frontend
lotar serve 8080
```

## 📁 How It Works

LoTaR creates a `.tasks/` directory in your repository:

```
.tasks/
├── index.yml                 # Global search index and project metadata
├── config.yml               # Global configuration
├── BACKEND/                 # Project folders
│   ├── config.yml          # Project-specific configuration (optional)
│   ├── 001.yml             # Individual task files
│   └── 002.yml
└── FRONTEND/
    └── 001.yml
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

LoTaR uses a flexible configuration system that supports both global and project-specific settings.

### Zero-Configuration Setup

**For most users, no configuration is needed!** LoTaR automatically initializes projects with sensible defaults when you create your first task:

```bash
# This automatically creates default configuration for "myproject"
lotar task add --title="First task" --project=myproject

# Subsequent tasks reuse the existing configuration
lotar task add --title="Second task" --project=myproject  # No auto-init message
```

The auto-initialization creates:
- Global config (`.tasks/config.yml`) if it doesn't exist
- Project-specific config (`.tasks/{PROJECT}/config.yml`) with default template
- Proper project folder structure with consistent naming

### Configuration Hierarchy

1. **Built-in defaults** (lowest priority)
2. **Global config** (`.tasks/config.yml`)
3. **Home config** (`~/.lotar`) 
4. **Project config** (`.tasks/{project}/config.yml`)
5. **Environment variables** (highest priority)

### Configuration Commands

```bash
# View current configuration
lotar config show

# Manual initialization (only needed for custom templates/settings)
lotar config init --template=agile --project=myapp

# Set global configuration
lotar config set server_port 9000
lotar config set default_project myapp

# Set project-specific configuration
lotar config set issue_states TODO,WORKING,REVIEW,DONE --project=myapp
lotar config set tags backend,frontend,* --project=myapp

# List available templates
lotar config templates
```

> **Note**: The `config init` command is only needed when you want to use custom templates or settings. For basic usage, just start adding tasks and LoTaR will handle the setup automatically.

### Available Templates

- **simple**: Minimal workflow (TODO/IN_PROGRESS/DONE)
- **agile**: Full agile workflow with epics, sprints, and stories
- **kanban**: Continuous flow with assignee requirements
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

## 🧪 Production Ready

- ✅ **129 tests passing** with comprehensive coverage
- ✅ **Memory safe** with Rust's ownership system
- ✅ **Performance optimized** for large task sets
- ✅ **Security validated** with project isolation

## 📖 Documentation

**Getting Started:**
- [📚 Complete Documentation](docs/README.md) - Features, commands, and usage
- [🏗️ Architecture & Technical Reference](docs/architecture-decisions.md) - System design and file formats

**Advanced:**
- [🔮 Future Features](docs/mcp-integration-specification.md) - Planned AI agent integration

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
# Run tests
cargo test

# Development build
cargo build

# Code quality
cargo clippy
cargo fmt
```

## 🤝 Use Cases

- **Development Teams**: Track features, bugs, and technical debt alongside code
- **Solo Developers**: Keep tasks organized without external dependencies
- **Code Reviews**: See task context in git history and diffs
- **Compliance**: Immutable audit trail of decisions and changes
- **Documentation**: Requirements that evolve with your codebase

## 📝 Example Workflow

```bash
# Start a new feature (auto-initializes with defaults)
lotar task add --title="Add user authentication" --type=feature --priority=HIGH

# Work on it (scan finds TODOs automatically)
lotar scan ./src

# Update status as you progress
lotar task status AUTH-001 IN_PROGRESS

# Add more tasks to the same project (reuses existing config)
lotar task add --title="Add password reset" --project=auth --type=feature

# Search related tasks
lotar task search "auth" --status=TODO

# Complete and track
lotar task status AUTH-001 DONE
git add .tasks/ && git commit -m "Complete user authentication feature"
```

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🌟 Why LoTaR?

Unlike external task trackers that become outdated and disconnected from your code, LoTaR keeps your task management **in sync with your development workflow**. Every requirement change, status update, and decision is version-controlled alongside the code it affects.

With **zero-configuration setup**, you can start tracking tasks immediately without any upfront configuration. LoTaR automatically creates sensible defaults, but still gives you full control to customize your workflow when needed.

Perfect for teams who want the benefits of structured task management without losing the simplicity and reliability of git-based workflows.
