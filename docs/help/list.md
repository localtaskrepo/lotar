# lotar list (alias: ls)

`lotar list` is your daily dashboard. Type what you want in the same syntax as the web UI search box, switch formats for scripts, and keep the output readable for humans.


## Quick Start

```bash
# Everyone's tasks, default view
lotar list

# Your open work
lotar list mine:true status:inprogress

# Search text plus filters
lotar list login priority:high --project AUTH --page-size 50

# Pipe JSON into tools
lotar --format json list due_within:7 tag:release
```

There is no separate `search` command: `list` with search text is the search. Quote the whole query when it contains shell characters such as `"`, `|`, `(` or `*`: `lotar list 'status:todo "login redirect"'`.

`lotar list`, the web UI, the REST `/api/tasks/list` and `/api/tasks/export` endpoints and the MCP `task_list` tool share one query: the same filter finds the same tasks in the same order everywhere.

## Filters in the query

The query uses the web search-box syntax. Values come from your configuration, so `priority:p1` works when the project's priorities are P0–P3.

| Write | Means | Aliases | REST / MCP |
| --- | --- | --- | --- |
| `status:todo,inprogress` | any of these statuses | `state:` | `status` |
| `priority:high` | any of these priorities | `prio:` | `priority` |
| `type:bug` | any of these types | `kind:`, `tasktype:` | `type` |
| `tag:ui,api` | any of these tags | `tags:` | `tags` (MCP also `tag`) |
| `sprint:3,4` | in any of these sprints | `sprints:` | `sprints` |
| `project:AUTH` | one project | `projectkey:` | `project` |
| `assignee:alice`, `assignee:@me`, `assignee:__none__` | assignee, yourself, or unassigned | `owner:` | `assignee` |
| `mine:true` | assigned to you | `me:` | `assignee=@me` |
| `due:today\|soon\|later\|overdue` | due bucket: soon is the next 7 days after today; overdue is before today and not done | `duedate:`, `dueon:` | `due` |
| `due_within:3` | due today through 3 days ahead | `duewithin:` | `due_within` |
| `recent:7d` | changed in the last 7 days | | `recent` |
| `needs:effort,due` | missing every listed field | `need:` | `needs` |
| `has:attachment,image,link,tag` | carries every listed kind | `with:` | `has` |
| `effort_min:2h`, `effort_max:1d` | inclusive effort bounds; time and points never compare; tasks without an effort are left out | `effortmin:`, `effortmax:` | `effort_min`, `effort_max` |
| `field:iteration=wave-2`, `iteration=wave-2` | a custom field | | any other key; MCP `custom_fields` |
| `sort_by=due-date`, `order=asc` | sort key and direction | | `sort_by`, `order` |
| `deletion=deleted\|all` | deleted tasks only, or both | | `deletion` |
| `q="exact words"` | the search text, set explicitly | | `q`; MCP `search` |

Rules, the same as in the web UI:

- Values of one filter are ORed (`status:todo,done`, or `status:todo status:done`); different filters are ANDed.
- `key:value` adds to a list filter; `key=value` replaces it. Quote values with spaces: `status:"In Progress"`.
- Keys are case-insensitive. A word with a colon whose key is not in the table, such as `title:login`, stays search text.
- A custom field whose name is also a key above must be quoted: `field:"due"=friday`.
- `@me` (in `assignee:` or `mine:true`) fails with an error when no identity is configured, rather than listing nothing.
- A malformed term such as `owner=` or an unclosed quote is an error.

## Search text

Words that are not filters are the search text. The same rules apply in the web UI search box, the REST `q` parameter and the MCP `search` parameter:

