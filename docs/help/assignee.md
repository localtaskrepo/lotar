# lotar assignee

**⚠️ PLACEHOLDER IMPLEMENTATION - Not fully functional yet**

Change or view task assignee.

## Current Status

This command is currently a placeholder implementation that only displays a warning message. It does not actually modify task data.

## Planned Usage

```bash
# View current assignee (planned)
lotar assignee <TASK_ID>

# Change assignee (planned)  
lotar assignee <TASK_ID> <NEW_ASSIGNEE>
```

## Workaround

Use the full task interface for now:
```bash
# Use the task edit interface instead
lotar task edit <TASK_ID> --assignee=<NEW_ASSIGNEE>
```

## Implementation Status

- [ ] Command parsing ✅ (implemented)
- [ ] Actual assignee modification ❌ (placeholder only)
- [ ] Validation ❌ (not implemented)
- [ ] Output formatting ❌ (placeholder only)
lotar assignee AUTH-001 team@example.com --format=json
```

## Assignee Format

Assignees can be specified in various formats:
- Email addresses: `john.doe@example.com`
- Usernames: `john.doe`
- Display names: `John Doe`
- Team aliases: `@frontend-team`

## Project Integration

- Works with any project structure
- Auto-detects project from task ID prefix
- Falls back to global configuration if no project context
- Supports custom assignee validation rules (project-specific)

## Alternative Interface

This command is also available through the full task interface:

```bash
lotar task assignee <TASK_ID> [NEW_ASSIGNEE]
# or using the alias
lotar tasks assignee <TASK_ID> [NEW_ASSIGNEE]
```

Both interfaces provide identical functionality.

## Output Formats

Assignee changes support all output formats:
- `text` (default): Human-readable with colors and emojis
- `json`: Machine-readable for scripts and automation  
- `table`: Clean tabular format
- `markdown`: Documentation-friendly format

## Error Handling

Common errors and solutions:

- **Task not found**: Verify task ID and project context
- **Permission denied**: Check file permissions in tasks directory
- **Invalid project**: Verify project exists and is properly configured

## See Also

- `lotar status` - Change task status
- `lotar priority` - Change task priority
- `lotar due-date` - Change task due date
- `lotar task assignee` - Full task interface
- `lotar config` - View and modify project configuration
