# lotar add

Create new tasks with smart defaults and validation.

## Usage

```bash
lotar add "Task title" [OPTIONS]
```

## Quick Examples

```bash
# Basic task
lotar add "Implement user authentication"

# Feature with details
lotar add "Add OAuth support" --type=feature -P high --assignee=john.doe

# Bug with due date
lotar add "Fix login crash" --type=bug -P critical --due=tomorrow

# Epic with custom fields
lotar add "User Management System" --type=epic --field=story_points=13 --field=sprint=2

# Custom tasks directory
lotar add "External task" --tasks-dir=/external/projects/tasks --project=external

# Environment variable usage
export LOTAR_TASKS_DIR=/project/tasks
lotar add "Environment task" --project=myapp  # Uses environment directory
```

## Options

### Core Properties
- `--type <TYPE>` - Task type: feature, bug, epic, spike, chore
- `--priority <PRIORITY>`, `-P <PRIORITY>` - Priority: low, medium, high, critical
- `--assignee <ASSIGNEE>` - Task assignee (email or @username). Supports `@me` to resolve to your identity.

### Scheduling
- `--due <DATE>` - Due date (YYYY-MM-DD or relative like 'tomorrow')
- `--effort <ESTIMATE>`, `-E <ESTIMATE>` - Effort estimate (e.g., 2d, 5h, 1w)

### Organization
- `--tag <TAG>`, `-i <TAG>` - Tags (can be used multiple times)
- `--description <TEXT>`, `-D <TEXT>` - Detailed description

### Shortcuts
- `--bug` - Shorthand for --type=bug
- `--epic` - Shorthand for --type=epic
- `--critical` - Shorthand for --priority=critical
- `--high` - Shorthand for --priority=high

### Planning and diagnostics
- `--dry-run` - Preview the task that would be created without writing
- `--explain` - Show how defaults (status/priority/reporter) were chosen
	- Includes project detection source, whether a path-derived tag was added, and any branch-based inference (type/status/priority) with alias hits

### Custom Fields
- `--field <KEY>=<VALUE>` - Arbitrary properties (can be used multiple times)

### Global Options
- `--format <FORMAT>` - Output format: text, table, json, markdown
- `--verbose` - Enable verbose output
- `--project <PROJECT>`, `-p <PROJECT>` - Target project (overrides auto-detection)
- `--tasks-dir <PATH>` - Custom tasks directory (overrides environment/config)

### Environment Variables
- `LOTAR_TASKS_DIR` - Default tasks directory location  
- `LOTAR_DEFAULT_ASSIGNEE` - Default assignee for new tasks
- `LOTAR_DEFAULT_REPORTER` - Default reporter identity when not provided

Reporter auto-set is driven by configuration: set `default_reporter` and ensure `auto.set_reporter: true` (default). The environment variable can provide a default reporter value.

Notes:
- `@me` resolution order: config.default_reporter (merged with precedence) → git user.name/email → $USER/$USERNAME.
- When `--assignee=@me` is provided, it is resolved at the CLI layer so previews and persisted values match.
 - See also: [Resolution & Precedence](./precedence.md).

## Output Formats

Control output with global `--format` option:

```bash
# Human-readable (default)
lotar add "New task" 
# 📋 Created task AUTH-001: New task

# JSON for scripting
lotar add "New task" --format=json
# {"status":"success","message":"Created task: AUTH-001","task":{"id":"AUTH-001", ...}}

# JSON dry-run preview
lotar add "Preview task" --dry-run --format=json
# {"status":"preview","action":"create","project":"AUTH","title":"Preview task","priority":"MEDIUM","status_value":"TODO"}
```

## Validation

All task properties are validated against project configuration:

- **Task Type**: Must be in configured `issue_types` list
- **Priority**: Must be in configured `issue_priorities` list  
- **Status**: Defaults to first state in `issue_states`
- **Tags**: Validated against `tags` config
- **Custom Fields**: Validated for format and allowed values

Configure validation rules with:
```bash
lotar config set issue.types feature,bug,chore
lotar config set issue.priorities low,medium,high,critical
```

## Project Resolution

