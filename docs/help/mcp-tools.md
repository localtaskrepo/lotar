# MCP Tools Reference

Every MCP tool can be invoked directly (`method: "task/list"`) or through `tools/call` using its snake_case name (`task_list`). Control-plane methods (`initialize`, `tools/list`, `notifications/*`, …) are not tools and are rejected under `tools/call` as unknown names. This guide summarizes the parameters, validation rules, and response payloads implemented by the server.


## Conventions

- All parameters use `snake_case` and mirror the CLI/REST field names.
- Responses follow the MCP `content` convention: `result.content[*].text` contains pretty-printed JSON (or multi-line text). Parse that string if you need structured data.
- Tool arguments are validated against each tool's advertised `inputSchema` before the handler runs. Missing required arguments, unknown fields, and type violations return JSON-RPC error `-32602` with `error.data.tool` (the tool name) and `error.data.issues` (what failed). This is a strictness change: previously undocumented extra fields were never supported aliases — they are now rejected instead of being silently ignored. For example, `sync_pull`/`sync_push` reject an unknown `dryrun` field instead of treating it as `dry_run`, so a misspelled dry-run request can no longer trigger a live sync.
- Enum values (`status`, `priority`, etc.) are validated with the same `CliValidator` as the CLI; errors include `error.data.suggestions`/`error.data.details` when enum hints are available.
- Unknown or control-plane tool names under `tools/call` return `-32602` with an `Unknown tool: <name>` message and are never dispatched. Unknown direct methods return `-32601` Method not found.
- Semantic/domain failures (unknown task ids, membership failures, sync failures, …) are tool-execution failures, not protocol errors: the response carries `result.isError: true` with no JSON-RPC `error` object, and the existing explanation/data envelope lives in the `content[0].text` payload. Bulk tools keep partial outcomes in `updated[]`/`failed[]` instead of failing the whole call.
- `tools/call` results additionally embed a `functionResponse` compatibility wrapper (successes and `isError` conversions alike); direct method calls omit it.
- `@me` is accepted anywhere a reporter/assignee is expected and resolves using the same identity chain as the CLI.
- Many responses include `enumHints` so hosts can surface the project’s allowed values.
- Hints are advisory, not schema constraints. Tool-list hints describe configuration only when the workspace has at most one project; multi-project workspaces omit them rather than advertise another project's values. Request-scoped validation and response hints use the operation's resolved project configuration. Configuration changes refresh these values.
- Sprint IDs are not included in `enumHints` or tool-list field hints. Discover current sprint IDs and labels with `sprint_list`; sprint integrity diagnostics are separate from enum hints.
- For paginated tools, `cursor: null` behaves as omitted and permits the `offset` alias. A non-null `cursor` takes precedence over `offset`.

## Task Tools

### `whoami`
- **Params:** optional `explain` (bool).
- **Behavior:** resolves the user identity used for `@me`.
- **Response:** JSON with `status`, `user`, and optional `explain` metadata.

### `task_create`
- **Params:** `title` (required), optional `description`, `project`, `priority`, `type` (alias `task_type`), `status`, `reporter`, `assignee`, `due_date`, `effort`, `tags[]`, `acceptance_criteria[]`, `relationships`, `custom_fields` map, and `sprints[]` (positive integers).
- **Behavior:** validates `status`/`priority`/`type` against the target project's configuration (project-only enum values are accepted; failures carry `error.data.suggestions` from that project); an explicit `status` is persisted atomically with creation; auto-fills missing defaults (priority/type/status/reporter/assignee/tags) per project config; `@me` supported for people fields.
- **Response:** JSON blob containing the saved `task` plus `metadata.appliedDefaults` (fields the server filled) and `metadata.enumHints` when available.
- **Project:** An explicit `project` selects that project. When omitted, MCP uses the shared task service's repository-derived project prefix for both enum validation and creation; it does not pre-validate against unrelated global values.
- **Type Alias:** A non-null `type` takes precedence over `task_type`; `type: null` permits the alias. This precedence is consistent in create, patch, and list filters.

### `task_get`
- **Params:** `id` (required), optional `project` override to disambiguate numeric IDs, and `include_deleted` (bool, default false).
- **Response:** Pretty-printed `TaskDTO` for the requested record.

### `task_update`
- **Params:** `id` (required) and `patch` object. Patch keys mirror `task_create` fields (including the `task_type` alias for `type`) plus `acceptance_criteria` and `sprints`.
- **Behavior:** tri-state semantics per key: omitted = no-op, `null` = clear, value = set. Empty string clears reporter/assignee/due_date/effort/description; empty array/object clears tags/acceptance_criteria/relationships/custom_fields/sprints. `title`/`status`/`priority`/`type` treat `null` as omitted and blank titles are rejected. Enum strings are validated against the task's project configuration (failures return `-32602` with `error.data.suggestions`); list and map patches replace the whole value; `sprints` must be an array of positive integers (invalid entries are rejected, not dropped); membership failures keep the `Task update failed` envelope with `data.message` (a tool-execution `isError` result).
- **Response:** Updated `TaskDTO` serialized to JSON.

