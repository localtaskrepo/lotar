# lotar list (alias: ls)

`lotar list` is your daily dashboard. Combine filters to zero in on the work you care about, switch formats for scripts, and keep the output readable for humans.


## Quick Start

```bash
# Everyone's tasks, default view
lotar list

# Only your open work
lotar list --mine --status in_progress

# Use search text plus filters
lotar list "login" --project AUTH --priority high --page-size 50

# Pipe JSON into tools
lotar --format json list --due-soon --tag release
```

`<SEARCH>` is optional positional text, not a `--search` flag. It performs a case-insensitive substring search across canonical IDs, titles, subtitles, descriptions, and tags. Other filters are applied alongside it.

## Build the perfect filter

| Category | Useful flags |
| --- | --- |
| Project or workspace | `--project/-p`, `--tasks-dir`, `--tag/-i` |
| Status & priority | `--status/-s`, `--priority/-P`, plus shortcuts `--high` and `--critical` |
| Type & ownership | `--type/-t`, `--assignee/-a`, `--mine/-m`, `--where assignee=alice` |
| Deletion | Active tasks by default; `--deleted` for deleted tasks only, or `--include-deleted` for both (mutually exclusive) |
| Dates | `--overdue` (due before today), `--due-soon[=days]` (due today through today+days inclusive) |
| Equality predicates | `--where key=value` or `--where field:<name>=value` (non-empty values; repeat as needed) |
| Effort windows | `--effort-min 2h`, `--effort-max 1d`, accepts time or points |
| Sorting & size | `--sort-by due-date` (keys: priority, status, effort, due-date, created, modified, assignee, reporter, title, type, project, id, tags, sprints, `field:<name>`), `--reverse`, `--page-size 100` (alias: `--limit`), plus `--offset` / `--page` |
| Output | `--format text|json`, `--log-level info`, `--details`; `table`/`markdown`/`md` are text aliases |

Tips:

- Combine multiple values by repeating the flag (`--status todo --status in_progress`).
- `--where` supports non-empty equality predicates only. Empty values (`assignee=`), inequality (`assignee!=alice`), and comparison operators are rejected rather than silently returning an empty result. Missing-field and general boolean expressions are not implemented; [DEV-5](../../.tasks/DEV/5.yml) owns the broader query language.
- Built-in predicate keys are `assignee`, `reporter`, `status`, `priority`, `type`, `tag`/`tags`, and `project`. Built-ins take precedence over wildcard custom fields. Other bare keys must be configured custom fields (or allowed by `custom_fields: ['*']`); `field:<name>` explicitly addresses a stored custom field.
- Different predicate keys are ANDed. Repeated equality values for the same key are ORed and use the existing fuzzy matching rules. Use `--mine` or `--assignee @me` to resolve the current user; `--where assignee=@me` is a literal comparison, not identity resolution.
- The first `=` separates the key from its literal value; operator-looking text after it is a value, not an expression. Put positional search text before `--where`, which can consume multiple predicates.

## Display styles

| Format | When to use it |
| --- | --- |
| `text` (default) | Colorful summaries for terminals. |
| `table`/`markdown`/`md` | Aliases for exactly the same text output; no aligned table or Markdown table is generated. |
| `json` | Trigger automation or feed dashboards. |

All formats show canonical IDs (`AUTH-12`), even if you entered `12`.

Deleted tasks retain their original status and `modified` timestamp. Deletion is a separate lifecycle, not a completion status; explicitly listing deleted tasks does not add them to normal project statistics or automation workload.

Default order (no `--sort-by`) is newest-modified first, matching the REST and MCP list endpoints; ties break by canonical ID ascending. `--assignee`/`--mine` match assignees fuzzily (case and `@`-insensitive), like the API. An unknown `--sort-by` key is an error unless it names a configured custom field (bare name, `field:<name>`, or `custom:<name>`).

Use `--details` to include task descriptions in text output, including its aliases. Separate table and Markdown renderers are not implemented. Older-than-N-days filtering ([DEV-3](../../.tasks/DEV/3.yml)) and list grouping/sums ([DEV-13](../../.tasks/DEV/13.yml)) remain separate features.

## Recipes

```bash
# Critical bugs assigned to Alice
lotar list --type bug --critical --assignee alice

# Custom sprint-field plan view (not numeric sprint membership)
lotar list --where field:sprint=2025-W35 --sort-by field:sprint --reverse

# Risk board for PMs
lotar --format json list --project ENG --overdue --page-size 100

# In-progress release work
lotar list --status in_progress --tag release --sort-by due-date

# Check another workspace
lotar list --tasks-dir /repos/infra/.tasks --project INFRA
```

The examples assume the named status/priority values and custom fields exist in your configuration. A custom field named `sprint` is separate from the task's numeric `sprints` membership.

## Performance & etiquette

- Start with a project filter or keep the default page size (20) so listing remains fast on giant repos.
- JSON mode skips banners, which helps tools parse output quickly.
- If you script `lotar list`, prefer `--format json --log-level error` to keep stdout clean.

## Pagination

The default page size is **20** tasks. When there are more results, the text output ends with a hint showing the remaining count and how to fetch the next page:

```
  … 144 more task(s) not shown. Use --page 2 (or --offset 20) to see the next page, or --page-size <N> to change page size.
```

Use either of these patterns:

```bash
# Offset-based (0-based)
lotar list --page-size 50 --offset 50

# Page-based (1-based)
lotar list --page-size 50 --page 2
```

Notes:

- `--limit` and `--per-page` are aliases for `--page-size`.
- `--page` conflicts with `--offset` (pick one).
- In `--format json` the response includes `total`, `limit`, `offset`, `page`, `total_pages`, `has_more`, `has_previous`, `next_offset`, and `next_page` so callers can paginate without parsing text.

## Troubleshooting

| Symptom | Try this |
| --- | --- |
| “Task not found” or inconsistent IDs | Confirm you’re in the right workspace (`lotar status --explain` also prints context) or pass `--project`. |
| Filters return nothing | Run without `--where` to make sure the field exists; custom keys must match your config names. |
| Sorting feels off | Ties between equal values always break by canonical ID ascending, even with `--reverse` (which flips the primary order only). Use `--sort-by field:<name>` for custom fields; tasks missing the field sort first ascending and last descending. |

Happy with your filter? Drop it into an alias or script for repeat use.
