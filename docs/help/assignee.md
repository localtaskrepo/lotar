# lotar assignee

Change or view a task's assignee with validation and project-aware ID resolution.

## Usage

```bash
# View current assignee
lotar assignee <TASK_ID>

# Change assignee
lotar assignee <TASK_ID> <NEW_ASSIGNEE>
```

## Examples

```bash
# Get current assignee
lotar assignee 1

# Set to yourself (resolved via identity detection)
lotar assignee 42 @me

# Set using @username with explicit project for numeric IDs
lotar assignee 42 @john_doe --project=AUTH

# JSON output for automation
lotar assignee 42 jane@example.com --format=json
```

## Accepted assignee formats

- Email: john.doe@example.com
- Username: @john_doe (letters, numbers, underscore, dash)
- Special: @me (resolved to your detected identity)

Use `lotar whoami --explain` to see how identity is resolved for @me.

## Task ID resolution

- Full IDs like AUTH-123 are used as-is
- Numeric IDs like 123 are resolved against the auto-detected project (current repo or configured `default_project`)
- Override the project with `--project` when multiple prefixes coexist or when the number alone would be ambiguous
- If both the ID prefix and `--project` are given, they must refer to the same project

## JSON output shapes

- Get current assignee (`lotar assignee <ID> --format=json`):
```json
{
  "status": "success",
  "task_id": "AUTH-001",
  "assignee": "john.doe@example.com"
}
```
When no assignee is set: `"assignee": null`.

- Set new assignee (`lotar assignee <ID> <ASSIGNEE> --format=json`):
```json
{
  "status": "success",
  "message": "Task AUTH-001 assignee changed from john.doe@example.com to jane@example.com",
  "task_id": "AUTH-001",
  "old_assignee": "john.doe@example.com",
  "new_assignee": "jane@example.com"
}
```

## Notes

- Validation rejects invalid usernames and emails.
- @me is resolved to a concrete identity when saving.
- Works across multi-project workspaces; resolution is project-aware.

## Alternative interface

This is also available under the task umbrella:

```bash
lotar task assignee <TASK_ID> [NEW_ASSIGNEE]
```

## See also

- `lotar whoami` – Show resolved identity
- `lotar status` – Change task status
- `lotar priority` – Change task priority
- `lotar due-date` – Change task due date
