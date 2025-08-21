# API Quick Reference

Endpoints with quick examples. For full schema see [OpenAPI](../openapi.json).

- POST /api/tasks/add (TaskCreate) -> { data: TaskDTO }
- GET  /api/tasks/list -> { data: TaskDTO[], meta: { count } }
- GET  /api/tasks/get?id=ID[&project=PREFIX] -> { data: TaskDTO }
- POST /api/tasks/update (TaskUpdateRequest) -> { data: TaskDTO }
- POST /api/tasks/delete ({ id }) -> { data: { deleted: bool } }
- GET  /api/config/show[?project=PREFIX] -> { data: object }
- POST /api/config/set ({ values, global?, project? }) -> { data: { updated: bool } }
- GET  /api/events -> text/event-stream (see SSE Events)

Notes
- People fields accept `@me`.
- /api/tasks/list accepts additional query keys beyond the documented ones: declared custom field names can be used directly (e.g., `?sprint=W35`). Values support CSV and fuzzy matching (case/sep-insensitive).
- /api/tasks/update ignores `status` (status changes via CLI); other fields are updated.
- Validation errors return 400 with INVALID_ARGUMENT.

See also: [Identity & Users](./identity.md), [Task Model](./task-model.md), and [SSE Events](./sse.md).
