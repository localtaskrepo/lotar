# lotar status

Change or inspect a task's status with validation, dry-run previews, and optional auto-assignment when nobody owns the task yet.


## Usage

```bash
lotar status <TASK_ID> [<NEW_STATUS>] [--dry-run] [--explain]
```

- Omit `<NEW_STATUS>` to show the current value (the handler switches to `TaskCommandContext::new_read_only` and emits `render_property_current`).
- `lotar task status <TASK_ID> <NEW_STATUS>` reuses the same handler but always requires the new value.

## Quick Examples

```bash
# Show current status
lotar status 42

# Change the status
lotar status 42 in_progress

# Explicit project (global flag shared by all commands)
lotar status 123 verify --project=backend

# JSON output for automation
lotar status AUTH-7 done --format=json

# Custom tasks directory (matches TasksDirectoryResolver precedence)
lotar status 7 done --tasks-dir=/workspace/.tasks
export LOTAR_TASKS_DIR=/project/.tasks && lotar status 1 in_progress

# Preview with diagnostics
lotar status 42 done --dry-run --explain
```

## Flags and global options

- `-n, --dry-run` - Preview the change without writing files. Only useful when `<NEW_STATUS>` is provided.
- `-e, --explain` - Adds an explanation block to the dry-run preview (currently ignored without `--dry-run`).
- `-p, --project` - Override project detection. Accepts either a prefix (AUTH) or the human-readable project name.
- `--tasks-dir` - Override the workspace path. Shares precedence with `LOTAR_TASKS_DIR` and config defaults (see `docs/help/precedence.md`).
- `-f, --format` - `text` or `json`. `table`, `markdown`, and `md` are aliases for `text`; `jsonl` and `ndjson` alias to `json`.
- `-l, --log-level` - Set verbosity (`error`, `warn`, `info`, `debug`, `trace`). `--verbose` is still accepted and simply maps to `--log-level=info`.

## Reading the current status

With no `<NEW_STATUS>` the command loads the task in read-only mode and emits a `PropertyCurrent` payload:

```
Task AUTH-42 status: InProgress
```

JSON output is structured for automation:

```json
{
  "status": "success",
  "task_id": "AUTH-42",
  "status_value": "InProgress"
}
```

## Changing a status

Changing a status follows these stages:

1. Resolve the workspace/project and load the task.
2. Validate `<NEW_STATUS>` against the merged `issue_states` from config.
3. Detect no-op transitions and report when the status already matches.
4. Decide whether auto-assign should add an owner (when enabled) by checking CODEOWNERS defaults first and then falling back to identity resolution (the same order `@me` uses).
5. If `--dry-run` is present, emit the preview (and optional explanation) and exit without touching disk.
6. Otherwise write the updated status, optionally set the assignee, save the task file, and print the result.

The `lotar task status` alias builds the same arguments and reuses this workflow.

## Status values

Each project controls allowed statuses via `issue_states` and `default_status` in `.tasks/<PROJECT>/config.yml`. Common defaults:

- `todo` - planned work
- `in_progress` - actively being developed
- `verify` - undergoing review or QA
- `blocked` - waiting on a dependency
- `done` - complete and verified

List the effective values for a project:

```bash
lotar config show --project=backend
lotar config show --project=backend --format=json | jq -r '.data.issue_states[]'
```

## Task ID and project resolution

Project detection runs in this order:

1. Explicit `--project` flag (prefix or full name).
2. Prefix embedded in `TASK_ID` (e.g., AUTH-123).
3. `default_project` from merged config.
4. Auto-detected prefix generated from the repo/workspace name when no default exists yet.

Numeric IDs (for example `123`) are expanded with the chosen prefix (AUTH-123). If no prefix can be deduced the command errors and asks for `--project`.

## Output formats

### Text (default)

```
Task AUTH-7 status changed from Todo to InProgress
Task AUTH-7 already has status 'InProgress'
```

### JSON

```json
{
  "status": "success",
  "message": "Task AUTH-7 status changed from Todo to InProgress",
  "task_id": "AUTH-7",
  "old_status": "Todo",
  "new_status": "InProgress",
  "assignee": "jane.doe"
}
```

Dry-run previews emit `status: "preview"`, a `status_change` action name, `old_status`, `new_status`, and any `would_set_assignee` or `explain` strings.

## Dry-run and explain

`--dry-run` (with optional `--explain`) shows exactly what would happen without writing:

```
DRY RUN: Would change AUTH-7 status from Todo to InProgress; would set assignee = jane.doe
Explanation: status validated against project config; auto-assign uses CODEOWNERS default when enabled, otherwise default_reporter→git user.name/email→system username.
```

JSON mode includes the same properties plus the explanation string.

## Auto-assign semantics

- Guarded by `auto.assign_on_status` (true by default, override via config or `LOTAR_AUTO_ASSIGN_ON_STATUS`).
- CLI path requires the task to be unassigned, the status to actually change, and the previous status to equal the configured default, ensuring the first move away from the default lane assigns an owner.
- When triggered, the candidate list checks for a CODEOWNERS default (when `auto.codeowners_assign` is true) before falling back to the standard identity chain documented in `docs/help/precedence.md`.
- REST/MCP updates use the same toggles but simply check whether the task was unassigned and the status changed.
- `@me` placeholders on other commands resolve to the same identity before comparisons happen.

## Validation and diagnostics

- `CliValidator` normalizes case/whitespace and ensures the status exists in the config list.
- `TaskCommandContext` reports detailed errors for missing workspaces, invalid IDs, or unresolved projects.
- Combine `--dry-run` and `--explain` to see which config/identity source was used before committing.
- Set `LOTAR_DEBUG=1` for extra resolver logs when troubleshooting precedence or identity issues.

## Error handling

Examples:

```
Status validation failed: 'invalid_status' is not in allowed values: [Todo, InProgress, Verify, Done]
Task 'AUTH-999' not found in project 'auth'
Could not resolve project: No project found for task ID '123'
Failed to edit task 'AUTH-7': <filesystem error message>
```

JSON mode surfaces the same messages under the `message` field.

## Environment variables

- `LOTAR_TASKS_DIR` - Highest-precedence workspace override after the command-line flag.
- `LOTAR_AUTO_ASSIGN_ON_STATUS`, `LOTAR_AUTO_CODEOWNERS_ASSIGN`, and the identity-related overrides listed in `docs/help/precedence.md` influence auto-assignment behavior.
