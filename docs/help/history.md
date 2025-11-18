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
- Project scope defaults to the current repo or configured `default_project`. Pass `--project` (or the fully qualified ID) when multiple prefixes coexist.
- Outside a git repository these commands return an error.

## Commands

### history
Show the git commit log for the task file (author, email, ISO-8601 timestamp, subject, commit SHA).

```bash
lotar task history 12
lotar --format json task history 12 -L 50
```

Options/behavior:
- `-L/--limit` (default `20`) caps the number of commits scanned.
- Text output prints each commit on its own line; JSON output emits `{status, action, id, project, count, items[]}`.
- When no commits exist for the file you will see `No history for this task.`

### diff
Show the raw unified patch for the latest (or specified) commit touching the task file.

```bash
lotar task diff 12
lotar task diff 12 --commit abcdef1
```

Flags:
- `--commit <sha>` pins the comparison to a specific commit; otherwise the latest commit for the task file is used.
- `--fields` emits a structured YAML-aware delta covering `title`, `status`, `priority`, `task_type`, `assignee`, `reporter`, `due_date`, `effort`, `tags`, `description`, `relationships`, `custom_fields`, and `sprints`. The command loads the selected commit and its parent; when one side is missing it falls back to dumping the JSON object for inspection.
- Raw patch mode prints the git diff exactly; JSON output always wraps the payload with `{status, action, id, project, commit, patch}`.

### history-by-field
Trace changes to a single field by comparing YAML snapshots across commit history.

```bash
lotar task history-by-field status 12 -L 20
lotar task history-field tags 12
```

Details:
- `history-by-field` also responds to the alias `history-field`.
- `-L/--limit` (default `20`) caps the number of detected changes after diffing adjacent snapshots.
- Supported fields: `status`, `priority`, `assignee`, `tags` (matches the `HistoryField` enum in the CLI).
- Text output prints each change as JSON on its own line; JSON mode returns `{status, action:"task.history_field", field, count, items[]}`.

### at
Show the task file snapshot at a specific commit.

```bash
lotar task at 12 abcdef1
```

Returns the full YAML for that version. JSON mode embeds it under `content` along with the task/project identifiers.

## JSON Examples

History:
```json
{
  "status": "ok",
  "action": "task.history",
  "items": [
    {
      "commit": "abc123",
      "author": "Alice",
      "email": "alice@example.com",
      "date": "2025-08-01T10:00:00Z",
      "message": "edit"
    }
  ]
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

History-by-field:
```json
{
  "status": "ok",
  "action": "task.history_field",
  "field": "status",
  "id": "PROJ-12",
  "project": "PROJ",
  "count": 2,
  "items": [
    { "field": "status", "old": "Todo", "new": "InProgress", "commit": "abc123" },
    { "field": "status", "old": "InProgress", "new": "Done", "commit": "def456" }
  ]
}
```
