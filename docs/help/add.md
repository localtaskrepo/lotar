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
- `--category <CATEGORY>`, `-c <CATEGORY>` - Project category
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
- **Categories**: Validated against `categories` config
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
4. **Auto-detection**: Based on current directory

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
