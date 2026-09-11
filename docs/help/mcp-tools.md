# MCP Tools Reference

Every MCP tool can be invoked directly (`method: "task/list"`) or through `tools/call` using its snake_case name (`task_list`). This guide summarizes the parameters, validation rules, and response payloads implemented by the server.


## Conventions

- All parameters use `snake_case` and mirror the CLI/REST field names.
- Responses follow the MCP `content` convention: `result.content[*].text` contains pretty-printed JSON (or multi-line text). Parse that string if you need structured data.
- Enum values (`status`, `priority`, etc.) are validated with the same `CliValidator` as the CLI; errors include `error.data.details` when enum hints are available.
- `@me` is accepted anywhere a reporter/assignee is expected and resolves using the same identity chain as the CLI.
- Many responses include `enumHints` so hosts can surface the project’s allowed values.

## Task Tools

### `whoami`
- **Params:** optional `explain` (bool).
- **Behavior:** resolves the user identity used for `@me`.
- **Response:** JSON with `status`, `user`, and optional `explain` metadata.

### `task_create`
- **Params:** `title` (required), optional `description`, `project`, `priority`, `type`, `status`, `reporter`, `assignee`, `due_date`, `effort`, `tags[]`, `acceptance_criteria[]`, `relationships`, `custom_fields` map, and `sprints[]` (numeric IDs).
- **Behavior:** validates `status`/`priority`/`type` against the target project's configuration (project-only enum values are accepted; failures carry `error.data.suggestions` from that project); an explicit `status` is persisted atomically with creation; auto-fills missing defaults (priority/type/status/reporter/assignee/tags) per project config; `@me` supported for people fields.
- **Response:** JSON blob containing the saved `task` plus `metadata.appliedDefaults` (fields the server filled) and `metadata.enumHints` when available.

### `task_get`
- **Params:** `id` (required) and optional `project` override to disambiguate numeric IDs.
- **Response:** Pretty-printed `TaskDTO` for the requested record.

### `task_update`
- **Params:** `id` (required) and `patch` object. Patch keys mirror `task_create` fields plus `acceptance_criteria` and `sprints`.
- **Behavior:** tri-state semantics per key: omitted = no-op, `null` = clear, value = set. Empty string clears reporter/assignee/due_date/effort/description; empty array/object clears tags/acceptance_criteria/relationships/custom_fields/sprints. `title`/`status`/`priority`/`type` treat `null` as omitted and blank titles are rejected. Enum strings are validated against the task's project configuration (failures return `-32602` with `error.data.suggestions`); list and map patches replace the whole value; `sprints` must be an array of positive integers (invalid entries are rejected, not dropped); membership failures keep the `Task update failed` envelope with `data.message`.
- **Response:** Updated `TaskDTO` serialized to JSON.

### `task_comment_add`
- **Params:** `id` (required), `text` (required).
- **Behavior:** appends a new comment and records a history entry.
- **Response:** Updated `TaskDTO`.

### `task_comment_update`
- **Params:** `id` (required), `index` (0-based, required), `text` (required).
- **Behavior:** updates the comment at the specified index and records a history entry.
- **Response:** Updated `TaskDTO`.

### `task_bulk_update`
- **Params:** `ids[]` (required), `patch` (required), optional `stop_on_error`.
- **Behavior:** applies the same patch to multiple tasks using the `task_update` tri-state semantics. Enum validation runs per task against its own project configuration, so a value valid in one project can fail in another (reported per id in `failed[]`). When `stop_on_error=true`, aborts after the first failure.
- **Response:** JSON with `updated[]` and `failed[]` per task id.

### `task_bulk_comment_add`
- **Params:** `ids[]` (required), `text` (required), optional `stop_on_error`.
- **Behavior:** appends the same comment to multiple tasks.
- **Response:** JSON with `updated[]` and `failed[]`.

### `task_bulk_reference_add`
- **Params:** `ids[]` (required), optional `project`, `kind` (required: `link|file|code|jira|github`), `value` (required), optional `stop_on_error`.
- **Behavior:** attaches the same reference to multiple tasks.
- **Response:** JSON with `updated[]` and `failed[]`.

### `task_bulk_reference_remove`
- **Params:** `ids[]` (required), optional `project`, `kind` (required: `link|file|code|jira|github`), `value` (required), optional `stop_on_error`.
- **Behavior:** detaches the same reference from multiple tasks.
- **Response:** JSON with `updated[]` and `failed[]`.

### `task_delete`
- **Params:** `id` (required) and optional `project`.
- **Response:** Text payload like `deleted=true` or `deleted=false`.

### `task_list`
- **Params:** filters matching `TaskListFilter`: `project`, `status`, `priority`, `type`, `tag`, `assignee`/`@me`, `search` (id/title/description/tags), `limit` (default 50, max 200), and `cursor` (string/number). Multiple values can be sent as arrays or comma-separated strings.
- **Errors:** `assignee: "@me"` that cannot be resolved returns JSON-RPC error `-32002` (fail closed — never returns the unfiltered list).
- **Response:** JSON with `status`, `count`, `total`, `cursor`, `limit`, `hasMore`, `nextCursor` (number or null), `tasks[]`, and optional `enumHints`. Pagination is 0-based; pass the returned `nextCursor` to fetch the next page.

