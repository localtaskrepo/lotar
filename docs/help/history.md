```markdown
# lotar task history / diff / at

Read-only history views powered by git. LoTaR never writes to git automatically.

## Usage

```bash
lotar task history <TASK_ID> [-L N]
lotar task history-by-field <status|priority|assignee|tags> <TASK_ID> [-L N]
lotar task diff <TASK_ID> [--commit <sha>] [--fields]
lotar task at <TASK_ID> <commit>
```

Notes:
- Project scope defaults to the current project; you can still pass `--project` to disambiguate.
- Outside a git repository these commands return an error.

## Commands

### history
Show the commit log for the task file.

Outputs author, email, date, short subject, and commit SHA.

```bash
lotar task history AUTH-123
lotar --format json task history AUTH-123 -L 50
```

### diff
Show the raw unified patch for the latest (or specified) commit touching the task file.

With `--fields`, emit a structured YAML-aware delta for common fields (title, status, priority, type, assignee, due_date, effort, tags). Falls back to raw patch by default when not requested.

```bash
lotar task diff AUTH-123
lotar task diff AUTH-123 --commit abcdef1
```

### history-by-field
Trace changes to a single field by comparing YAML snapshots across commit history.

```bash
lotar task history-by-field status AUTH-123 -L 20
```
Fields supported: status, priority, assignee, tags.

### at
Show the task file snapshot at a specific commit.

```bash
lotar task at AUTH-123 abcdef1
```

## JSON Examples

History:
```json
{
  "status": "ok",
  "action": "task.history",
  "items": [ { "commit": "abc123", "author": "Alice", "email": "alice@example.com", "date": "2025-08-01T10:00:00Z", "message": "edit" } ]
}
```

Diff:
```json
{ "status": "ok", "action": "task.diff", "commit": "abc123", "patch": "diff --git ..." }
```

At:
```json
{ "status": "ok", "action": "task.at", "commit": "abc123", "content": "title: One\n..." }
```

```
