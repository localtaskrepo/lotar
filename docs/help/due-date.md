# lotar due-date

**⚠️ PLACEHOLDER IMPLEMENTATION - Not fully functional yet**

Change or view task due date.

## Current Status

This command is currently a placeholder implementation that only displays a warning message. It does not actually modify task data.

## Planned Usage

```bash
# View current due date (planned)
lotar due-date <TASK_ID>

# Change due date (planned)
lotar due-date <TASK_ID> <NEW_DATE>
```

## Workaround

Use the full task interface for now:
```bash
# Use the task edit interface instead
lotar task edit <TASK_ID> --due=<NEW_DATE>
```

## Implementation Status

- [ ] Command parsing ✅ (implemented)
- [ ] Actual due date modification ❌ (placeholder only)
- [ ] Date validation ❌ (not implemented)  
- [ ] Output formatting ❌ (placeholder only)

# JSON output for automation
lotar due-date AUTH-001 2025-09-01 --format=json
```

## Date Formats

Supported date formats:
- **ISO format**: `2025-12-31`, `2025-08-15`
- **Relative**: `tomorrow`, `next week`, `next friday`
- **Offset**: `+1 day`, `+2 weeks`, `+1 month`
- **Natural**: `"in 3 days"`, `"next Monday"`

## Project Integration

- Works with any project structure
- Auto-detects project from task ID prefix
- Falls back to global configuration if no project context
- Supports custom date validation rules (project-specific)

## Alternative Interface

This command is also available through the full task interface:

```bash
lotar task due-date <TASK_ID> [NEW_DATE]
# or using the alias
lotar tasks due-date <TASK_ID> [NEW_DATE]
```

Both interfaces provide identical functionality.

## Output Formats

Due date changes support all output formats:
- `text` (default): Human-readable with colors and emojis
- `json`: Machine-readable for scripts and automation  
- `table`: Clean tabular format
- `markdown`: Documentation-friendly format

## Error Handling

Common errors and solutions:

- **Invalid date format**: Use ISO format (YYYY-MM-DD) or supported relative formats
- **Past dates**: Some projects may not allow due dates in the past
- **Task not found**: Verify task ID and project context
- **Permission denied**: Check file permissions in tasks directory
- **Invalid project**: Verify project exists and is properly configured

## Integration Features

- **Overdue detection**: Use `lotar list --overdue` to find tasks past due date
- **Date filtering**: Use `lotar list --due-date=today` for date-based filtering
- **Sorting**: Use `lotar list --sort-by=due_date` to sort by due dates

## See Also

- `lotar status` - Change task status
- `lotar priority` - Change task priority
- `lotar assignee` - Change task assignee
- `lotar task due-date` - Full task interface
- `lotar list` - List tasks with date filtering
- `lotar config` - View and modify project configuration
