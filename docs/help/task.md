# lotar task

Legacy multi-action entry point that wraps the modern task commands for backward compatibility.


## Invocation

```bash
lotar task <ACTION> [OPTIONS]
lotar tasks <ACTION> [OPTIONS]   # alias
```

- All global flags (`--project/-p`, `--tasks-dir`, `--format`, `--log-level`, `--verbose`) behave exactly as they do for the top-level commands because this mode reuses the same workspace/project detection logic.
- Running `lotar task` (or `lotar tasks`) with no action prints the available subcommands via `emit_subcommand_overview`.
- Numeric task IDs are expanded to full IDs using the same project detection chain described in `docs/help/precedence.md`.

## Actions at a glance

| Action | Purpose | Shares handler with |
|--------|---------|---------------------|
| `add` | Create a task with the core fields. | `lotar add`. |
| `list` | Search/filter tasks with pagination helpers. | `lotar list`. |
| `edit` | Modify an existing task, optionally previewing changes. | Same pipeline as `lotar add`. |
| `status` | Change a task status (write-only in legacy mode). | `lotar status`. |
| `priority` | Show or update priority. | `lotar priority`. |
| `assignee` | Show or update assignee. | `lotar assignee`. |
| `due-date` | Show or update due date. | `lotar due-date`. |
| `relationships` | List dependency/parent/child links. | Relationships helper. |
| `effort` | Show, set, or clear effort estimates. | `lotar effort`. |
| `delete` | Remove a task (with optional dry-run). | Delete helper. |
| `history` / `history-field` | Git history utilities. | See `docs/help/history.md`. |
| `diff` | Show the latest (or specific) git diff for a task. | History/diff helper. |
| `at` | Print the file at a specific commit. | History helper. |
| `comment` | Add or list comments for a task. | `lotar comment`. |

Each action below highlights the legacy-specific nuances; for behavior details see the dedicated help pages linked in the rightmost column.

## Action details

### add

```bash
lotar task add --title "Task title" [--type|-t] [--priority|-P] [--reporter|-R] [--assignee|-a] [--due|-d] [--effort|-E] [--description|-D] [--tag|-i ...] [--field|-F key=value ...]
```

- Uses the same pipeline as `lotar add`, so status/priority/type defaults, auto-tags, branch inference, and validation all match the dedicated command.
- Legacy mode only exposes the core fields. Flags such as `--dry-run`, `--explain`, `--bug`, `--epic`, `--critical`, and `--high` live exclusively on the top-level `lotar add` command.

### list

```bash
lotar task list [QUERY] [--assignee|-a <VALUE>] [--mine|-m] [--status|-s <VALUE>...] [--priority|-P <VALUE>...] [--type|-t <VALUE>...] [--tag|-i <VALUE>...] [--high|-H] [--critical|-C] [--sort-by|-S FIELD] [--reverse|-R] [--limit|-L N] [--overdue] [--due-soon[=DAYS]] [--where key=value ...] [--effort-min VALUE] [--effort-max VALUE]
```

- Mirrors the filters documented in `docs/help/list.md`, including `@me`/`--mine`, overdue/due-soon windows, unified `--where` filters, and effort bounds.
- Sorting accepts `priority|due-date|created|modified|status|assignee|type|project|id` and defaults to 20 results per page.

### edit

```bash
lotar task edit <TASK_ID> [--title|-T] [--type|-t] [--priority|-P] [--reporter|-R] [--assignee|-a] [--effort|-E] [--due|-d] [--description|-D] [--tag|-i ...] [--field|-F key=value ...] [--dry-run|-n]
```

- Invokes the same mutation pipeline as `lotar add`, so validation, normalization, and `@me` handling match the top-level commands.
- `--dry-run` works with both text and JSON output (`--format=json` emits the preview envelope described in `docs/help/effort.md`).

### status

```bash
lotar task status <TASK_ID> <NEW_STATUS>
```

- This alias always supplies a new status and therefore writes immediately; use `lotar status` if you need to inspect the current status or run `--dry-run/--explain`.
- See `docs/help/status.md` for validation, auto-assign semantics, and output formats.

### priority

```bash
lotar task priority <TASK_ID> [NEW_PRIORITY]
```

- Without `NEW_PRIORITY` it prints the current value; otherwise it reuses `PriorityHandler` to validate and update the task.
- Refer to `docs/help/priority.md` for supported values and renderer output.

### assignee

```bash
lotar task assignee <TASK_ID> [NEW_ASSIGNEE]
```

- Accepts raw emails, usernames, or the `@me` token (resolved via the identity chain in `docs/help/precedence.md`).
- Legacy mode does not expose `--dry-run` or `--explain`; use `lotar assignee` if you need a preview.

### due-date

```bash
lotar task due-date <TASK_ID> [NEW_DATE]
```

- `NEW_DATE` accepts absolute (`YYYY-MM-DD`) or relative values (`tomorrow`, `+3d`, etc.) as described in `docs/help/due-date.md`.
- Preview/explain flags are only available via the top-level `lotar due-date` command.

### relationships

```bash
lotar task relationships <TASK_ID> [--kind <KIND> ...]
```

- `--kind` accepts any combination of `depends-on`, `blocks`, `related`, `parent`, `children`, `fixes`, `duplicate-of` (case-insensitive) and filters the output accordingly.
- Loads relationships from storage and renders them in text or JSON form.

### effort

```bash
lotar task effort <TASK_ID> [NEW_EFFORT] [--clear] [--dry-run] [--explain]
```

- Wraps the primary `lotar effort` command, so it supports previews, explanations, and clearing effort values. See `docs/help/effort.md` for accepted formats (`2d`, `5h`, point counts, etc.).

### delete

```bash
lotar task delete <TASK_ID> [--dry-run] [--force | --yes | -y]
```

- `--force`, `--yes`, and `-y` all skip the confirmation prompt. `--dry-run` provides a JSON/text preview of the record that would be removed.

### Git history helpers (`history`, `history-field`, `diff`, `at`)

- `lotar task history <TASK_ID> [--limit|-L N]` - shows the git commit log touching the task file.
- `lotar task history-field <FIELD> <TASK_ID> [--limit|-L N]` - alias for `history-by-field` that traces only `status`, `priority`, `assignee`, or `tags` changes.
- `lotar task diff <TASK_ID> [--commit <SHA>] [--fields]` - prints the latest raw patch (or a specific commit). `--fields` switches to the structured renderer that lists per-field changes.
- `lotar task at <TASK_ID> <COMMIT>` - dumps the task file snapshot at `git show COMMIT`. All behaviors match `docs/help/history.md`.

### comment

```bash
lotar task comment <TASK_ID> [TEXT] [--message|-m TEXT] [--file|-F PATH] [--dry-run|-n] [--explain|-e]
```

- Shares `CommentHandler` with `lotar comment`: TEXT can come from the positional argument, `--message`, `--file`, or stdin. Without comment content the command lists existing comments.
- See `docs/help/comment.md` for JSON payloads and formatting rules.

## Tips and examples

```bash
# Quick legacy flows
lotar tasks list --project=backend --mine --due-soon=5
lotar task edit AUTH-42 --priority=High --tag backend --tag api --dry-run --format=json
lotar task history AUTH-42 --limit=5
lotar tasks comment AUTH-42 -m "Ready for review" --dry-run
```

- Prefer the direct commands (`lotar add`, `lotar status`, etc.) when you need the newest flags or shorthands. `lotar task` remains supported for scripts that rely on the legacy shape.
- All actions continue to respect identity resolution, workspace detection, and configuration precedence exactly as described in `docs/help/precedence.md`.
