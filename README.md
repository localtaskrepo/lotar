# LoTaR - Local Task Repository

> A git-integrated task management system that lives in your repository.

[![Production Ready](https://img.shields.io/badge/status-production%20ready-brightgreen)](docs/README.md)
[![Tests](https://github.com/localtaskrepo/lotar/actions/workflows/test.yml/badge.svg)](https://github.com/localtaskrepo/lotar/actions/workflows/test.yml)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://www.rust-lang.org/)

## 🚀 Quick Start

LoTaR ships as both a signed Homebrew formula and a ready-to-run Docker image, so you can get moving without installing Rust or managing binaries manually. Pick the installer that matches your setup, then run the same commands everywhere.

### 1. Install LoTaR

**macOS ([Homebrew](https://brew.sh/))**
```bash
brew tap localtaskrepo/lotar
brew install lotar
lotar --version
```
This adds the CLI to your PATH and keeps it updated with `brew upgrade lotar`.

**Windows ([Scoop](https://scoop.sh/))**
```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser -Force
iwr -useb get.scoop.sh | iex
scoop bucket add lotar https://github.com/localtaskrepo/scoop-lotar
scoop install lotar
lotar --version
```
This installs LoTaR via the community bucket we publish during every release. Pass
`--version` to `scoop install` if you need to pin to a specific release.

**GitHub Releases (macOS • Linux • Windows)**
```bash
# Pick the asset for your platform from the releases page
curl -LO https://github.com/localtaskrepo/lotar/releases/latest/download/lotar-vX.Y.Z-linux-x64.tar.gz
tar -xzf lotar-vX.Y.Z-linux-x64.tar.gz
sudo mv lotar /usr/local/bin/
lotar --version
```
Verify signatures/checksums from the same release before moving the binary into your PATH if you need extra assurance.

**Any OS ([Docker Hub](https://hub.docker.com/r/mallox/lotar))**
```bash
docker pull mallox/lotar
docker run --rm mallox/lotar --version
```
The Docker image bundles the latest signed musl build, so Linux, macOS, and Windows users can all run the same artifact.

**Rust developers (from source)**
```bash
git clone https://github.com/localtaskrepo/lotar
cd lotar
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```
Use this option if you want to contribute or hack on LoTaR itself.

### 2. Point LoTaR at your repository

- **Homebrew/source install**: `cd /path/to/your/repo` and run LoTaR commands directly.
- **Docker**: mount your repo at `/workspace` and your `.tasks` directory at `/tasks`.

```bash
docker run --rm \
    -v "$PWD":/workspace \
    -v "$PWD/.tasks":/tasks \
    -w /workspace \
    mallox/lotar list
```

### 3. Track work

```bash
# Create your first task (auto-initializes defaults)
lotar add "Plan product launch" --priority=high

# List everything in a friendly table (auto-detects single project)
lotar list --format table

# Update status or assignee (numeric IDs work when LoTaR auto-detects the project or you set a default)
lotar status 1 in_progress
lotar assignee 1 alex@example.com

# Open the web UI if you prefer a browser
lotar serve --open
```

> LoTaR automatically scopes to your single project (or `default_project` setting), which is why the commands above can reference tasks with just the numeric portion. When you manage multiple projects or overlapping prefixes, use the fully-qualified IDs (`AUTH-12`) or attach `--project`. See [🗂️ Multi-Project Workflows (Advanced)](#%F0%9F%97%82%EF%B8%8F-multi-project-workflows-advanced) for details.

## ✨ What is LoTaR?

LoTaR is a **production-ready task management system** designed for developers who want their task tracking to live alongside their code. Instead of external tools that get out of sync, LoTaR stores tasks as human-readable YAML files in your repository.

Think of it as a local-first, git-native issue tracker (and task tracker) built for developer workflows.

### Key Benefits
- 🔒 **Git-native**: Tasks are version-controlled with your code
- 📝 **Human-readable**: YAML files you can edit manually
- 🚀 **Fast**: Sub-100ms operations with direct file operations
- 🔍 **Integrated**: Scan source code for TODO comments
- 🌐 **Complete**: CLI, web interface, and REST API
- 🛡️ **Secure**: Project isolation and input validation
- ⚡ **Zero-config**: Auto-initializes projects with sensible defaults
- 🧠 **Smart**: Intelligent project resolution and auto-detection

## 🗂️ Multi-Project Workflows (Advanced)

Most users never need to think about project prefixes—LoTaR automatically scopes to whichever project you’re working in. If you maintain multiple concurrent projects (monorepos, shared storage, cross-repo worktrees), use the fully-qualified IDs and project-specific commands below.

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
lotar init --template=agile --project=backend    # shortcut for `lotar config init`
lotar config set issue_states TODO,IN_PROGRESS,REVIEW,DONE --project=backend
```
> **[📖 Smart Project Management Guide](docs/smart-project-management.md)** - Detailed documentation on intelligent project resolution, auto-detection, and flexible naming

## 🎯 Core Features

### Task Management
```bash
# Full CRUD operations with formatted IDs
lotar add "OAuth Implementation" --type=feature --priority=high
lotar status 2 in_progress
lotar assignee 2 john.doe@company.com
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
# Built-in web server with Vue frontend
lotar serve --host 127.0.0.1 --port 8080
```

### Screenshots

A quick peek at the built-in web UI:

| Tasks | Boards |
| --- | --- |
| ![Tasks list](docs/assets/screenshots/ui-tasks.png) | ![Boards](docs/assets/screenshots/ui-boards.png) |
| ![Sprints](docs/assets/screenshots/ui-sprints.png) | ![Insights](docs/assets/screenshots/ui-insights.png) |

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
lotar init --template=agile --project=myapp

# Set global/project settings
lotar config set server_port 9000
lotar config set issue_states TODO,WORKING,REVIEW,DONE --project=myapp

# List available templates (default, agile, kanban)
lotar config templates
# See also: docs/help/templates.md for details
```

> Tip: `lotar init` is a shorthand for `lotar config init` and accepts the same flags.

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
- **default**: Basic workflow using global defaults (Todo/InProgress/Done, Feature/Bug/Chore, Low/Medium/High, wildcard tags, and categories)
- **agile**: Full agile workflow with epics, spikes, sprints, and rich vocabularies
- **kanban**: Continuous flow with verify gate, feature/bug/epic/chore types, and category custom field

### Configurable Fields
**Global Settings:**
- `server_port`: Web interface port (default: 8080)
- `default_project`: Default project name
- `tasks_dir_name`: Task storage directory name

**Project Settings:**
- `issue_states`: Valid task statuses (TODO, IN_PROGRESS, DONE, etc.)
- `issue_types`: Task types (feature, bug, chore, epic, etc.)
- `issue_priorities`: Priority levels (LOW, MEDIUM, HIGH, etc.)
- `custom_fields`: Additional fields like `product`, `sprint`, etc. (wildcard or curated lists)
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

## 🆘 Support & Bug Reports

- Support & bug reports: https://github.com/localtaskrepo/lotar/issues
- Security issues: https://github.com/localtaskrepo/lotar/security/advisories/new

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

Pick the delivery path that matches your environment; every artifact is produced by the same release workflow, so features and signatures stay consistent.

### macOS ([Homebrew](https://brew.sh/))
```bash
brew tap localtaskrepo/lotar
brew install lotar
lotar --version
```
The tap hosts universal binaries, so both Apple Silicon and Intel machines are supported. Upgrade any time with `brew upgrade lotar`.

### Windows ([Scoop](https://scoop.sh/))
```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser -Force
iwr -useb get.scoop.sh | iex
scoop bucket add lotar https://github.com/localtaskrepo/scoop-lotar
scoop install lotar
lotar --version
```
Use `scoop install lotar --version vX.Y.Z` when validating a specific release manifest from the bucket.

### Docker (macOS • Linux • Windows)
```bash
docker pull mallox/lotar
docker run --rm mallox/lotar --version

# Operate on your current repository
docker run --rm \
    -v "$PWD":/workspace \
    -v "$PWD/.tasks":/tasks \
    -w /workspace \
    mallox/lotar list
```
The image is a minimal `scratch` container that already contains the signed musl binary. See [`docs/docker.md`](docs/docker.md) or the [Docker Hub page](https://hub.docker.com/r/mallox/lotar) for more scenarios (shared tasks directories, environment variables, etc.).

### Build from Source (Rust)
```bash
git clone https://github.com/mallox/lotar
cd lotar
cargo build --release

# Optional: Add to PATH
export PATH="$PATH:$(pwd)/target/release"
```
You’ll need the stable Rust toolchain plus Node/npm (for the web assets) if you intend to run tests or `npm run build` locally.

### Development
```bash
npm run test            # Preferred full test run (uses nextest)
cargo nextest run --all-features  # Direct harness access
cargo build             # Development build
cargo clippy            # Code quality
```

### Additional testing notes
- Nextest uses a more efficient harness and parallelism; see `.config/nextest.toml` for defaults.
- Doc tests remain available via `cargo test --doc --all-features`.
- The legacy `cargo test` command intentionally errors and instructs you to use nextest.

### Releases
- Follow the automated release pipeline and post-release verification steps in
    [`docs/release-guide.md`](docs/release-guide.md).
- Manual verification workflows (`Verify Scoop Install` and `Verify Homebrew Install`)
    live under **Actions** for quick smoke-tests of installer paths.

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🌟 Why LoTaR?

Unlike external task trackers that become outdated and disconnected from your code, LoTaR keeps your task management **in sync with your development workflow**. Every requirement change, status update, and decision is version-controlled alongside the code it affects.

With **zero-configuration setup** and **intelligent project management**, you can start tracking tasks immediately without any upfront configuration. LoTaR automatically creates sensible defaults and intelligently resolves project names, but still gives you full control to customize your workflow when needed.

Perfect for teams who want the benefits of structured task management without losing the simplicity and reliability of git-based workflows.
