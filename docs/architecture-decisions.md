# Architecture & Technical Reference

*Last Updated: August 1, 2025*

## System Architecture

LoTaR follows a clean, modular architecture built around Rust's type safety and performance characteristics.

### Core Components

**Storage Layer (`src/storage/*`)**
- YAML-based persistence with human-readable `.yml` files
- Project isolation with separate directories
- Automatic ID generation (`PROJ-1` format with 4-character prefixes)
- Metadata management for task counting and file mapping

**Type System (`types.rs`)**
- TaskStatus enum: TODO, IN_PROGRESS, VERIFY, BLOCKED, DONE
- Priority enum: LOW, MEDIUM, HIGH, CRITICAL
- TaskType enum: Feature, Bug, Epic, Spike, Chore
- Relationships struct for dependencies and hierarchies
- Custom fields HashMap for team-specific data

**Search & Filtering (`src/storage/search.rs`)**
- Query helpers over on-disk tasks without a DB
- Filtering by status, priority, task type, tags, text
- Case-insensitive fuzzy matching utilities for flexible queries

**Scanner Engine (`scanner.rs`)**
- Multi-language support (25+ programming languages)
- Comment detection for //, #, --, ;, %, /* */ styles
- UUID tracking for persistent TODO identification
- File type recognition via extension mapping

**CLI Interface (`src/cli/*`)**
- Command pattern for extensible command structure
- Robust enum-based argument validation
- Custom error types with proper propagation
- Automatic project context detection

**Web Server (`web_server.rs`, `api_server.rs`)**
- Embedded Vue frontend built via Vite into `target/web`
- REST API with JSON endpoints
- Static file serving with proper MIME types
- Configurable port and path settings

## Task File Format

### YAML Structure
```yaml
title: "Implement OAuth Authentication"
status: "TODO"                          # Required enum
priority: "HIGH"                        # Required enum
task_type: "feature"                    # Required enum
assignee: "john.doe@company.com"        # Optional
project: "webapp"                       # Auto-set
created: "2025-07-30T10:00:00Z"         # Auto-generated
modified: "2025-07-30T14:30:00Z"        # Auto-updated
due_date: "2025-08-15"                  # Optional
effort: "5d"                            # Optional

# Structured fields
acceptance_criteria:
  - "User can login with Google OAuth"
  - "User can login with GitHub OAuth"

relationships:
  depends_on: ["AUTH-002", "SEC-001"]
  blocks: ["USER-005"]
  related: ["AUTH-003"]
  parent: "EPIC-USER-AUTH"

comments:
  - author: "jane.smith@company.com"
    date: "2025-07-30T15:00:00Z"
    text: "Added security requirements"

# Legacy fields (backward compatibility)
subtitle: "OAuth integration for web app"
description: "Detailed implementation notes..."
tags: ["auth", "security", "oauth"]

# Custom fields (team-specific)
custom_fields:
  epic: "user-management"
  story_points: 8
  security_review_required: true
```

### File Organization
```
.tasks/
├── index.yml                 # Global search index
├── PROJECT-A/               # Project folder (matches task ID prefix)
│   ├── metadata.yml         # Project metadata
│   ├── 1.yml               # Task files (numeric names)
│   ├── 2.yml
│   └── 3.yml
└── PROJECT-B/
    ├── metadata.yml
    └── 1.yml
```

## Sprint Storage Model

Sprints are stored separately from project task files so lifecycle data stays canonical and easy to audit.

### Directory Layout

```
.tasks/
└── @sprints/
    ├── 1.yml
    ├── 2.yml
    └── ...
```

- Sprint identifiers come from the filename (`<number>.yml`) and are not duplicated inside the YAML body.
- Writers produce deterministic key ordering and drop empty values to keep Git diffs clean.
- Sprint files never embed task identifiers. Tasks own a `sprints: []` array and CLI helpers (`sprint add | move | remove | backlog`) enforce the single-membership contract unless `--force` is explicitly supplied.

### YAML Structure

```yaml
plan:
  label: "Sprint 42"
  goal: "Ship new onboarding flow"
  starts_at: 2025-10-10T09:00:00-04:00
  length: 2w
  ends_at: 2025-10-24T17:00:00-04:00
  capacity:
    points: 40
    hours: 320
  overdue_after: 12h
  notes: |
    Kickoff on Monday. Focus on onboarding.
actual:
  started_at: 2025-10-13T09:00:00-04:00
  closed_at: 2025-10-24T16:12:11-04:00
history:
  - at: 2025-10-12T14:22:00Z
    actor: alice@example.com
    changes:
      - field: plan.goal
        new: "Ship new onboarding flow"
```

### Derived Status Rules

- **Pending** when `actual.started_at` is absent.
- **Active** once started and before the computed end.
- **Overdue** after the computed end while `actual.closed_at` remains unset.
- **Complete** once `actual.closed_at` is recorded.

End dates prefer `plan.ends_at`; otherwise the planner adds `plan.length` to the actual start. If both `length` and `ends_at` are supplied, tools keep `ends_at`, clear `length`, and emit a warning so the precedence change is obvious in command output.

### Lifecycle & Integrity Helpers