Tasks are created in the appropriate project:

1. **Explicit project**: `--project=myproject` or `-p myproject`
2. **Task ID prefix**: If title starts with "PROJ-", uses PROJ project  
3. **Default project**: From global config `default_project`
4. **Auto-detection**:
	 - `LOTAR_PROJECT` environment variable (if set)
	 - Nearest project manifest walking up to the repo root:
		 - `package.json` name (scope stripped, e.g. `@scope/api` -> `api`)
		 - `Cargo.toml` `[package].name`
		 - `go.mod` module last path segment
	 - Git repo name (folder name at repo root)
	 - Current directory name

Auto-detection walks upward but stops at the repository root (supports `.git` directory or file for worktrees/submodules).

### Auto Tags from Monorepo Paths

When you don't pass any `--tag` and your config has no `default.tags`, LoTaR will try to derive a single tag from common monorepo structures:

- Recognized folders: `packages/<name>`, `apps/<name>`, `libs/<name>`, `services/<name>`, `examples/<name>`
- Hidden names are ignored (e.g. `.hidden`)
- The derived tag is validated against your configured `tags` list; if invalid, it's skipped

Examples:
```bash
# In repo/packages/api
lotar add "Implement endpoint"             # auto-adds --tag=api if allowed

# In repo/apps/web, with no configured 'web' tag
lotar add "Style header"                   # does NOT add tag (fails validation)
```

Configuration toggle:
- Set `auto.tags_from_path: false` in `.tasks/config.yml` (global) or in a project config to disable this behavior entirely.
	- Default is `true`.

### Branch-aware defaults (Phase 4)

When no `--type`/shortcuts are provided, LoTaR will try to infer a task type from the current git branch name:

- `feat/*` or `feature/*` → type = Feature
- `fix/*`, `bugfix/*`, `hotfix/*` → type = Bug
- `chore/*`, `docs/*`, `refactor/*`, `test/*`, `perf/*` → type = Chore

Notes:
- In addition to conventional prefixes, LoTaR also matches configured type names against the branch prefix: if your config lists a type like `Research`, a branch like `research-123` or `research/123` (case-insensitive; supports `-`, `_`, `/`) will infer that type when allowed by your config.
- If the inferred type isn’t allowed by your config’s `issue.types`, it’s ignored and the default type resolved from your configuration (following precedence) is used.
- Inference only runs inside a git repository with a readable HEAD; otherwise default behavior applies.

Configuration toggle:
- Set `auto.branch_infer_type: false` to disable branch-based type inference.
	- Default is `true`.

Status and priority can also be inferred using branch alias maps when enabled. LoTaR inspects the first segment of the branch (before the first `/`) and looks it up in configured alias maps:

- `branch.status_aliases`: Maps a token to a status from `issue.states` (e.g., `wip: InProgress`)
- `branch.priority_aliases`: Maps a token to a priority from `issue.priorities` (e.g., `hotfix: Critical`)

Behavior and toggles:
- Set `auto.branch_infer_status: true` to enable status inference from `branch.status_aliases`.
- Set `auto.branch_infer_priority: true` to enable priority inference from `branch.priority_aliases`.
- Project-level alias maps override/extend global ones; keys are matched case-insensitively (normalized to lowercase).
- Inference only applies when the inferred value is valid per your config; otherwise the normal defaults apply.

See `lotar config help` for how to configure alias maps.

## Examples by Use Case

### Development Team
```bash
# Feature development
lotar add "Implement dark mode" --type=feature --assignee=alice@company.com --due=2025-08-15

# Bug reporting
lotar add "Login page crashes on mobile" --bug --critical --tag=mobile --tag=urgent

# Technical debt
lotar add "Refactor authentication module" --type=chore --effort=3d
```

### Project Management
```bash
# Epic planning
lotar add "User Management System" --epic --field=story_points=21 --field=quarter=Q3

# Sprint tasks
lotar add "Design user profile page" --field=sprint=15 --field=team=frontend
```

### Personal Productivity
```bash
# Simple todo
lotar add "Review pull requests"

# With deadline
lotar add "Prepare presentation slides" --due=friday --effort=2h
```
