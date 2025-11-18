# lotar comment

Add or list comments for a task. Supports positional text, message flag, file input, or stdin. If no text is provided, the command lists existing comments.

## Usage

```bash
lotar comment <TASK_ID> [TEXT]
lotar comment <TASK_ID> -m|--message <TEXT>
lotar comment <TASK_ID> -F|--file <PATH>

# Alias
lotar c <TASK_ID> [TEXT]

# Alternate form
lotar task comment <TASK_ID> [TEXT]
```

- If neither TEXT, -m, nor -F are provided, the command reads from stdin when piped; otherwise it lists existing comments for the task.
- TEXT is optional when using -m/--message or -F/--file.

## Examples

```bash
# Positional text
lotar comment 1 "Investigated and will fix tomorrow"

# Message flag (shell-safe)
lotar comment 1 -m "Re-tested with latest build; still failing"

# From file
lotar comment 1 -F notes/update.txt

# From stdin (multiline)
printf "Line 1\nLine 2\n" | lotar comment 1

# JSON output
lotar --format json comment 1 -m "First note"

# List existing comments (no text provided)
lotar comment 1
lotar --format json comment 1
lotar task comment 1
```

Example JSON response:
```json
{
  "status": "success",
  "action": "task.comment",
  "task_id": "AUTH-1",
  "comments": 3,
  "added_comment": {
    "date": "2025-08-19T10:30:42Z",
    "text": "First note"
  }
}
```

Example JSON list response (when no text is provided):
```json
{
  "status": "ok",
  "action": "task.comment.list",
  "task_id": "AUTH-1",
  "comments": 0,
  "items": []
}
```

## Notes

- Author attribution isn’t stored in the comment; use git blame/history to see who added or changed comments.
- Each comment stores a UTC timestamp (RFC3339) and the text.
- Project context is inferred from the numeric ID using your current repo (or configured `default_project`). Pass a fully-qualified ID (`AUTH-1`) or `--project` when multiple prefixes exist.
