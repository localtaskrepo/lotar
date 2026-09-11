# API Quick Reference

Endpoints with quick examples. For full schema see [OpenAPI](../openapi.json).

- POST /api/tasks/add (TaskCreate) -> { data: TaskDTO }
- GET  /api/tasks/list -> { data: TaskDTO[], meta: { count } }
- GET  /api/tasks/get?id=ID[&project=PREFIX] -> { data: TaskDTO }
- POST /api/tasks/update (TaskUpdateRequest) -> { data: TaskDTO }
- POST /api/tasks/delete ({ id }) -> { data: { deleted: bool } }
- POST /api/tasks/references/add (GenericReferenceAddRequest) -> { data: GenericReferenceAddResponse }
- POST /api/tasks/references/remove (GenericReferenceRemoveRequest) -> { data: GenericReferenceRemoveResponse }
- GET  /api/config/show[?project=PREFIX] -> { data: object }
- POST /api/config/set ({ values, global?, project? }) -> { data: ConfigSetResponse }
- POST /api/scan/run (ScanRequest) -> { data: ScanResponse }
- POST /api/sync/pull (SyncRequest) -> { data: SyncResponse }
- POST /api/sync/push (SyncRequest) -> { data: SyncResponse }
- POST /api/sync/validate (SyncValidateRequest) -> { data: SyncValidateResponse }
- GET  /api/sync/reports/list?project=PREFIX&limit=N&offset=N -> { data: SyncReportListResponse }
- GET  /api/sync/reports/get?path=<relative>[&project=PREFIX] -> { data: SyncReport }
- POST /api/jobs (AgentJobCreateRequest) -> { data: AgentJobCreateResponse }
- GET  /api/jobs?ticket_id=ID&status=STATUS -> { data: AgentJobListResponse }
- GET  /api/jobs/get?id=JOB_ID -> { data: AgentJobStatusResponse }
- POST /api/jobs/cancel ({ id }) -> { data: AgentJobCancelResponse }
- POST /api/jobs/cancel-all -> { data: AgentJobCancelAllResponse }
- GET  /api/jobs/logs?id=JOB_ID -> { data: AgentJobLogsResponse }
- GET  /api/automation/show[?project=PREFIX] -> { data: AutomationInspectResponse }
- POST /api/automation/set (AutomationSetRequest) -> { data: AutomationSetResponse }
- POST /api/jobs/message (AgentJobMessageRequest) -> { data: AgentJobMessageResponse }
- GET  /api/events -> text/event-stream (see SSE Events)

Notes
- People fields accept `@me`.
- /api/tasks/list accepts additional query keys beyond the documented ones: declared custom field names can be used directly (e.g., `?sprint=W35`). Values support CSV and fuzzy matching (case/sep-insensitive).
- Task mutations (add/update/status) validate `status`/`priority`/`type` strings against the target project's resolved configuration, so project-only enum values are accepted and out-of-set values return 400.
- `/api/tasks/add` accepts `status` for atomic initial status plus `custom_fields` (object) and `acceptance_criteria[]`; legacy `fields` key/value payloads still work with `custom_fields` winning per key.
- Patch null semantics are explicit: omitted = no-op, `null` = clear, value = set. Empty string clears clearable scalars (reporter/assignee/due_date/effort/description); empty array/object clears `tags`/`acceptance_criteria`/`relationships`/`custom_fields`/`sprints`. `title`/`status`/`priority`/`type` treat `null` as omitted and blank titles are rejected. List and map patches replace the whole value; an explicit `tags` clear does not re-apply configured `default_tags`.
- `custom_fields` replace-all patches keep keys already present on the task even when removed from project config, canonicalize configured keys to their configured spelling, and reject brand-new undeclared keys. `sprints` entries must be positive integers; invalid entries are rejected (REST and MCP).
- Validation errors return 400 with INVALID_ARGUMENT.
- `assignee=@me` that cannot be resolved fails closed with a 400 error rather than returning an unfiltered list.
- `/api/tasks/export` supports the same filter query parameters as `/api/tasks/list`.

See also: [Identity & Users](./identity.md), [Task Model](./task-model.md), and [SSE Events](./sse.md).
