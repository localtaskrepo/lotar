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

# Ask a question with options; answer it
lotar comment <TASK_ID> --question <TEXT> --option "<key>: <label>"... [--set "<key>:<field>=<value>"...] [--recommend <key>] [--multiple] [--no-other]
lotar comment <TASK_ID> --answer [<keys>] [--to <DATE>] [NOTE]
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

## Questions and answers

A question comment carries options the reader answers by choosing one, in the web UI by clicking (the Comments tab, board cards and the Tasks list show open questions) or with `--answer`. Use it instead of asking "A, B or C?" in prose.

```bash
# Ask; choosing "fix" assigns the ticket to the @fix agent and reopens it
lotar comment AUTH-1 --question "Rework or close?" \
  --option "fix: Rework with @fix" --set "fix:assignee=@fix" --set "fix:status=Todo" \
  --option "close: Close as WontDo" --set "close:status=WontDo" \
  --recommend fix

# Answer the last open question by its key, with a note
lotar comment AUTH-1 --answer fix "Keep the old endpoint"

# Answer in your own words instead of an option
lotar comment AUTH-1 --answer -- "Ask me again next week"

# Tasks with an open question
lotar list --has question
```

- `--option "<key>: <label>"` adds an option; keys are letters, digits, `-` and `_`. `--recommend <key>` marks one, `--multiple` lets the reader choose several (`--answer a,b`), `--no-other` allows only the options.
- `--set "<key>:<field>=<value>"` sets a field when that option is chosen, as an automation `set:` does: `status`, `assignee`, `reporter`, `priority`, `type`, `due_date`, `effort` or `tags` (comma-separated; replaces the tags). Options only set fields; they never run commands or start anything else. Unknown fields and values the project does not allow are refused when you ask.
- `--answer <keys>` answers the newest open question, or the one asked at `--to <DATE>` (its comment date, or a unique start of it). Without keys, the comment text is a free-text answer. A question is answered once.
- The answer applies the chosen options' fields as an update by the answerer, recorded in history under their name; `on.updated`/`on.assigned` rules fire as for any change, then `on.commented` rules see the answer (`comment: { answer: fix }`, `${{comment.choice}}`; see [Automation](./automation.md#comment-conditions)).
- The answer and its option fields are saved together. A rejected field update leaves the question open so you can retry.
- The comment's `text` keeps the question with its options, or the answer with its labels, in prose, so `lotar show`, older lotar versions and agents reading the YAML see everything. `lotar comment <ID>` and `lotar show` mark a question no answer points to as `[open question]`.
- The stored comment adds `question: { options: [{key, label, set?}], recommended?, multiple?, other? }` or `answer: { to, choice?, note? }`; REST (`POST /api/tasks/comment`) and MCP (`task_comment_add`) take the same `question` and `answer` fields.

## Notes

- Each comment stores a UTC timestamp (RFC3339), its author and the text. The author is the identity the history records as actor (`default_reporter` such as `LOTAR_DEFAULT_REPORTER`, then git or system user; see [Identity & Users](./identity.md)). Comments written before lotar stored authors have no `author` and show without one.
- `lotar comment <ID>` and `lotar show` print the author before the text (`2025-08-19T10:30:42Z  alice: First note`).
- Project context is inferred from the numeric ID using your current repo (or configured `default_project`). Pass a fully-qualified ID (`AUTH-1`) or `--project` when multiple prefixes exist.
