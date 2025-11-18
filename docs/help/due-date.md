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
lotar due-date 1

# Set to today/tomorrow/next week
lotar due-date 7 today
lotar due-date 7 tomorrow
lotar due-date 7 "next week"

# Set an explicit date (YYYY-MM-DD)
lotar due-date 7 2025-09-01 --project=AUTH

# JSON output for automation
lotar due-date 1 2025-09-01 --format=json
```

## Supported date formats

- ISO Date: `YYYY-MM-DD` (interpreted as local midnight, stored in UTC)
- RFC3339 DateTime: `2025-12-31T15:04:05Z`, `2025-12-31T15:04:05+02:00`
- Local DateTime (assumed local tz): `YYYY-MM-DD HH:MM[:SS]`, `YYYY-MM-DDTHH:MM[:SS]`
- Relative keywords: `today`, `tomorrow`, `next week`, `next <weekday>`
- Offsets: `+3d`, `+2w`, `+1 day`, `+2 weeks`, `in 3 days`, `in 2 weeks`
- Business days: `+1bd`, `+3 business days`, `next business day`
- Weekday shortcuts: `this friday`, `by friday`, `fri`, `next week monday`

## Task ID resolution

- Full IDs like AUTH-123 are used as-is
- Numeric IDs like 123 use the auto-detected or configured default project
- Add `--project` (or pass the full ID) in multi-project workspaces so the CLI can disambiguate
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

- If a date is provided without a time or timezone, midnight local time is assumed and stored as an RFC3339 UTC timestamp.
- Validation checks format and supports only the formats listed above. Error messages include hints with examples.
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