### `task_comment_add`
- **Params:** `id` (required), `text` (required).
- **Behavior:** appends a new comment and records a history entry.
- **Response:** Updated `TaskDTO`.

### `task_comment_update`
- **Params:** `id` (required), `index` (0-based, required), `text` (required).
- **Behavior:** updates the comment at the specified index and records a history entry.
- **Response:** Updated `TaskDTO`.

### `task_reference_add`
- **Params:** `id` (required), optional `project`, `kind` (required: `link|file|code|jira|github|attachment`), `value` (required).
- **Behavior:** attaches one reference to a task with the same semantics as the bulk variant: `file` values are repository-relative paths (attachments-store paths are rejected), `attachment` values are stored blob names and fail closed when the blob is missing, and `code`/`file` adds require a repository root.
- **Response:** JSON with the updated `task` and a `changed` flag.

### `task_reference_remove`
- **Params:** `id` (required), optional `project`, `kind` (required: `link|file|code|jira|github|attachment`), `value` (required).
- **Behavior:** detaches one reference with the same semantics as the bulk variant: `attachment` detach is reference-only and never deletes blobs, `code` removal works outside a Git repository, and `file` removal requires a repository root.
- **Response:** JSON with the updated `task` and a `changed` flag.

### `task_bulk_update`
- **Params:** `ids[]` (required), `patch` (required, same keys as `task_update` including the `task_type` alias), optional `stop_on_error`.
- **Behavior:** applies the same patch to multiple tasks using the `task_update` tri-state semantics. Enum validation runs per task against its own project configuration, so a value valid in one project can fail in another (reported per id in `failed[]`). When `stop_on_error=true`, aborts after the first failure.
- **Response:** JSON with `updated[]` and `failed[]` per task id.

### `task_bulk_comment_add`
- **Params:** `ids[]` (required), `text` (required), optional `stop_on_error`.
- **Behavior:** appends the same comment to multiple tasks through the same pipeline as the single-task tool: identical `comment_added` history entries and timestamps, and `on.commented` automation fires exactly once per task. Malformed or unknown ids are reported per id in `failed[]` (padded aliases like `TP-001` canonicalize). When `stop_on_error=true`, aborts after the first failure.
- **Response:** JSON with `updated[]` and `failed[]`.

### `task_bulk_reference_add`
- **Params:** `ids[]` (required), optional `project`, `kind` (required: `link|file|code|jira|github|attachment`), `value` (required), optional `stop_on_error`.
- **Behavior:** attaches the same reference to multiple tasks. `file` values are repository-relative paths (attachments-store paths are rejected); `attachment` values are stored blob names, fail closed when the blob is missing, and serialize on the store lock. `code` and `file` adds resolve repository-relative paths and therefore require a repository root — a missing root fails those items individually with `Unable to locate git repository` instead of aborting the batch. Every changed item records one `reference_added` history entry and fires the same post-commit automation as the single-task tool.
- **Response:** JSON with `updated[]` and `failed[]`.

### `task_bulk_reference_remove`
- **Params:** `ids[]` (required), optional `project`, `kind` (required: `link|file|code|jira|github|attachment`), `value` (required), optional `stop_on_error`.
- **Behavior:** detaches the same reference from multiple tasks. `attachment` detach is reference-only and never deletes blobs. `code` removal matches stored values and works outside a Git repository (like the single-task tool); `file` removal still requires a repository root, failing per item when it is missing. Every changed item records one `reference_removed` history entry and fires the same post-commit automation as the single-task tool.
- **Response:** JSON with `updated[]` and `failed[]`.

### `task_delete`
- **Params:** `id` (required), optional `project`, and `hard` (bool, default false).
- **Behavior:** default soft deletion sets `deleted_at`, retains content, and leaves `modified` unchanged. Hard deletion removes only the task file, retaining managed attachment blobs and incoming task relationships.
- **Response:** JSON with `deleted`, `hard`, and `warnings[]`; hard-delete warnings list retained attachment references and detectable incoming relationships.

### `task_restore`
- **Params:** `id` (required) and optional `project`.
- **Behavior:** clears `deleted_at` and appends history without changing `modified`; restores the same ID and content under current project validation. Already-active tasks are unchanged.
- **Response:** Updated `TaskDTO`.

