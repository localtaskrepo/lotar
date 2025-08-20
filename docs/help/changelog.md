# Changelog

Show recent changes to tasks from git history.

## Usage

```bash
lotar changelog            # compare working tree vs HEAD in .tasks
lotar changelog HEAD~1     # compare HEAD vs HEAD~1 (range: HEAD~1..HEAD)
lotar --format json changelog
```

- Shows per-task field changes:
  - Default: working tree (incl. staged) vs HEAD.
  - With a ref: changes between <ref>..HEAD.
- Outside a git repository it prints a notice and returns empty.

## JSON Example

```json
{
  "status": "ok",
  "action": "changelog",
  "mode": "working",
  "count": 2,
  "items": [
    {
      "id": "AUTH-1",
      "project": "AUTH",
      "file": ".tasks/AUTH/1.yml",
      "changes": [
        {"field": "status", "old": "In Progress", "new": "Done"},
        {"field": "due_date", "old": "2025-08-20", "new": "2025-08-22"}
      ]
    }
  ]
}
```
