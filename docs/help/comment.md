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

- `-F -` reads the comment from stdin. A file that can't be read is an error (exit 1), never a silent listing.
- If neither TEXT, -m, nor -F are provided, the command reads from stdin when piped and data arrives within a moment; otherwise (a terminal, or a pipe that stays silent, as in many agent and CI harnesses) it lists existing comments for the task right away.
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
    "author": "alice",
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

- Each comment stores a UTC timestamp (RFC3339), its author and the text. The author is the identity the history records as actor (`default_reporter` such as `LOTAR_DEFAULT_REPORTER`, then git or system user; see [Identity & Users](./identity.md)). Comments written before lotar stored authors have no `author` and show without one.
- `lotar comment <ID>` and `lotar show` print the author before the text (`2025-08-19T10:30:42Z  alice: First note`).
- Project context is inferred from the numeric ID using your current repo (or configured `default_project`). Pass a fully-qualified ID (`AUTH-1`) or `--project` when multiple prefixes exist.