- **Every word must match**, in any order and in any searched field: `lotar list redirect login` finds "Fix the login redirect loop".
- **Quoted phrases** match the words together: `lotar list '"login redirect"'`.
- **Exclusions** drop tasks containing a word or phrase: `lotar list login -oauth`.
- **Partial words, case and accents** don't matter: `redir` finds `redirect`, `uber` finds `über`, and `ATLS-2` finds the task ID.
- **Typos are tolerated** in words of four or more letters (one mistake, two from eight letters): `redirct` finds `redirect`. Words with digits or symbols, phrases and exclusions match exactly.
- **Searched fields:** ID, title, subtitle, description, acceptance criteria, tags, references, and comments.
- **One field only:** prefix a term with `id:`, `title:`, `subtitle:`, `desc:` (or `description:`), `criteria:` (or `acceptance:`), `ref:` (or `reference:`), or `comment:`, for example `title:login`, `comment:"needs repro"` or `-title:draft`. Put the value right after the colon. In `lotar list` and the web UI, `tag:` is the tag filter; in REST `q` and MCP `search` it limits a term to tags.
- **Regular expressions** go between slashes: `lotar list '/fix(es|ed)? #\d+/'` or `title:/^draft/`. They are case-insensitive (start with `(?-i)` for case-sensitive), match the original text without accent folding, and may contain spaces: a pattern ends at the first `/` followed by a space or the end. Patterns use [Rust regex syntax](https://docs.rs/regex/latest/regex/#syntax), which always runs in linear time but has no lookaround or backreferences. `\w`, `\d`, `\s`, `\b` and case-insensitive matching understand every script, as do general categories such as `\p{L}` (letters) or `\p{Lu}` (uppercase letters); named scripts and properties such as `\p{Greek}`, `\p{Han}` or `\p{Emoji}` are not supported (use a character range like `[\u4e00-\u9fff]` instead). The same holds for scan ticket patterns, validation patterns and automation `matches` conditions. An invalid pattern is an error (`400` over REST), not an empty result.

Results keep the usual order. Add `--sort-by relevance` (or `sort_by=relevance`) to list the best matches first: an exact ID, then an exact title, then matches in the title, subtitle, description, tags and comments. `--reverse` lists the weakest matches first.

## Flags

Flags set the same filters as the query and combine with it: list filters add up, and the query wins for single values.

| Category | Flags |
| --- | --- |
| Project or workspace | `--project/-p`, `--tasks-dir` |
| Values | `--status/-s`, `--priority/-P`, `--type/-t`, `--tag/-i`, `--sprint` (repeat a flag for several values), `--assignee/-a` (`@me`, `__none__`) |
| Dates | `--due <bucket>`, `--due-within <days>`, `--recent 7d` |
| Missing or present | `--needs effort,due`, `--has attachment,image,link,tag` |
| Effort | `--effort-min 2h`, `--effort-max 1d` |
| Deletion | Active tasks by default; `--deleted` for deleted tasks only, or `--include-deleted` for both (mutually exclusive) |
| Sorting & size | `--sort-by due-date` (keys: priority, status, effort, due-date, created, modified, assignee, reporter, title, type, project, id, tags, sprints, relevance, `field:<name>`), `--reverse`, `--page-size 100` (alias: `--limit`), plus `--offset` / `--page` |
| Output | `--format text|json`, `--log-level info`, `--details`; `table`/`markdown`/`md` are text aliases |

The former shortcuts are gone; write the filter instead: `--mine` is `mine:true`, `--unassigned` is `assignee:__none__`, `--high`/`--critical` are `priority:high`/`priority:critical`, `--overdue` is `due:overdue`, `--due-soon=N` is `due_within:N`, and `--where key=value` is `key=value` (or `field:name=value`).

## Display styles

| Format | When to use it |
| --- | --- |
| `text` (default) | Colorful summaries for terminals. |
| `table`/`markdown`/`md` | Aliases for exactly the same text output; no aligned table or Markdown table is generated. |
| `json` | Trigger automation or feed dashboards. |

All formats show canonical IDs (`AUTH-12`), even if you entered `12`.

Deleted tasks retain their original status and `modified` timestamp. Deletion is a separate lifecycle, not a completion status; explicitly listing deleted tasks does not add them to normal project statistics or automation workload.

Default order (no sort key) is newest-modified first, matching the REST and MCP list endpoints; ties break by canonical ID ascending. With a sort key the CLI lists ascending unless `--reverse` or `order=desc` is given. Assignees match fuzzily (case and `@`-insensitive), like the API. An unknown `--sort-by` key is an error unless it names a configured custom field (bare name, `field:<name>`, or `custom:<name>`).

Use `--details` to include task descriptions in text output, including its aliases. Separate table and Markdown renderers are not implemented. Older-than-N-days filtering ([DEV-3](../../.tasks/DEV/3.yml)) and list grouping/sums ([DEV-13](../../.tasks/DEV/13.yml)) remain separate features.

## Recipes

```bash
# Critical bugs assigned to Alice
lotar list type:bug priority:critical assignee:alice

# Custom sprint-field plan view (not numeric sprint membership)
lotar list 'field:"sprint"=2025-W35' --sort-by field:sprint --reverse

# Risk board for PMs
lotar --format json list --project ENG due:overdue --page-size 100

# Sprint 4 work that still needs an estimate
lotar list sprint:4 needs:effort

# Tasks with screenshots, changed this week
lotar list has:image recent:7d

# In-progress release work
lotar list status:inprogress tag:release --sort-by due-date

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
| Filters return nothing | Drop the custom-field term to make sure the field exists; custom keys must match your config names. |
| Sorting feels off | Ties between equal values always break by canonical ID ascending, even with `--reverse` (which flips the primary order only). Use `--sort-by field:<name>` for custom fields; tasks missing the field sort first ascending and last descending. |

Happy with your filter? Drop it into an alias or script for repeat use.
