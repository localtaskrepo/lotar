# LoTaR - Local Task Repository

> A git-integrated task management system that lives in your repository.

[![Production Ready](https://img.shields.io/badge/status-production%20ready-brightgreen)](docs/README.md)
[![Tests](https://img.shields.io/badge/tests-66%20passing-brightgreen)](docs/README.md#testing--quality)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://www.rust-lang.org/)

## 🚀 Quick Start

```bash
# Clone and build
git clone https://github.com/mallox/lotar
cd lotar
cargo build --release

# Create your first task
lotar task add --title="Setup project" --project=myapp --priority=HIGH

# List tasks
lotar task list --project=myapp

# Start web interface
lotar serve 8080
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

## 🎯 Core Features

### Task Management
```bash
# Full CRUD operations with formatted IDs
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
├── index.yml                 # Global search index
├── BACKEND/                 # Project folders
│   ├── metadata.yml         # Project metadata
│   ├── 1.yml               # Individual task files
│   └── 2.yml
└── FRONTEND/
    ├── metadata.yml
    └── 1.yml
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

## 🧪 Production Ready

- ✅ **66 tests passing** with comprehensive coverage
- ✅ **Zero compilation errors** 
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
# Start a new feature
lotar task add --title="Add user authentication" --type=feature --priority=HIGH

# Work on it (scan finds TODOs automatically)
lotar scan ./src

# Update status as you progress
lotar task status AUTH-001 IN_PROGRESS

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

Perfect for teams who want the benefits of structured task management without losing the simplicity and reliability of git-based workflows.
