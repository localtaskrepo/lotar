# lotar list (aliases: ls, l)

Display tasks with filtering and multiple output formats.

## Usage

```bash
lotar list [OPTIONS]
# or
lotar ls [OPTIONS]
# or
lotar l [OPTIONS]
```

## Quick Examples

```bash
# All tasks (default text format)
lotar list

# Specific project
lotar list -p auth

# Filter by status
lotar list -s todo -s in_progress

# Table output
lotar list -f table

# JSON for scripting
lotar list -f json

# Custom tasks directory
lotar list --tasks-dir=/custom/path -p auth

# Environment variable usage
export LOTAR_TASKS_DIR=/project/tasks
lotar list --project=auth  # Uses environment directory
```

## Filtering Options

### Status Filtering
- `--status, -s <STATUS>` - Filter by task status (can be used multiple times)
- Valid statuses depend on project configuration

### Priority Filtering  
- `--priority, -P <PRIORITY>` - Filter by priority level
- `--high, -H` - Show only HIGH priority tasks
- `--critical, -C` - Show only CRITICAL priority tasks

### Type Filtering
- `--type, -t <TYPE>` - Filter by task type
- `--bugs` - Show only bug tasks
- `--features` - Show only feature tasks

### Assignment & Due Dates
- `--assignee, -a <ASSIGNEE>` - Tasks assigned to specific person (accepts @me)
- `--unassigned` - Tasks with no assignee
 - Tip: Use `--assignee=@me` or `--mine, -m` to filter to your tasks. Your identity resolves from config default_reporter → git user → system username.
- `--overdue` - Tasks past their due date (strictly before now)
- `--due-soon[=N]` - Tasks due within N days (default 7)

### Project & Organization
- `--project, -p <PROJECT>` - Specific project (overrides auto-detection)
- `--tag, -i <TAG>` - Tasks with specific tag (can be used multiple times)

### Text Search
- `--search <QUERY>` - Search in title and description
- `--title-only` - Search only in task titles

### Global Options
- `--format, -f <FORMAT>` - Output format: text, table, json, markdown
- `--verbose, -v` - Enable verbose output
- `--tasks-dir <PATH>` - Custom tasks directory (overrides environment/config)

### Environment Variables
- `LOTAR_TASKS_DIR` - Default tasks directory location

## Output Formats

### Text (Default)
Human-readable with colors and emojis:
```
📋 Implement OAuth [feature] - HIGH (TODO) - 👤 john.doe
🚧 Fix login bug [bug] - CRITICAL (IN_PROGRESS) - 📅 2025-08-15
✅ Setup CI/CD [chore] - MEDIUM (DONE)
```

### Table
Structured terminal output:
```
| ID       | Title              | Status      | Priority | Type    | Assignee | Due Date   |
|----------|--------------------|-----------  |----------|---------|----------|------------|
| AUTH-001 | Implement OAuth    | TODO        | HIGH     | feature | john.doe | 2025-08-20 |
| AUTH-002 | Fix login bug      | IN_PROGRESS | CRITICAL | bug     | alice    | 2025-08-15 |
```

### JSON
Machine-readable for automation:
```json
[
  {
    "id": "AUTH-001",
    "title": "Implement OAuth",
    "status": "TODO", 
    "priority": "HIGH",
    "task_type": "feature",
    "assignee": "john.doe",
    "due_date": "2025-08-20",
    "tags": ["auth", "security"]
  }
]
```

### Markdown
Documentation-friendly tables:
```markdown
| ID | Title | Status | Priority | Type |
|----|-------|--------|----------|------|
| AUTH-001 | Implement OAuth | TODO | HIGH | feature |
| AUTH-002 | Fix login bug | IN_PROGRESS | CRITICAL | bug |
```

## Sorting & Grouping

### Sorting Options
- `--sort-by, -S <FIELD>` - Sort by: priority, status, effort, due-date, created, modified, assignee, type, project, id, or a declared custom field (you can also use `field:<name>`)
- `--reverse, -R` - Reverse sort order
- `--limit, -L <N>` - Limit results (default: 20)

### Grouping
- `--group-by <FIELD>` - Group by: status, priority, assignee, type
- `--show-counts` - Show task counts per group

## Advanced Filtering

### Unified filters (built-in and custom fields)
- `--where key=value` (repeatable) — filter by any property. Supported keys: assignee, status, priority, type, tag (or tags), project, and custom fields declared by your project. You can pass declared custom field names directly (e.g., `sprint=W35`) or use `field:<name>` explicitly (both work).
- Matching is fuzzy and case-insensitive for strings. For tags, matching applies to the set of tags.

Examples:
```bash
# Built-ins
lotar list --where status=todo --where priority=high

# Tags
lotar list --where tag=auth

# Custom fields declared by your project (example: sprint)
lotar list --where sprint=2025-W35
# `field:` prefix also works (legacy/explicit form)
lotar list --where field:sprint=2025-W35
```

### Effort filters
- `--effort-min <VAL>` — minimum effort (e.g., 2h, 1d, 8h, or points like 3)
- `--effort-max <VAL>` — maximum effort

Notes:
- Time is normalized to hours internally (m/h/d=8h/w=40h). Points are numeric. Mixed kinds aren’t compared: a time filter doesn’t include point-only tasks and vice versa.

### Multiple Criteria
```bash
# High priority bugs assigned to john
lotar list -t bug -P high -a john.doe

# Tasks due this week in auth project  
lotar list -p auth --due-soon

# All open tasks (not done)
lotar list -s todo -s in_progress -s blocked
```

### Complex Queries
```bash
# Overdue critical tasks
lotar list --overdue --priority=critical --format=table

# Unassigned features for sprint planning
lotar list --type=feature --unassigned --sort-by=priority

# Tasks in a sprint with effort window and custom sort
lotar list \
  --where field:sprint=2025-W35 \
  --effort-min=4h --effort-max=2d \
  --sort-by=effort
```

## Performance Notes

- Filtering is done in-memory after loading tasks
- Use `--project` to limit scope for better performance
- JSON format is fastest for scripting use cases
- Index-based search provides sub-100ms query performance

## Examples by Role

### Developers
```bash
# My current work
lotar list --assignee=$USER --status=in_progress

# Bugs to fix
lotar list --type=bug --status=todo --sort-by=priority --reverse
```

### Project Managers  
```bash
# Sprint overview
lotar list --format=table --group-by=status --show-counts

# Risk assessment
lotar list --overdue --priority=critical --format=table
```

### QA Engineers
```bash
# Tasks ready for testing
lotar list --status=verify --format=table

# High priority items
lotar list --high-priority --sort-by=due_date
```
