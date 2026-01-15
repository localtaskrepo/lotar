# MCP Tools Reference

Every tool registered in `src/mcp/server/tools.rs` can be invoked directly (`method: "task/list"`) or through `tools/call` using its snake_case name (`task_list`). This guide summarizes the parameters, validation rules, and response payloads implemented by the handlers in `src/mcp/server/handlers/*`.

## Conventions

- All parameters use `snake_case` and mirror the CLI/REST field names.
- Responses follow the MCP `content` convention: `result.content[*].text` contains pretty-printed JSON (or multi-line text). Parse that string if you need structured data.
- Enum values (`status`, `priority`, etc.) are validated with the same `CliValidator` as the CLI; errors include `error.data.details` when enum hints are available.
- `@me` is accepted anywhere a reporter/assignee is expected and resolves via `src/utils/identity.rs`.
- Many responses include `enumHints` so hosts can surface the project’s allowed values.
- List-style tools are paginated. Use `limit` plus `cursor` (0-based offset) to page; most handlers also accept `offset` as an alias for `cursor`. Pass the returned `nextCursor` to fetch the next page.

## Task Tools

### `task_create`
- **Params:** `title` (required), optional `description`, `project`, `priority`, `type`, `status`, `reporter`, `assignee`, `due_date`, `effort`, `tags[]`, `relationships`, `custom_fields` map, and `sprints[]` (numeric IDs).
- **Behavior:** Validates enums via `CliValidator`; auto-fills missing defaults (priority/type/status/reporter/assignee/tags) per project config; `@me` supported for people fields.
- **Response:** JSON blob containing the saved `task` plus `metadata.appliedDefaults` (fields the server filled) and `metadata.enumHints` when available.

### `task_get`
- **Params:** `id` (required) and optional `project` override to disambiguate numeric IDs.
- **Response:** Pretty-printed `TaskDTO` for the requested record.

### `task_update`
- **Params:** `id` (required) and `patch` object. Patch keys mirror `task_create` fields and can be nulled/reset (e.g., `relationships: null` clears relationships).
- **Response:** Updated `TaskDTO` serialized to JSON.

### `task_delete`
- **Params:** `id` (required) and optional `project`.
- **Response:** Text payload like `deleted=true` or `deleted=false`.

### `task_list`
- **Params:** filters matching `TaskListFilter`: `project`, `status`, `priority`, `type`, `tag`, `assignee`/`@me`, `search` (id/title/description/tags), `limit` (default 50, max 200), and `cursor` (string/number). Multiple values can be sent as arrays or comma-separated strings.
- **Response:** JSON with `status`, `count`, `total`, `cursor`, `limit`, `hasMore`, `nextCursor`, `tasks[]`, and optional `enumHints`. Pagination is 0-based; pass the returned `nextCursor` to fetch the next page.

## Sprint Tools

### `sprint_add`
- **Params:** `tasks` (string or array, required), `sprint` (numeric id or keyword), `allow_closed` (default `false`), `force_single`/`force` (force reassignments), and `cleanup_missing` (remove dangling references first).
- **Response:** JSON with `status`, `action` (`created|updated|moved`), `sprint_id`, `sprint_label`, lists of `modified`, `unchanged`, `replaced`, `missing_sprints`, and optional `integrity` metrics. If reassignments occur, an additional text content item lists the human-readable warnings.

### `sprint_remove`
- **Params:** Same as `sprint_add` (`tasks`, optional `sprint`, optional `cleanup_missing`).
- **Response:** Mirrors `sprint_add` but describes removal results rather than assignments.

### `sprint_delete`
- **Params:** `sprint` (numeric id, required) and optional `cleanup_missing` to scrub dangling references.
- **Response:** Two content items: a summary sentence and a JSON object containing `deleted`, `sprint_id`, `sprint_label`, `removed_references`, `updated_tasks`, and optional `integrity` data.

### `sprint_backlog`
- **Params:** `project`, `status` list (defaults come from config), `tag` filter, `assignee`, `limit` (default 20, max 100), `cursor` (<= 5000), and `cleanup_missing`.
- **Response:** Paginated backlog with `status`, `count`, `total`, `cursor`, `nextCursor`, `tasks[]`, `missing_sprints`, `enumHints`, and a `truncated`/`hasMore` flag.

## Project Tools

### `project_list`
- **Params:** `limit` (default per server, max per server), and `cursor` (string/number, 0-based). `offset` is accepted as an alias for `cursor`.
- **Response:** JSON with `status`, `count`, `total`, `cursor`, `limit`, `hasMore`, `nextCursor`, and `projects[]`.

### `project_stats`
- **Params:** `name` (project key).
- **Response:** Aggregated stats (open counts, priorities, etc.) from `ProjectService::stats` for the requested project.

## Config Tools

### `config_show`
- **Params:** `global` (bool) and optional `project` scope.
- **Response:** Pretty-printed YAML-equivalent JSON representing the resolved config at the requested scope.

### `config_set`
- **Params:** `values` map of key→string plus optional `global`/`project` selectors.
- **Response:** Text summary indicating success along with any validation warnings/info from `ConfigService::set`.

## Schema Tool

### `schema_discover`
- **Params:** Optional `tool` name to filter the output.
- **Response:** Same structure as `tools/list`: `{ "status": "ok", "toolCount": N, "tools": [ ...definitions with inputSchema... ] }`. Use this to refresh per-tool enums without triggering a separate `tools/list` round-trip.

See also: [Identity & Users](./identity.md), [Task Model](./task-model.md), and [lotar mcp](./mcp.md) for transport details.
