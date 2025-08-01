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
lotar add "Add OAuth support" --type=feature --priority=high --assignee=john.doe

# Bug with due date
lotar add "Fix login crash" --bug --critical --due=tomorrow

# Epic with custom fields
lotar add "User Management System" --epic --field=story_points=13 --field=sprint=2
```

## Options

### Core Properties
- `--type <TYPE>` - Task type: feature, bug, epic, spike, chore
- `--priority <PRIORITY>` - Priority: low, medium, high, critical
- `--assignee <ASSIGNEE>` - Task assignee (email or @username)

### Scheduling
- `--due <DATE>` - Due date (YYYY-MM-DD or relative like 'tomorrow')
- `--effort <ESTIMATE>` - Effort estimate (e.g., 2d, 5h, 1w)

### Organization  
- `--category <CATEGORY>` - Project category
- `--tag <TAG>` - Tags (can be used multiple times)
- `--description <TEXT>` - Detailed description

### Shortcuts
- `--bug` - Shorthand for --type=bug
- `--epic` - Shorthand for --type=epic
- `--critical` - Shorthand for --priority=critical
- `--high` - Shorthand for --priority=high

### Custom Fields
- `--field <KEY>=<VALUE>` - Arbitrary properties (can be used multiple times)

## Output Formats

Control output with global `--format` option:

```bash
# Human-readable (default)
lotar add "New task" 
# 📋 Created task AUTH-001: New task

# JSON for scripting
lotar add "New task" --format=json
# {"task_id": "AUTH-001", "status": "created"}

# Table format
lotar add "New task" --format=table
# | Property | Value    |
# |----------|----------|
# | Task ID  | AUTH-001 |
# | Status   | Created  |
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
lotar config set issue_types feature,bug,chore
lotar config set issue_priorities low,medium,high,critical
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
