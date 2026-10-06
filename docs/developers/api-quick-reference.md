# API Quick Reference

Endpoints with quick examples. For full schema see [OpenAPI](../openapi.json).

- POST /api/tasks/add (TaskCreate) -> { data: TaskDTO }
- GET  /api/tasks/list -> { data: { total, limit, offset, tasks? } }; `tasks` is a `TaskDTO[]` page and is
  omitted when the page is empty (treat missing as `[]`); empty page:
  `{ "data": { "total": 0, "limit": 50, "offset": 0 } }`, populated page:
  `{ "data": { "total": 1, "limit": 50, "offset": 0, "tasks": [ { "id": "QA-1", "title": "Wire contract tests", "status": "Todo", "priority": "Medium", "task_type": "Feature", "created": "2026-09-15T10:00:00+00:00", "modified": "2026-09-15T11:30:00+00:00" } ] } }`
- GET  /api/tasks/get?id=ID[&project=PREFIX][&include_deleted=true] -> { data: TaskDTO }; default reads hide deleted tasks
- POST /api/tasks/update (TaskUpdateRequest) -> { data: TaskDTO }
- POST /api/tasks/delete ({ id, hard?: bool }[?project=PREFIX]) -> { data: { deleted: bool, hard: bool, warnings: string[] } }; soft by default; hard deletion retains attachments and warns about retained attachment references and incoming task relationships
- POST /api/tasks/restore ({ id }[?project=PREFIX]) -> { data: TaskDTO }; restores without changing `modified`
- POST /api/sprints/create (SprintCreateRequest) -> { data: SprintCreateResponse }; configured sprint defaults apply unless `skip_defaults: true` (see `applied_defaults`, omitted when empty)
- POST /api/sprints/update (SprintUpdateRequest) -> { data: SprintUpdateResponse }; sprint must be >= 1 (0 -> 400); omitted fields are unchanged, `null` clears `capacity_points`/`capacity_hours`/`actual_started_at`/`actual_closed_at` while `null` on single-value fields is treated as omitted; 400 for unknown sprint ids
- POST /api/sprints/delete ({ sprint, cleanup_missing? }) -> { data: SprintDeleteResponse }; sprint must be >= 1; 404 for unknown sprint ids; tasks keep their data
- POST /api/tasks/attachments/upload ({ id, filename, content_base64 }) -> { data: { stored_path, attached, task } }; uploads attach typed `attachment` references
- POST /api/tasks/attachments/remove ({ id, stored_path }) -> { data: { task, deleted, still_referenced } }; 400 when the task lacks the managed `attachment` reference (blob untouched)
- POST /api/automation/simulate ({ ticket_id, event, project? }) -> { data: { matched, rule_name, actions, task_before, task_after } }; `event` is one of created, updated, assigned, commented, sprint_changed, job_started, job_completed, job_failed, job_cancelled (case-insensitive legacy aliases accepted); automation scope follows the ticket's project (`project` is ignored); `rule_name`/`task_after` are explicit `null` when no rule matched
- GET  /api/config/show[?project=PREFIX] -> { data: object }
- POST /api/config/set ({ values, global?, project? }) -> { data: { updated: bool } }
- GET  /api/events -> text/event-stream (see SSE Events)

Notes
- List/export support `deletion=active|deleted|all`, default `active`. Ordinary mutations reject deleted tasks until restored. Lifecycle operations preserve `modified` and append their own timestamped history.
- People fields accept `@me`.
- /api/tasks/list accepts additional query keys beyond the documented ones: declared custom field names can be used directly (e.g., `?sprint=W35`). Values support CSV and fuzzy matching (case/sep-insensitive).
- /api/tasks/update ignores `status` (status changes via CLI); other fields are updated.
- Validation errors return 400 with INVALID_ARGUMENT.

See also: [Identity & Users](./identity.md), [Task Model](./task-model.md), and [SSE Events](./sse.md).