### `task_list`
- **Deletion visibility:** `deletion` is `active` (default), `deleted`, or `all`; invalid/blank values fail validation. Deleted tasks retain status and content but cannot be ordinarily edited until restored.
- **Params:** filters matching `TaskListFilter`: `project`, `status`, `priority`, `type`, `tag`, `assignee`/`@me`, `search` (id/title/description/tags), `sprints`, `custom_fields`, smart filters `due` (`today|soon|later|overdue`), `recent` (`7d`), `needs` (CSV or array of `effort`,`due`), ordering `sort_by` (builtins `priority`,`status`,`effort`,`due-date`,`created`,`modified`,`assignee`,`reporter`,`title`,`type`,`project`,`id`,`tags`,`sprints` or `custom:<name>`/`field:<name>`; tags compare lexicographically and sprints numerically, empty first ascending) and `order` (`asc|desc`), `limit` (default 50, max 200), and `cursor` (string/number). Multiple values can be sent as arrays or comma-separated strings; multi-value filters (`status`, `priority`, `tags`) also accept `null` to clear the filter.
- **Errors:** `assignee: "@me"` that cannot be resolved fails closed as a tool-execution `isError` result (never returns the unfiltered list). Invalid explicit `status`/`priority`/`type` values (with enum hints), invalid `sprints` entries, and invalid `order`/`sort_by`/`due`/`recent`/`needs` values return `-32602` instead of being silently dropped, as do explicitly blank `due`/`recent`/`needs` strings. Enum filters validate against the explicit `project`'s resolved configuration when one is requested.
- **Response:** JSON with `status`, `count`, `total`, `cursor`, `limit`, `hasMore`, `nextCursor` (number or null), `tasks[]`, and optional `enumHints`. Pagination is 0-based; pass the returned `nextCursor` to fetch the next page. Pages iterate a deterministic global order (default `modified` desc, canonical-ID ascending tiebreak) identical to REST `/api/tasks/list` and `/api/tasks/export`.
- **Completion and due dates:** `due: "overdue"` means non-finished tasks due before today's local calendar date. Each task uses its own project's done-state policy, including mixed-project results; finished and due-today tasks do not match. Task DTOs include computed `task_state` metadata rather than a persisted completion flag. See [Done States](./config.md#done-states).

## Sprint Tools

### `sprint_list`
- **Params:** `limit` (default 50, max 200), `cursor`/`offset` (string/number), optional `include_integrity`.
- **Response:** JSON with `status`, `count`, `total`, `cursor`, `limit`, `hasMore`, `nextCursor` (number or null), `sprints[]`, and optional `missing_sprints`/`integrity`.

### `sprint_get`
- **Params:** `sprint` (reference like `#1`, keyword, or numeric id) or `sprint_id` (numeric; preferred).
- **Response:** JSON with `status` and a single `sprint` entry.

### `sprint_create`
- **Params:** `label`, `goal`, `plan_length`, `starts_at`, `ends_at`, `capacity_points`, `capacity_hours`, `overdue_after`, `notes`, `skip_defaults` (boolean, default `false`; explicit `null` is rejected). Capacity values are non-negative integers, not fractions.
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
- **Params:** `limit` (positive integer, default 6), `include_active` (default false), `metric` (`tasks|points|hours`). Invalid explicit windows are rejected rather than silently defaulted.
- **Response:** Same payload as the CLI sprint velocity report.

### `sprint_add`
- **Params:** `tasks` (string or array, required), optional `sprint` (reference like `#1`, numeric id, or keyword), optional `sprint_id` (numeric id), `allow_closed` (default `false`), `force_single`/`force` (advertised aliases — force reassignments), and `cleanup_missing` (remove dangling references first).
- **Aliases:** `force_single: null` permits the `force` alias; a boolean `force_single` takes precedence, including explicit `false`.
- **Response:** JSON with `status`, `action` (`created|updated|moved`), `sprint_id`, `sprint_label`, lists of `modified`, `unchanged`, `replaced`, `missing_sprints`, and optional `integrity` metrics. If reassignments occur, an additional text content item lists the human-readable warnings.

### `sprint_remove`
- **Params:** Same forms as `sprint_add` (`tasks` scalar or array, optional `sprint` reference/numeric id, optional `sprint_id`, optional `cleanup_missing`).
- **Response:** Mirrors `sprint_add` but describes removal results rather than assignments.

### `sprint_delete`
- **Params:** `sprint` (reference like `#1`) or `sprint_id` (numeric id), plus optional `cleanup_missing` to scrub dangling references.
- **Behavior:** `force` is not a parameter of this tool — it was previously advertised but ignored, and the schema now rejects it as an unknown property (`-32602`).
- **Response:** Two content items: a summary sentence and a JSON object containing `deleted`, `sprint_id`, `sprint_label`, `removed_references`, `updated_tasks`, and optional `integrity` data.

### `sprint_backlog`
- **Params:** `project`, `status` list (defaults come from config), `tag` filter, `assignee`, `limit` (default 20, max 100), `cursor` (<= 5000), and `cleanup_missing`.
- **Validation:** `sort_by`, `order`, `due`, `recent`, and `needs` are not supported here. They were previously advertised but ignored; they now return `-32602` rather than silently returning an unfiltered backlog. Use `task_list` for those filters.
- **Validation:** Explicit status values are canonicalized and validated against the selected project's configuration; invalid values return `-32602` with that project's suggestions. Validation also runs for empty results.
- **Response:** Paginated backlog with `status`, `count`, `total`, `cursor`, `limit`, `nextCursor` (number or null), `tasks[]`, `missing_sprints`, optional project-scoped `enumHints`, and `truncated`/`hasMore` flags. Empty pages retain the same pagination fields; cursors beyond the result set are clamped to its length.

## Project Tools

### `project_list`
- **Params:** `limit` (default 50, max 200), `cursor` (string/number, 0-based, <= 5000), and `offset` (cursor alias).
- **Validation:** `sort_by`, `order`, `due`, `recent`, and `needs` are rejected with `-32602`; the handler never implemented these previously advertised parameters.
- **Response:** Paginated project metadata in `projects[]`, with `count`, `total`, `cursor`, `limit`, `hasMore`, and `nextCursor` (string or null). Projects are ordered by prefix.

### `project_stats`
- **Params:** `name` (project key).
- **Response:** Aggregated stats (open counts, priorities, etc.) for the requested project.

## Config Tools

### `config_show`
- **Params:** `global` (bool) and optional `project` scope.
- **Response:** Pretty-printed YAML-equivalent JSON representing the resolved config at the requested scope.

### `config_set`
- **Params:** `values` map of key→string plus optional `global`/`project` selectors. Values must be strings — non-string values are rejected by the schema (silent coercion was removed).
- **Response:** Text summary indicating success along with any validation warnings/info from the config service.

## Sync Tools

### `sync_pull`
- **Params:** `remote` (required), optional `project`, `task_id`, `auth_profile`, `dry_run`, `include_report`, `write_report`, `client_run_id`.
- **Behavior:** `task_id` targets one local task using a fully qualified ID. The shared sync service resolves its project and remote linkage and rejects project/ID mismatches. Omit it or pass `null` to sync the selected project's scope. `dry_run` previews changes but pull still needs remote authentication and reads.
- **Response:** JSON summary plus report metadata; `include_report` returns per-item entries.

### `sync_push`
- **Params:** `remote` (required), optional `project`, `task_id`, `auth_profile`, `dry_run`, `include_report`, `write_report`, `client_run_id`.
- **Behavior:** `task_id` targets one local task with the same project/ID validation as `sync_pull`; omit it or pass `null` for the selected project's scope. Unknown fields such as `dryrun` remain errors and never trigger a live sync.
- **Response:** JSON summary plus report metadata; `include_report` returns per-item entries.

## Agent Tools

### `agent_run`
- **Params:** `ticket_id` (required, e.g. `PROJ-1`), `prompt` (required), optional `runner` (`copilot|claude|codex|gemini|command`) and `agent` (profile name from config).
- **Validation:** Supply either `runner` or `agent`. Unknown tickets, unsupported runners, and unknown profiles return explanatory tool-execution `isError` results; malformed argument types still return `-32602`.
- **Response:** Job details including the job ID used for status tracking.

### `agent_status`
- **Params:** `id` (job ID, required).
- **Response:** Job details including status, exit code, last message, and timing. Unknown job ids are tool-execution `isError` results.

### `agent_list_jobs`
- **Params:** none.
- **Response:** All agent jobs (running, queued, completed, failed, cancelled; newest first) plus queue statistics.

### `agent_cancel`
- **Params:** `id` (job ID, required).
- **Response:** Whether the job was cancelled, plus job details.

### `agent_send_message`
- **Params:** `id` (running job ID, required), `message` (required).
- **Behavior:** Only supported for runners that accept stdin input (Claude, Copilot).
- **Response:** Updated job details.

## Schema Tool

### `schema_discover`
- **Params:** Optional `tool` name to filter the output.
- **Response:** Same structure as `tools/list`: `{ "status": "ok", "toolCount": N, "tools": [ ...definitions with inputSchema... ] }`. Use this to refresh per-tool enums without triggering a separate `tools/list` round-trip.

See also: [Identity & Users](./identity.md), [Task Model](./task-model.md), and [lotar mcp](./mcp.md) for transport details.
