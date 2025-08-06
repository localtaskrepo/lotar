# lotar priority

Change or view task priority with validation.

## Usage

```bash
# View current priority
lotar priority <TASK_ID>

# Change priority
lotar priority <TASK_ID> <NEW_PRIORITY>
```

## Quick Examples

```bash
# View current priority
lotar priority AUTH-001

# Change priority to high
lotar priority AUTH-001 high

# With explicit project
lotar priority 123 critical --project=backend

# JSON output for automation
lotar priority AUTH-001 low --format=json
```

## Priority Values

Available priorities depend on your project configuration. Common defaults:

- `low` - Low priority task
- `medium` - Medium priority task (usually default)
- `high` - High priority task
- `critical` - Critical priority task requiring immediate attention

Check your project's valid priorities:
```bash
lotar config show priorities
```

## Project Integration

- Respects project-specific priority configurations
- Validates priority values against project settings
- Auto-detects project from task ID prefix
- Falls back to global configuration if no project context

## Alternative Interface

This command is also available through the full task interface:

```bash
lotar task priority <TASK_ID> [NEW_PRIORITY]
# or using the alias
lotar tasks priority <TASK_ID> [NEW_PRIORITY]
```

Both interfaces provide identical functionality.

## Output Formats

Priority changes support all output formats:
- `text` (default): Human-readable with colors and emojis
- `json`: Machine-readable for scripts and automation  
- `table`: Clean tabular format
- `markdown`: Documentation-friendly format

## Error Handling

Common errors and solutions:

- **Invalid priority**: Check valid priorities with `lotar config show priorities`
- **Task not found**: Verify task ID and project context
- **Permission denied**: Check file permissions in tasks directory
- **Invalid project**: Verify project exists and is properly configured

## See Also

- `lotar status` - Change task status
- `lotar assignee` - Change task assignee  
- `lotar due-date` - Change task due date
- `lotar task priority` - Full task interface
- `lotar config` - View and modify project configuration
