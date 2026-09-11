# Task Model

Canonical fields, enums, and invariants for tasks returned by REST, MCP, and CLI renderers. Sources of truth live in `src/api_types.rs` (DTO definitions), `src/types.rs` (strongly-typed enums/records), and the [OpenAPI spec](../openapi.json).

## Identity and IDs

- Tasks are stored under `.tasks/<PROJECT>/TASK.yml`. The project prefix is derived from the project name unless overridden (see `generate_project_prefix` in `src/project.rs`).
- IDs always follow `PREFIX-<NUMBER>` (e.g. `AUTH-42`). CLI commands accept the numeric portion when the active project is unambiguous (`lotar status 42`), otherwise pass the fully-qualified ID or `--project`.
- `created` timestamps are immutable and always precede or equal `modified`. Updates bump `modified` to the wall-clock time recorded by `TaskService`.
- **ID grammar (canonical parse):** the numeric suffix is the FINAL dash-separated segment, so prefixes may themselves contain dashes, digits, underscores, or Unicode letters (`ABC-OPS-12` -> project `ABC-OPS`, task `12`; `dev-ops-4` -> project `dev-ops`). Prefix grammar = `is_valid_project_prefix` in `src/storage/safety.rs`; the parser lives in `src/storage/identity.rs` (`TaskId::parse`).
- **Exact case, no folding:** IDs and project prefixes match exactly (`abc` and `ABC` are different projects).
- **Padded aliases are deliberate:** `TP-001` and `TP-1` denote the same task file (`TP/1.yml`); reads surface the canonical unpadded spelling in DTO `id` fields. Malformed IDs (empty, missing numeric suffix, non-digit suffix such as `TP-+12` or `TP-1-extra`, path traversal, `u64` overflow) fail closed.
- **Multiple storage roots:** `StorageLocator::candidate_task_roots` (primary + sibling workspace `.tasks`) drives unfiltered search, project-filtered search, full-ID get, and numeric resolution consistently. Config, sprint memberships, and attachments for a nested-root task come from the root that actually holds it.
- **Attachment store concurrency:** all LoTaR surfaces (REST upload/remove, MCP and CLI file references that point inside the attachments store) serialize blob creation, dedup, reference attach/detach, reclamation checks, and rollback cleanup on a cross-process advisory lock (`.attachments-store.lock` inside the store directory, fs2-backed, bounded contention retry that fails closed). Lock order is always store-lock -> task-lock. Cooperating LoTaR processes are covered; external non-cooperating writers that create or delete blobs in the store directory directly are explicitly unsupported and can produce orphans or dangling references.
- **Ambiguity fails closed:** duplicate full IDs or numbers across roots never resolve to an arbitrary task; resolvers report the candidate locations. Mutations refuse cross-root targets before any side effect (edit the task from inside its own workspace); `canonical_sync_task_id` keeps sync locked to the primary workspace via single-root reads. Listing endpoints enumerate every stored copy (duplicates appear as separate entries) rather than deduplicating an identity they cannot prove.

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
`effort` | `string?` | Stored exactly as provided (e.g., `3h`, `5pts`).
`subtitle` | `string?` | Short secondary label; omitted unless explicitly set.
`description` | `string?` | Markdown-friendly long description.
`tags` | `string[]` | Normalized, unique tags. Empty array when unset.
`relationships` | `TaskRelationships` | Structured references to other tasks (see below).
`comments` | `TaskComment[]` | Each comment carries `{ date, text }`.
`references` | `ReferenceEntry[]` | Code locations (`code`) and/or external URLs (`link`).
`sprints` | `u32[]` | Numeric sprint IDs the task belongs to.
`sprint_order` | `BTreeMap<u32, u32>` | Optional manual ordering per sprint (task id → order index).
`history` | `TaskChangeLogEntry[]` | Chronological change log entries (field deltas, actor, timestamp).
`custom_fields` | `CustomFields` | Map of configured custom-field keys → YAML/JSON values. Skipped when empty.

### Relationships & related structs

- `TaskRelationships` exposes dedicated arrays for `depends_on`, `blocks`, `related`, `children`, `fixes`, plus single-value `parent` and `duplicate_of`. All properties are optional; empty collections are dropped on serialization.
- `TaskComment` holds `{ date: RFC3339, text: string }`. Comments do not store authorship today.
- `ReferenceEntry` supports either `code` (e.g., `src/lib.rs:120`) or `link` (URL). Multiple references may be attached to track external systems.
- `TaskChangeLogEntry` captures `{ at, actor?, changes[] }`, where each `TaskChange` includes `field`, `old`, and `new` values for audit review.

### Custom fields

`custom_fields` is a free-form map backed by `HashMap<String, serde_yaml::Value>` (or JSON when schema generation is enabled). Keys correspond to names declared in `config.custom_fields` or dynamic fields permitted by `custom_fields.values`. CLI and REST mutate them via `field:<name>` syntax, while listing/filtering converts them back into strings using `custom_value_to_string`.

### Sprints & ordering

`sprints` mirrors the sprint memberships stored on the task file. When sprint assignment commands enable manual ordering, `sprint_order` stores a per-sprint sequence so boards and reports can render deterministic lanes. Both properties are managed by `SprintService` and `sprint_assignment` helpers.

## Enumerations & config-driven values

- `TaskStatus`, `Priority`, and `TaskType` are thin wrappers around strings (see `src/types.rs`). They validate against the arrays declared in `config.yml` under `issue_states`, `issue_priorities`, and `issue_types`. The examples below are defaults; workspaces routinely customize them.
	- `issue_states`: e.g., `TODO`, `IN_PROGRESS`, `VERIFY`, `BLOCKED`, `DONE`, `CANCELED`.
	- `issue_priorities`: e.g., `Low`, `Medium`, `High`, `Critical`, `Blocker`.
	- `issue_types`: e.g., `feature`, `bug`, `epic`, `spike`, `chore`.
- Because these values are data-driven, client code should not assume a fixed enum list; always render the exact casing stored on the task.

## Create/update payloads

- `TaskCreate` accepts `title`, optional `project`, and optional metadata for `priority`, `task_type`, `reporter`, `assignee`, `due_date`, `effort`, `description`, `tags`, `relationships`, `custom_fields`, and `sprints`. Missing properties are defaulted downstream.
- `TaskUpdate` treats every field as optional and only patches values that are present in the JSON body. Passing `tags` replaces the entire list; omitting it leaves tags untouched.
- Both structs live in `src/api_types.rs` and share the same schema in `docs/openapi.json`.

## Invariants & best practices

- `created <= modified` (enforced by `TaskService`).
- `tags`, `comments`, `references`, `history`, and `relationships.*` are always arrays even when empty, simplifying client iteration.
- Explicit `assignee` values persist across status transitions; automation must clear them deliberately if needed.
- When exporting/importing YAML directly, keep field names lower_snake_case to match the DTOs. Unknown keys are preserved by serde but ignored by CLI readers.

See also: [OpenAPI spec](../openapi.json) for the full REST contract and [Identity & Users](./identity.md) for `reporter`/`assignee` resolution rules.
