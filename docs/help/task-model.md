# Task Model

Canonical fields, enums, and invariants for tasks returned by REST, MCP, and CLI renderers. The [OpenAPI spec](../openapi.json) mirrors the same schema.


## Identity and IDs

- Tasks are stored under `.tasks/<PROJECT>/TASK.yml`. The project prefix is derived from the project name unless overridden.
- IDs always follow `PREFIX-<NUMBER>` (e.g. `AUTH-42`). CLI commands accept the numeric portion when the active project is unambiguous (`lotar status 42`), otherwise pass the fully-qualified ID or `--project`.
- Normal task-service mutations retain `created` and update `modified` using the current clock. Direct YAML edits and low-level storage writes are not timestamp-validated.

## TaskDTO fields

Field | Type | Notes
----- | ---- | -----
`id` | `string` | Canonical task identifier (`PROJECT-N`).
`title` | `string` | Required summary/title.
`status` | `TaskStatus` | Validated against `config.issue_states` (see below). Stored as user-provided casing.
`priority` | `Priority` | Validated against `config.issue_priorities`.
`task_type` | `TaskType` | Validated against `config.issue_types`.
`reporter` | `string?` | Optional; resolved via identity helpers if omitted during creation.
`assignee` | `string?` | Optional; never auto-cleared when statuses change.
`created` | `RFC3339 string` | UTC timestamp recorded at creation.
`modified` | `RFC3339 string` | UTC timestamp updated on any mutation.
`due_date` | `string?` | ISO8601 date/time or natural-language token parsed by CLI validators.
`effort` | `string?` | CLI/service writes normalize time to hours with two decimal places (e.g., `90m` becomes `1.50h`) and points to `pt` (e.g., `5pts` becomes `5pt`). Direct YAML reads retain the stored string.
`subtitle` | `string?` | Short secondary label; omitted unless explicitly set.
`description` | `string?` | Markdown-friendly long description.
`tags` | `string[]?` | Normalized, unique tags. Omitted when empty; clients should default missing arrays to `[]`.
`relationships` | `TaskRelationships` | Structured references to other tasks (see below).
`comments` | `TaskComment[]?` | Each comment carries `{ date, text }`; omitted when empty.
`references` | `ReferenceEntry[]?` | Code locations (`code`), external URLs (`link`), attachments (`file`), or platform references (`jira`, `github`); omitted when empty.
`sprints` | `u32[]?` | Numeric sprint IDs the task belongs to; omitted when empty.
`sprint_order` | `BTreeMap<u32, u32>?` | Optional manual ordering for this task (sprint id → order index).
`history` | `TaskChangeLogEntry[]?` | Chronological change log entries (field deltas, actor, timestamp); omitted when empty.
`custom_fields` | `CustomFields` | Map of configured custom-field keys → YAML/JSON values. Skipped when empty.

### Relationships & related structs

- `TaskRelationships` exposes dedicated arrays for `depends_on`, `blocks`, `related`, `children`, `fixes`, plus single-value `parent` and `duplicate_of`. All properties are optional; empty collections are dropped on serialization.
- `TaskComment` holds `{ date: RFC3339, text: string }`. Comments do not store authorship today.
- `ReferenceEntry` supports `code` (e.g., `app/lib.rs:120`), `link` (URL), `file` (a relative attachment path stored under the configured attachments root), and platform references via `jira` or `github`.
- `TaskChangeLogEntry` captures `{ at, actor?, changes[] }`, where each `TaskChange` includes `field`, `old`, and `new` values for audit review.

### Custom fields

`custom_fields` is a free-form map backed by `HashMap<String, serde_yaml::Value>` (or JSON when schema generation is enabled). Keys correspond to names declared in `config.custom_fields` or dynamic fields permitted by `custom_fields.values`. CLI and REST mutate them via `field:<name>` syntax, while listing/filtering converts them back into strings using `custom_value_to_string`.

### Sprints & ordering

Sprint files under `.tasks/@sprints/<ID>.yml` are authoritative for membership and manual ordering: their `tasks` entries identify tasks and carry optional order values. The DTO's `sprints` and `sprint_order` are derived from these records by the sprint services/helpers. The task YAML's legacy `sprints` field remains readable but is not the membership authority.

### YAML extensions

Unknown top-level task YAML keys are retained as typed YAML values across task edits, including scalar, list, and nested mapping values. This is **semantic preservation**, not lossless text editing: comments, key order, quoting, whitespace, and anchor formatting are not preserved. Built-in fields and their aliases (such as `type`/`task_type`) remain authoritative and cannot be overridden by extension values.

These extensions are separate from `custom_fields` and are not added to REST/MCP task DTOs. Tolerant legacy parsing retains valid structured fields and extensions; malformed structured data is rejected rather than silently emptied during an edit.

## Enumerations & config-driven values

- `TaskStatus`, `Priority`, and `TaskType` are thin wrappers around strings. They validate against the arrays declared in `config.yml` under `issue_states`, `issue_priorities`, and `issue_types`. The examples below are defaults; workspaces routinely customize them.
	- `issue_states`: e.g., `TODO`, `IN_PROGRESS`, `VERIFY`, `BLOCKED`, `DONE`, `CANCELED`.
	- `issue_priorities`: e.g., `Low`, `Medium`, `High`, `Critical`, `Blocker`.
	- `issue_types`: e.g., `feature`, `bug`, `epic`, `spike`, `chore`.
- Because these values are data-driven, client code should not assume a fixed enum list; always render the exact casing stored on the task.

## Create/update payloads

- `TaskCreate` accepts `title`, optional `project`, and optional metadata for `status`, `priority`, `task_type`, `reporter`, `assignee`, `due_date`, `effort`, `description`, `tags`, `acceptance_criteria`, `relationships`, `custom_fields`, and `sprints`. Missing properties are defaulted downstream; enum strings are validated against the target project's configuration and an explicit `status` is stored atomically with creation.
- `TaskUpdate` treats every field as optional with explicit null semantics: omitted = no-op, `null` = clear (where clearing is allowed), value = set. Empty string clears the clearable scalars (`reporter`, `assignee`, `due_date`, `effort`, `description`); empty array/object clears `tags`, `acceptance_criteria`, `relationships`, `custom_fields`, and `sprints`. `title`, `status`, `priority`, and `task_type` treat `null` as omitted. List and map patches replace the whole value, and enum strings are validated against the task's project configuration.
- Both structs share the same schema in `docs/openapi.json`.

## Invariants & best practices

- Keep `created <= modified` when editing YAML manually; low-level persistence does not enforce it.
- Empty `tags`, `comments`, `references`, `history`, and relationship collections may be omitted. Relationship `parent` and `duplicate_of` are optional single values, not arrays.
- Explicit `assignee` values persist across status transitions; automation must clear them deliberately if needed.
- When exporting/importing YAML directly, use the storage field names: notably `type` in YAML corresponds to `task_type` in DTOs. Unknown top-level keys are preserved semantically as described above, not exposed as configured CLI fields.

See also: [OpenAPI spec](../openapi.json) for the full REST contract and [Identity & Users](./identity.md) for `reporter`/`assignee` resolution rules.