- `sprints.defaults.*` configuration keys pre-populate length, capacity, and overdue thresholds; callers may override them per sprint or per command.
- `sprints.notifications.enabled` toggles lifecycle warnings (late start/close, future-start info). Commands also accept `--no-warn` to silence messages ad hoc.
- `sprint cleanup-refs` and the `--cleanup-missing` flag remove task memberships that point at deleted sprint files, returning both structured integrity payloads and human-readable messages.
- CLI, REST, MCP, and UI surfaces share the same integrity responses so automated workflows and interactive clients stay in sync when reassignments are forced or cleanup occurs.

## Architecture Decisions

### AD-001: YAML Over JSON
**Decision**: Use YAML for all data persistence  
**Rationale**: Human-readable, git-friendly diffs, comment support, consistent format

### AD-002: Project-Based Directories
**Decision**: Store tasks in project-specific directories  
**Rationale**: Isolation, scalability, performance, clear boundaries

### AD-003: Formatted Task IDs
**Decision**: Use `PROJ-1` format for external references (4-character prefix + sequential number)  
**Rationale**: Human-readable, unique, sortable, professional, compact

### AD-004: Command Pattern for CLI
**Decision**: Implement CLI using Command trait pattern  
**Rationale**: Extensibility, testability, maintainability, consistent error handling

### AD-005: Enum-Based Type Safety
**Decision**: Use Rust enums for status, priority, task type  
**Rationale**: Compile-time validation, performance, consistency, evolution support

### AD-006: Global Index for Performance
**Decision**: Maintain global index file for searches  
**Rationale**: Sub-100ms performance, cross-project search, scalability

### AD-007: Embedded Web Interface
**Decision**: Include Vue frontend (built with Vite) in Rust binary  
**Rationale**: Single binary deployment, no external dependencies, portability

### AD-008: Multi-Language Scanner
**Decision**: Support 25+ programming languages  
**Rationale**: Universal compatibility, flexibility, accuracy, maintainability

### AD-009: Project Isolation Security
**Decision**: Enforce strict project boundaries  
**Rationale**: Security, data integrity, team separation, compliance

## Performance Characteristics

### Response Times
- **Task operations**: < 50ms for typical workloads
- **Search operations**: < 100ms for 100+ tasks
- **Minimal footprint**: Lazy loading of task content
- **No memory leaks**: Rust ownership prevents issues
- Cross-project access explicitly prevented
- Security boundaries enforced at storage layer
- Atomic file operations where possible
- Proper error handling for failures
**Web Server (`web_server.rs`, `api_server.rs`)**
- Embedded Vue frontend built (Vite) into the binary (`target/web`)
- Server-Sent Events (SSE) for realtime updates on `/api/events` and list streaming on `/api/tasks/stream`
Tasks support arbitrary custom fields via `custom_fields` HashMap:
- Sprint numbers, story points, team assignments
### Test Coverage
- **Handler Unit Tests**: CLI command handler validation with mocking
- Any YAML-serializable data type

**Quality Metrics**
- Zero build warnings (clippy clean)
- All tests passing
- Memory safety via Rust ownership
- Project isolation validated
- Performance targets verified in CI

## Design Principles

1. **Git-Native**: All data structures designed for version control
2. **Human-Readable**: Files can be manually edited and reviewed
3. **Type Safety**: Leverage Rust's type system for correctness
4. **Performance**: Sub-100ms operations for typical workloads
5. **Portability**: Single binary with no external dependencies
6. **Security**: Project isolation and input validation
7. **Extensibility**: Clean interfaces for adding features

## Testing Framework & Quality

LoTaR includes a comprehensive, multi-layered testing framework ensuring production readiness:

### Test Coverage: 225 Tests
- **Handler Unit Tests**: CLI command handler validation with mocking
- **Integration Tests**: End-to-end CLI workflows and parameter validation
- **Storage Tests**: CRUD operations, validation, and project isolation
- **Configuration Tests**: Config system, templates, and home directory handling
- **Index Tests**: Search performance and data consistency
- **Scanner Tests**: Multi-language TODO detection
- **Experimental CLI Tests**: Real command execution in isolated environments

### Testing Infrastructure

**SimpleHandlerTestHarness**
- Isolated test environments with temporary directories
- Mock implementations of storage and configuration systems
- Extensible framework for adding new CLI commands

**CliTestHarness**  
- Integration testing with real command execution
- Environment isolation and cleanup
- Parameter validation and edge case testing

**TestDataBuilder**
- Fluent API for creating test task data
- Customizable task properties and relationships
- Realistic test scenarios and data generation

### Quality Metrics
- **Zero build warnings** - Clean compilation
- **100% test pass rate** - All 225 tests passing
- **Memory safety** - Rust ownership prevents leaks
- **Project isolation** - Security boundaries validated
- **Performance tested** - Sub-100ms operation validation

### Development Workflow
```bash
# Run full test suite
cargo nextest run --all-features

# Run specific test categories
cargo nextest run --all-features --filter handler_unit_tests
cargo nextest run --all-features --filter integration_workflows
cargo nextest run --all-features --filter experimental_cli

# Performance and code quality
cargo clippy    # Zero warnings enforced
cargo fmt       # Consistent formatting
```