## Sprint Tools

### `sprint_list`
- **Params:** `limit` (default 50, max 200), `cursor`/`offset` (string/number), optional `include_integrity`.
- **Response:** JSON with `status`, `count`, `total`, `cursor`, `limit`, `hasMore`, `nextCursor` (number or null), `sprints[]`, and optional `missing_sprints`/`integrity`.

### `sprint_get`
- **Params:** `sprint` or `sprint_id`.
- **Response:** JSON with `status` and a single `sprint` entry.

### `sprint_create`
- **Params:** `label`, `goal`, `plan_length`, `starts_at`, `ends_at`, `capacity_points`, `capacity_hours`, `overdue_after`, `notes`, `skip_defaults`.
- **Response:** JSON with `status`, created `sprint`, plus any warnings/defaults applied.

### `sprint_update`
- **Params:** `sprint`/`sprint_id` plus any fields to update (supports clearing capacity/actual timestamps via `null`).
- **Response:** JSON with `status`, updated `sprint`, and any warnings.

### `sprint_summary`
- **Params:** `sprint`/`sprint_id`.
- **Response:** Same payload as the CLI sprint summary report (status, metrics, timeline).

### `sprint_burndown`
- **Params:** `sprint`/`sprint_id`.
- **Response:** Same payload as the CLI sprint burndown report (`series[]` etc.).

### `sprint_velocity`
- **Params:** `limit` (default 6), `include_active` (default false), `metric` (`tasks|points|hours`).
- **Response:** Same payload as the CLI sprint velocity report.

### `sprint_add`
- **Params:** `tasks` (string or array, required), optional `sprint` (reference like `#1` or keyword), optional `sprint_id` (numeric id), `allow_closed` (default `false`), `force_single`/`force` (force reassignments), and `cleanup_missing` (remove dangling references first).
- **Response:** JSON with `status`, `action` (`created|updated|moved`), `sprint_id`, `sprint_label`, lists of `modified`, `unchanged`, `replaced`, `missing_sprints`, and optional `integrity` metrics. If reassignments occur, an additional text content item lists the human-readable warnings.

### `sprint_remove`
- **Params:** Same as `sprint_add` (`tasks`, optional `sprint`, optional `sprint_id`, optional `cleanup_missing`).
- **Response:** Mirrors `sprint_add` but describes removal results rather than assignments.

### `sprint_delete`
- **Params:** `sprint` (reference like `#1`) or `sprint_id` (numeric id), plus optional `cleanup_missing` to scrub dangling references.
- **Response:** Two content items: a summary sentence and a JSON object containing `deleted`, `sprint_id`, `sprint_label`, `removed_references`, `updated_tasks`, and optional `integrity` data.

### `sprint_backlog`
- **Params:** `project`, `status` list (defaults come from config), `tag` filter, `assignee`, `limit` (default 20, max 100), `cursor` (<= 5000), and `cleanup_missing`.
- **Response:** Paginated backlog with `status`, `count`, `total`, `cursor`, `nextCursor` (number or null), `tasks[]`, `missing_sprints`, `enumHints`, and a `truncated`/`hasMore` flag.

## Project Tools

### `project_list`
- **Params:** none.
- **Response:** Array of project metadata.

### `project_stats`
- **Params:** `name` (project key).
- **Response:** Aggregated stats (open counts, priorities, etc.) for the requested project.

## Config Tools

### `config_show`
- **Params:** `global` (bool) and optional `project` scope.
- **Response:** Pretty-printed YAML-equivalent JSON representing the resolved config at the requested scope.

### `config_set`
- **Params:** `values` map of key→string plus optional `global`/`project` selectors.
- **Response:** Text summary indicating success along with any validation warnings/info from the config service.

## Sync Tools

### `sync_pull`
- **Params:** `remote` (required), optional `project`, `auth_profile`, `dry_run`, `include_report`, `write_report`, `client_run_id`.
- **Response:** JSON summary plus report metadata; `include_report` returns per-item entries.

### `sync_push`
- **Params:** `remote` (required), optional `project`, `auth_profile`, `dry_run`, `include_report`, `write_report`, `client_run_id`.
- **Response:** JSON summary plus report metadata; `include_report` returns per-item entries.

## Schema Tool

### `schema_discover`
- **Params:** Optional `tool` name to filter the output.
- **Response:** Same structure as `tools/list`: `{ "status": "ok", "toolCount": N, "tools": [ ...definitions with inputSchema... ] }`. Use this to refresh per-tool enums without triggering a separate `tools/list` round-trip.

See also: [Identity & Users](./identity.md), [Task Model](./task-model.md), and [lotar mcp](./mcp.md) for transport details.
