# LoTaR - Local Task Repository

A git-integrated task management system that lives in your repository.

## 🎉 Production Ready - 100% Functional

LoTaR is a complete, production-ready task management system with CLI interface, web server, and advanced source code integration. All core features are implemented and thoroughly tested with **66 tests passing** and zero failures.

## ✨ Key Features

### 📋 Complete Task Management
- **Full CRUD Operations**: Create, read, update, delete tasks via CLI
- **Advanced Status System**: TODO, IN_PROGRESS, VERIFY, BLOCKED, DONE
- **Priority Levels**: LOW, MEDIUM, HIGH, CRITICAL
- **Task Types**: Feature, Bug, Epic, Spike, Chore
- **Project Organization**: Strict project isolation with security boundaries
- **Formatted IDs**: PROJECT-001, PROJECT-002 format with automatic generation

### 🔍 Advanced Search & Filtering
- **Full-text search** across titles, descriptions, and tags
- **Multi-criteria filtering** by status, priority, project, tags
- **High-performance indexing** with sub-100ms search times
- **Cross-project search** with proper security isolation
- **Tag-based organization** with global tag indexing

### 💻 Command Line Interface
```bash
# Task management
lotar task add --title="OAuth Implementation" --type=feature --priority=HIGH
lotar task list --project=backend --status=IN_PROGRESS
lotar task search "authentication" --priority=HIGH
lotar task status PROJ-001 DONE
lotar task delete PROJ-001

# Source code integration
lotar scan ./src

# Web interface
lotar serve 8080

# System maintenance
lotar index rebuild
```

### 🔧 Source Code Integration
- **25+ Programming Languages** supported (Rust, JavaScript, Python, Java, C++, Go, etc.)
- **TODO comment detection** with UUID tracking
- **Recursive directory scanning** with file type filtering
- **Multiple comment styles**: //, #, --, ;, %, /* */
- **Smart comment recognition** by file extension

### 🌐 Web Server & API
- **Built-in web server** with embedded React frontend
- **REST API endpoints** for task management
- **Static file serving** with proper MIME types
- **Configurable ports** and routing
- **JSON/YAML data exchange**

### 📁 Modern Storage Architecture
- **YAML format**: Human-readable .yml files (standardized from .yaml)
- **Project-based directories**: Each project gets its own folder
- **Metadata tracking**: Task counts, ID generation, file mappings
- **Git-friendly**: All files are version-controllable
- **Performance indexing**: Global tag index for fast searches

## 🏗️ Architecture

### File Structure
```
.tasks/
├── index.yml                 # Global search index
├── PROJECT-A/               # Project folder (auto-generated prefix)
│   ├── metadata.yml         # Project metadata & ID mapping
│   ├── 1.yml               # Task files (numeric names)
│   ├── 2.yml
│   └── 3.yml
├── BACKEND/                 # Another project
│   ├── metadata.yml
│   ├── 1.yml
│   └── 2.yml
└── WEB/                     # Web project
    ├── metadata.yml
    └── 1.yml
```

### Task ID Format
- **Format**: `PROJECT-001`, `BACKEND-005`, `WEB-012`
- **Generation**: Automatic based on project folder name
- **Uniqueness**: Per-project numbering with global uniqueness
- **Security**: Project isolation prevents cross-project access

### Index System
- **Global tag index**: Fast tag-based searches across all projects
- **Project metadata**: Task counts, current ID, file mappings
- **YAML format**: Consistent with task files
- **Automatic rebuilding**: Maintains consistency

## 🚀 Quick Start

### Installation
```bash
# Clone and build
git clone https://github.com/mallox/lotar
cd lotar
cargo build --release

# Add to PATH (optional)
export PATH="$PATH:$(pwd)/target/release"
```

### Basic Usage
```bash
# Create your first task
lotar task add --title="Setup project" --project=myapp --priority=HIGH

# List tasks
lotar task list --project=myapp

# Update task status
lotar task status MYAP-001 IN_PROGRESS

# Search tasks
lotar task search "setup" --project=myapp

# Scan source code for TODOs
lotar scan ./src

# Start web interface
lotar serve 8080
```

## 🧪 Testing & Quality

### Test Coverage
- **66 tests passing** across 6 test suites
- **21 CLI integration tests** - Command-line interface
- **15 storage tests** - Data persistence and retrieval
- **9 search/filter tests** - Query functionality
- **8 index system tests** - Performance indexing
- **8 comprehensive tests** - End-to-end scenarios
- **5 scanner tests** - Source code integration

### Code Quality
- **Zero compilation errors**
- **Comprehensive error handling** with custom error types
- **Memory safety** with Rust's ownership system
- **Project isolation security** - Prevents cross-project data leaks
- **Performance optimization** - Sub-100ms operations

### Architecture Benefits
- **Command pattern**: Clean, extensible CLI commands
- **Enum-based types**: Type-safe status, priority, and task types
- **YAML standardization**: Consistent file format across system
- **Index optimization**: Fast searches without full file scans
- **Project security**: Strict boundaries between projects

## 📖 Documentation

- [Architecture Decisions](docs/architecture-decisions.md) - Core design choices
- [Current State Analysis](docs/current-state-analysis.md) - Feature completion status
- [Implementation Roadmap](docs/implementation-roadmap.md) - Development phases
- [Feature Specifications](docs/feature-specifications.md) - Detailed feature docs
- [Testing Strategy](docs/testing-strategy.md) - Quality assurance approach

## 🔧 Development

### Building
```bash
cargo build          # Debug build
cargo build --release # Optimized build
```

### Testing
```bash
cargo test           # Run all tests
cargo test storage   # Run storage tests only
cargo test -- --nocapture # Show test output
```

### Code Quality
```bash
cargo clippy         # Linting
cargo fmt           # Code formatting
cargo doc           # Generate documentation
```

## 🎯 Current Status (July 2025)

✅ **Production Ready** - All core features implemented and tested  
✅ **Architecture Refactored** - Clean command pattern and error handling  
✅ **Dead Code Cleaned** - Removed deprecated methods and unused code  
✅ **Project Isolation Fixed** - Secure boundaries between projects  
✅ **File Format Standardized** - Consistent YAML (.yml) format  
✅ **Performance Optimized** - Fast indexing and search capabilities  
✅ **Test Suite Complete** - Comprehensive coverage with 66 passing tests  

## 📝 License

This project is licensed under the MIT License - see the LICENSE file for details.

---

**LoTaR - Where tasks meet code** 🎯
