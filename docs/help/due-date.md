# lotar due-date

Change or view a task's due date with validation and project-aware ID resolution.

## Usage

```bash
# View current due date
lotar due-date <TASK_ID>

# Change due date
lotar due-date <TASK_ID> <NEW_DATE>
```

## Examples

```bash
# Get current due date
lotar due-date AUTH-001

# Set to today/tomorrow/next week
lotar due-date 7 today
lotar due-date 7 tomorrow
lotar due-date 7 "next week"

# Set an explicit date (YYYY-MM-DD)
lotar due-date 7 2025-09-01 --project=AUTH

# JSON output for automation
lotar due-date AUTH-001 2025-09-01 --format=json
```

## Supported date formats

- ISO: YYYY-MM-DD (e.g., 2025-12-31)
- Relative: today, tomorrow, next week, next monday (any weekday)
- Offsets: +3d, +2w, +1 day, +2 weeks

## Task ID resolution

- Full IDs like AUTH-123 are used as-is
- Numeric IDs like 123 use the default project; override with `--project`
- If both the ID prefix and `--project` are given, they must refer to the same project

## JSON output shapes

- Get current due date (`lotar due-date <ID> --format=json`):
```json
{
  "status": "success",
  "task_id": "AUTH-001",
  "due_date": "2025-09-01"
}
```
When no due date is set: `"due_date": null`.

- Set new due date (`lotar due-date <ID> <DATE> --format=json`):
```json
{
  "status": "success",
  "message": "Task AUTH-001 due date updated",
  "task_id": "AUTH-001",
  "old_due_date": "2025-08-15",
  "new_due_date": "2025-09-01"
}
```

## Notes

- Validation checks format and supports only the formats listed above.
- Works across multi-project workspaces; resolution is project-aware.

## Alternative interface

This is also available under the task umbrella:

```bash
lotar task due-date <TASK_ID> [NEW_DATE]
```

## See also

- `lotar status` – Change task status
- `lotar priority` – Change task priority
- `lotar assignee` – Change task assignee
