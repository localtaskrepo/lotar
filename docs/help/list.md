# lotar list (alias: ls)

`lotar list` is your daily dashboard. Combine filters to zero in on the work you care about, switch formats for scripts, and keep the output readable for humans.


## Quick Start

```bash
# Everyone's tasks, default view
lotar list

# Only your open work
lotar list --mine --status in_progress

# Use search text plus filters
lotar list "login" --project AUTH --priority high --limit 50

# Pipe JSON into tools
lotar --format json list --due-soon --tag release
```

`<SEARCH>` (optional) looks at titles and descriptions. All other filters are additive.

## Build the perfect filter

| Category | Useful flags |
| --- | --- |
| Project or workspace | `--project/-p`, `--tasks-dir`, `--tag/-i` |
| Status & priority | `--status/-s`, `--priority/-P`, plus shortcuts `--high` and `--critical` |
| Type & ownership | `--type/-t`, `--assignee/-a`, `--mine/-m`, `--where assignee=""` for unassigned |
| Dates | `--overdue`, `--due-soon[=days]` |
| Custom data | `--where key=value` or `--where field:<name>=value` (repeat as needed) |
| Effort windows | `--effort-min 2h`, `--effort-max 1d`, accepts time or points |
| Sorting & size | `--sort-by due-date`, `--reverse`, `--limit 100` |
| Output | `--format text|json|table|markdown`, `--log-level info` |

Tips:

- Combine multiple values by repeating the flag (`--status todo --status in_progress`).
- Use empty strings to catch missing data (`--where assignee=""`).
- `--where` works on any custom field you declared in config.

## Display styles

| Format | When to use it |
| --- | --- |
| `text` (default) | Colorful summaries for terminals. |
| `table`/`markdown` | Column-aligned output, great for copy/paste. |
| `json` | Trigger automation or feed dashboards. |

All formats show canonical IDs (`AUTH-12`), even if you entered `12`.

## Recipes

```bash
# Critical bugs with owners
lotar list --type bug --critical --where assignee!=""

# Sprint plan view
lotar list --where sprint=2025-W35 --sort-by field:sprint --reverse

# Risk board for PMs
lotar --format json list --project ENG --overdue --limit 100

# QA ready column
lotar list --status verify --tag release --sort-by due-date

# Check another workspace
lotar list --tasks-dir /repos/infra/.tasks --project INFRA
```

## Performance & etiquette

- Start with a project filter or keep the default limit (20) so listing remains fast on giant repos.
- JSON mode skips banners, which helps tools parse output quickly.
- If you script `lotar list`, prefer `--format json --log-level error` to keep stdout clean.

## Troubleshooting

| Symptom | Try this |
| --- | --- |
| “Task not found” or inconsistent IDs | Confirm you’re in the right workspace (`lotar status --explain` also prints context) or pass `--project`. |
| Filters return nothing | Run without `--where` to make sure the field exists; custom keys must match your config names. |
| Sorting feels off | Remember that string sorts are case-insensitive but depend on the stored values. Use `--sort-by field:<name>` for custom fields.

Happy with your filter? Drop it into an alias or script for repeat use.
