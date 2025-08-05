# lotar set

Set arbitrary properties on existing tasks.

## Usage

```bash
lotar set <TASK_ID> <PROPERTY> <VALUE>
```

## Examples

```bash
# Set assignee
lotar set PROJ-123 assignee john.doe@company.com

# Set priority
lotar set PROJ-123 priority high

# Set custom field
lotar set PROJ-123 story_points 8

# Set description
lotar set PROJ-123 description "Updated task description"
```

## Arguments

- `<TASK_ID>` - Task identifier (e.g., PROJ-123)
- `<PROPERTY>` - Property name to set
- `<VALUE>` - New value for the property

## Common Properties

- `assignee` - Task assignee (email or username)
- `priority` - Task priority (low, medium, high, critical)
- `status` - Task status (todo, in_progress, verify, blocked, done)
- `description` - Task description
- `due_date` - Due date (YYYY-MM-DD format)
- `category` - Task category
- `effort` - Effort estimate (e.g., 2d, 4h, 1w)

## Notes

- For status changes, consider using `lotar status` command instead
- Property names are case-sensitive
- Values with spaces should be quoted
- Task ID can include or omit project prefix
