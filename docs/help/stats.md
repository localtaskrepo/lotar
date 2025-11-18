# lotar stats

Read-only analytics derived from git history (no git writes).


Project scope defaults to the current project; use `--global` to span all projects.

Performance note: some aggregations apply a safety cap to the number of tasks processed. Override via `LOTAR_STATS_EFFORT_CAP` when needed.

## Usage

```bash
lotar stats <SUBCOMMAND> [OPTIONS]
```

Global flags available on all subcommands:

- Standard CLI flags apply everywhere: `--project` (override auto-detected project), `--tasks-dir` (custom `.tasks` path), `--format` (text/table/json/markdown), and `--log-level` / `--verbose`.
- Unless otherwise noted, `--global` is the stats-scoped flag that spans every project under `.tasks`. Without it the active project (auto-detected or supplied via `--project`) scopes the query.
- Time windows (`--since`, `--until`, `--threshold`, etc.) accept RFC3339 timestamps (`2025-08-01T10:00Z`), ISO dates, or relative tokens like `14d`, `yesterday`, and weekday names. When both flags are omitted the commands default to `since = now - 30d` and `until = now`.

## Subcommands

### changed
Tickets changed within a window (git-only).

```bash
lotar stats changed [--since <when>] [--until <when>] [--author <sub>] [--limit N] [--global]
```
Options:
- `--author <sub>`: Case-insensitive substring match on author name/email
- `--limit N`: Max items (default 20)

Example:
```bash
lotar stats changed --since 14d
lotar --format json stats changed --since 7d --author alice
```

### churn
Highest churn by ticket (commit count) in a window.

```bash
lotar stats churn [--since <when>] [--until <when>] [--author <sub>] [--limit N] [--global]
```

Options:
- `--author <sub>`: Case-insensitive substring match on author name/email.
- `--limit N`: Max items (default 20).

### authors
Top authors by commits touching tasks in a window.

```bash
lotar stats authors [--since <when>] [--until <when>] [--limit N] [--global]
```

Notes:
- `--limit N`: Defaults to 20.
- Without `--since`/`--until`, the shared resolver uses the last 30 days.

### activity
Grouped commit activity over time.

```bash
lotar stats activity [--since <when>] [--until <when>] [--group-by author|day|week|project] [--limit N] [--global]
```
Defaults: `--group-by day`, `--limit 20`.

Examples:
```bash
# Group by day in the last 60 days (current project)
lotar stats activity --since 60d --group-by day

# Group by project across all projects
lotar stats activity --since 90d --group-by project --global
```

### stale
Tickets whose last change is older than a threshold.

```bash
lotar stats stale [--threshold Nd|Nw] [--limit N] [--global]
```
Defaults: `--threshold 21d`, `--limit 20`.

Examples:
```bash
lotar stats stale --threshold 30d
lotar --format json stats stale --threshold 8w --global --limit 50
```

### tags
Top tags across current tasks (snapshot; not git-window based).

```bash
lotar stats tags [--limit N] [--global]
```
Defaults: `--limit 20`.

Example JSON output:
```json
{
  "status": "ok",
  "action": "stats.tags",
  "items": [
    { "tag": "infra", "count": 7 },
    { "tag": "frontend", "count": 5 }
  ]
}
```

### distribution
Distribution of tasks by a field (snapshot).

```bash
lotar stats distribution --field status|priority|type|assignee|reporter|project|tag [--limit N] [--global]
```
Defaults: `--limit 20`.

To bucket by custom metadata (for example a `product` field), use:

```bash
lotar stats custom-field --field product [--limit N] [--global]
```

Example JSON output:
```json
{
  "status": "ok",
  "action": "stats.distribution",
  "field": "status",
  "items": [
    { "key": "TODO", "count": 10 },
    { "key": "IN_PROGRESS", "count": 4 }
  ]
}
```

### due
Summarize tasks into due-date buckets (snapshot), or show only overdue items with an age threshold.

```bash
lotar stats due [--buckets overdue,today,week,month,later] [--global]
lotar stats due --overdue [--threshold 0d|7d] [--global]
```
Defaults:
- Buckets: `overdue,today,week,month,later`
- Overdue threshold: `0d` (include any overdue)

Example JSON output:
```json
{
  "status": "ok",
  "action": "stats.due",
  "buckets": "overdue,today,week,month,later",
  "overdue_only": false,
  "items": [
    { "bucket": "overdue", "count": 2 },
    { "bucket": "today", "count": 1 },
    { "bucket": "week", "count": 3 },
    { "bucket": "month", "count": 5 },
    { "bucket": "later", "count": 7 }
  ]
}
```

Overdue-only example:
```json
{
  "status": "ok",
  "action": "stats.due",
  "buckets": "overdue",
  "overdue_only": true,
  "threshold": "7d",
  "items": [ { "bucket": "overdue", "count": 4 } ]
}
```

### time-in-status
Compute time spent in each status for tasks within a window (derived from git history; read-only). Defaults to current project; use `--global` to include all projects. Output includes per-status seconds/hours/percent and per-task totals.

```bash
lotar stats time-in-status [--since <when>] [--until <when>] [--limit N] [--global]
```

Defaults: `--limit 20`.

Notes:
- Window defaults to `--since now-30d` / `--until now` when omitted.
- Uses commit timestamps and YAML snapshots at each commit to infer status durations.
- Tolerates individual snapshot parse failures; durations are accumulated only when status is known.
- Results are sorted by total seconds descending and limited by `--limit`.

Example JSON output (excerpt):
```json
{
  "status": "ok",
  "action": "stats.time_in_status",
  "since": "2025-08-01T00:00:00Z",
  "until": "2025-08-18T00:00:00Z",
  "global": true,
  "count": 1,
  "items": [
    {
      "id": "TEST-1",
      "total_seconds": 1432800,
      "total_hours": 398.0,
      "items": [
        { "status": "TODO", "seconds": 774000, "hours": 215.0, "percent": 0.5402 },
        { "status": "IN_PROGRESS", "seconds": 615600, "hours": 171.0, "percent": 0.4296 },
        { "status": "DONE", "seconds": 43200, "hours": 12.0, "percent": 0.0301 }
      ]
    }
  ]
}
```

### status (per-ticket time in status)

```bash
lotar stats status <TASK_ID> --time-in-status [--since <when>] [--until <when>]
```

Notes:
- `<TASK_ID>` accepts numeric IDs (resolved against the active project) or fully-qualified IDs such as `AUTH-123`.
- `--time-in-status` is currently the only supported mode; the command errors with guidance if it is omitted.
- Windows default to `--since now-30d` / `--until now` when the flags are not provided.
- Output matches `time-in-status` but returns a single entry for the selected task and honors global CLI `--format` settings.

### age

Group tasks by age since creation as a snapshot, using day/week/month buckets.

```bash
lotar stats age [--distribution day|week|month] [--limit N] [--global]
```

Notes:
- Uses each task's `created` timestamp. Bucketing is approximate for months (30 days).
- Default distribution: day. Limit controls how many buckets are shown (newest first).

### effort

Aggregate effort estimates across tasks (snapshot). Effort strings support h (hours), d (days=8h), w (weeks=40h).

Options:
- `--by <key>` grouping key. Built-ins: assignee, reporter, type, status, priority, project, tag. Declared custom fields are accepted directly (e.g., `--by sprint`). You can also use `field:<name>` explicitly. Default: `assignee`.
- `--where key=value` (repeatable) filters. Keys follow the same rules as `--by`. Tags accept `tag` or `tags`.
- `--unit hours|days|weeks|points|auto` select output unit; auto picks hours if any present, else points. Default: `hours`.
- `--since <date>` / `--until <date>` window filters (RFC3339 or relative like `14d`, `yesterday`, weekday names).
- `--transitions <STATUS>` include only tasks that transitioned into STATUS within the window (requires git).
  - Window semantics: a task is included if a commit inside [since, until] shows its status changed into STATUS. Commits outside the window are ignored for this filter.
- `--limit <N>` cap number of groups (default 20); `--global` to span all projects.

Examples:
```bash
lotar stats effort --by assignee --unit hours
lotar stats effort --by sprint --where sprint=2025-W35
lotar stats effort --by field:sprint --where status=todo --since 14d
```

Tip: Use an RFC3339 window like `--since 2025-08-11T00:00:00Z --until 2025-08-13T00:00:00Z` to precisely target a day when testing transitions.

JSON output example:
```json
{
  "status":"ok","action":"stats.effort","by":"assignee","items":[
    {"key":"alice","hours":40.0,"days":5.0,"weeks":1.0,"tasks":2}
  ]
}
```

### comments

Top tasks by comment count:

```
lotar stats comments-top [--limit N] [--global]
```

By author:

```
lotar stats comments-by-author [--limit N] [--global]
```

Notes:
- Both commands default to `--limit 20`; raise it for larger leaderboards.

### custom fields

List most common custom field keys:

```
lotar stats custom-keys [--limit N] [--global]
```

Distribution of values for a specific field:

```
lotar stats custom-field --field <name> [--limit N] [--global]
```

Notes:
- `--limit` defaults to 20 on both custom field commands.

## JSON Output Examples

Changed (excerpt):
```json
{
  "status": "ok",
  "action": "stats.changed",
  "items": [
    { "id": "AUTH-12", "project": "AUTH", "file": ".tasks/AUTH/12.yml", "last_author": "Alice", "last_commit": "abc123", "last_date": "2025-08-15T12:00:00Z", "commits": 3 }
  ]
}
```
Authors (excerpt):
```json
{
  "status": "ok",
  "action": "stats.authors",
  "items": [
    { "author": "Alice", "email": "alice@example.com", "commits": 5, "last_date": "2025-08-16T12:00:00Z" }
  ]
}
```

Activity (excerpt):
```json
{
  "status": "ok",
  "action": "stats.activity",
  "group_by": "day",
  "items": [
    { "key": "2025-08-15", "count": 4, "last_date": "2025-08-15T12:00:00Z" }
  ]
}
```

## Notes
- Outside a git repository, stats return an empty set with a warning/note.
- Time parsing is shared across commands and accepts RFC3339, YYYY-MM-DD, and relative forms like `14d`, `yesterday`, weekday names, etc.
