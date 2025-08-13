# MCP + Web API Foundation Plan (Tasks & Projects)

Last updated: 2025-08-08 (evening, MCP tools wired + logging/setLevel)
Status: In progress (MCP + Web API foundations)
Owners: Core CLI + Storage maintainers

## Goals

- Expose core project and task operations over two channels:
  1) Web API (JSON over HTTP) to power the React UI and programmatic clients
  2) MCP tools (JSON-RPC/stdio) to let AI agents interact with LoTaR safely
- Keep behavior consistent with the existing CLI and storage semantics (YAML, project isolation).
- Keep it small and dependency-light; reuse OutputRenderer for diagnostics.

## Non-Goals (for the initial foundation)

- Authentication/authorization
- External ticket integrations (GitHub/Jira/Linear)
- Bulk/streaming operations and long-running jobs
- Advanced relationships graph traversal APIs

## Guiding Principles

- Single source of truth: storage layer (YAML files per project)
- Thin transports (Web/MCP) over a common service layer
- Strict separation of user payload (JSON) from diagnostics (stderr logs)
- Backwards-compatible IDs and file layout (PROJ-123)

---

## Current State (Quick Analysis)

- Storage
  - CRUD: `storage::operations` (add/get/edit/delete), `storage::manager::Storage` facade
  - Search: `storage::search` with `TaskFilter`
  - Types: `storage::task::Task` (no ID field in-file; derived from path), common enums in `types.rs`
- CLI
  - Handlers for add/list/edit/delete/status/priority; good validation and logging hygiene
- Web
  - `web_server.rs` implements a minimal TCP HTTP server with routing; serves static files from `target/web` and `/api/*` dispatch via `api_server::ApiServer`
  - Structured request parsing (method, path, query, headers, body), permissive CORS, and SSE endpoints `/api/events` and `/api/tasks/stream` are implemented
  - SSE supports per-connection debounce and filters via query params; filesystem watcher integration is still pending
- MCP
  - Implemented JSON-RPC 2.0 stdio server with MCP handshake (protocolVersion "2025-06-18")
  - Tools exposed (see MCP Tools below); accepts both legacy slash names and host-compatible underscore names
  - Added logging capability and logging/setLevel handler (stores level; keeps stdout clean)

Implication: We need a real API surface (methods, bodies, errors) and a thin adapter in both Web and MCP to call into a new service layer.

---

## Proposed Architecture

- Core services layer (new): `src/services/`
  - `task_service.rs`
    - Dependencies: `storage::manager::Storage`, `storage::operations`, `storage::search`
    - Functions:
      - create_task(req: TaskCreate, ctx: Context) -> Result<TaskDTO>
      - get_task(id: &str, ctx) -> Result<TaskDTO>
      - update_task(id: &str, req: TaskUpdate, ctx) -> Result<TaskDTO>
      - delete_task(id: &str, ctx) -> Result<bool>
      - list_tasks(filter: TaskListFilter, ctx) -> Result<Vec<TaskDTO>>
    - Notes: Implements project isolation; maps file-backed Task to API DTO by attaching `id`
  - `project_service.rs`
    - Functions:
      - list_projects(ctx) -> Result<Vec<ProjectDTO>> (directory names under tasks root)
      - project_stats(project: &str, ctx) -> Result<ProjectStatsDTO>

- Shared DTOs (new): `src/api_types.rs`
  - TaskDTO { id, title, status, priority, task_type, assignee, created, modified, due_date, effort, subtitle?, description?, category?, tags[], custom_fields{}, relationships, comments[] }
  - TaskCreate { title, project?, priority?, task_type?, assignee?, due_date?, effort?, description?, category?, tags[], custom_fields{} }
  - TaskUpdate { same as create but all Option<...> to PATCH semantics; status?, priority? }
  - TaskListFilter { status[], priority[], task_type[], project?, category?, tags[], text_query? }
  - ProjectDTO { name, prefix }
  - ProjectStatsDTO { name, open_count, done_count, recent_modified, tags_top[] }
  - Error payload: { code: string, message: string, details?: object }

- Web API adapter (evolve existing): `api_server.rs` + `routes.rs`
  - Introduce basic HTTP parsing for:
    - Method (GET/POST/PATCH/DELETE)
    - Path params (e.g., /api/v1/tasks/{id})
    - Query params (for filters, pagination)
    - JSON request/response bodies (serde_json)
  - Routing table keyed by (method, path pattern)
  - Consistent error responses and status lines
  - Keep the server minimal (no external framework) for now; we can swap later if needed
  - Add SSE broadcast manager for realtime updates (multiple clients/tabs supported)
  - Add filesystem watcher (notify) to emit task/project change events
  - Enable permissive CORS by default for third‑party UIs and tools

- MCP Server (new): `src/mcp/`
  - Minimal JSON-RPC 2.0 over stdio process implementing MCP tools
  - Tools exposed initially:
  - task/create, task/get, task/update, task/delete, task/list
  - project/list, project/stats
  - Payloads map 1:1 to DTOs above; transport layer only wraps JSON-RPC envelopes
  - Rate limits, capabilities, and batching can be added later

- Logging
  - Reuse `OutputRenderer` for diagnostics to stderr in both servers
  - All API/MCP payloads are JSON-only on stdout/network responses

---

## HTTP API Surface

Base path: `/api`
Content-Type: application/json

Endpoint style
- Prefer action-style aliases to align with CLI and reduce mapping logic.
- Examples: `/api/tasks/add`, `/api/tasks/update`, `/api/tasks/delete`, `/api/tasks/list`, `/api/tasks/stream`.

- POST /tasks/add
  - Body: TaskCreate
  - 201 { data: TaskDTO }
- POST /tasks/update
  - Body: { id: string, patch: TaskUpdate }
  - 200 { data: TaskDTO } | 404 { error }
- POST /tasks/delete
  - Body: { id: string }
  - 200 { data: { deleted: true } } | 404 { error }
- GET /tasks/get
  - Query: id
  - 200 { data: TaskDTO } | 404 { error }
- GET /tasks/list
  - Query: status, priority, type, project, tags, category, q
  - 200 { data: TaskDTO[], meta: { count } }
- GET /tasks/stream (SSE streaming list)
  - Content-Type: text/event-stream
  - Streams results while scanning the filesystem
  - Events:
    - `event: task_list_item` with `data: { task: TaskDTO }` per match
    - `event: task_list_done` with `data: { count: number }` when complete

- GET /projects/list
  - 200 { data: ProjectDTO[] }
- GET /projects/stats
  - Query: name
  - 200 { data: ProjectStatsDTO } | 404 { error }

- GET /config/show
  - Query: global=true|false, project? (if omitted, default project resolution applies)
  - 200 { data: GlobalConfig | ProjectConfig }
- POST /config/set
  - Body: { global?: bool, project?: string, values: { <key>: <value>, ... } }
  - 200 { data: { updated: true } } | 400 { error } | 404 { error }

Realtime events (SSE)
- GET /events (SSE)
  - Content-Type: text/event-stream
  - Multiple concurrent clients supported
  - Event types and payloads (full payloads for great DX):
    - `task_created`  data: { task: TaskDTO }
    - `task_updated`  data: { task: TaskDTO }
    - `task_deleted`  data: { id: string }  (Note: full task payload may be unavailable post-delete)
    - `project_changed` data: { name: string }
  - `config_updated` data: { scope: "global" | "project", project?: string }
  - Per-connection filters (implemented):
    - `debounce_ms` (number, default 100) — debounce before emitting buffered events
    - `kinds` (CSV) — only emit matching event kinds, e.g. `kinds=task_created,task_updated`
    - `project` (string) — filter events by ID prefix (e.g. `LOTAR`)
  - Keep-alive comments (not yet implemented)

Error model
- 4xx/5xx always: { error: { code, message, details? } }
- Examples: INVALID_ARGUMENT, NOT_FOUND, CONFLICT, INTERNAL

Notes
- No versioning; UI and API ship together and stay in sync
- Streaming preferred to pagination for list/search (local-app model)
- IDs remain `PROJ-123`

Parameter mapping & CLI parity
- For GET endpoints: reuse CLI long option names as query parameter keys (snake_case to match DTOs), e.g., `--task-type` → `task_type`.
- For POST: JSON body keys mirror DTO field names (snake_case), aligned with CLI long options.
- Short flags are not exposed as param names; they are CLI-only conveniences.
- Config commands: align `lotar config` to support multiple `--key=value` in one call; API `/config/set` accepts a `values` object with the same keys.

---

## MCP Tools (initial set)

Tool names (host-facing):
- task_create(params: TaskCreate) -> { task: TaskDTO }
- task_get(params: { id: string, project?: string }) -> { task: TaskDTO }
- task_update(params: { id: string, patch: TaskUpdate }) -> { task: TaskDTO }
- task_delete(params: { id: string, project?: string }) -> { deleted: bool }
- task_list(params: TaskListFilter) -> { tasks: TaskDTO[] }
- project_list(params: {}) -> { projects: ProjectDTO[] }
- project_stats(params: { name: string }) -> { stats: ProjectStatsDTO }
- config_show(params: { global?: bool, project?: string }) -> { config: GlobalConfig | ProjectConfig }
- config_set(params: { global?: bool, project?: string, values: Map<string, any> }) -> { updated: bool }

Notes
- The server also accepts legacy slash names (e.g., task/create) for compatibility. Hosts must use [a-z0-9_-] names; tools/list advertises underscores.
- logging/setLevel is supported to avoid host errors; we don't emit logs over MCP.

Transport
- JSON-RPC 2.0 messages over stdio
- Errors surface as JSON-RPC errors with { code, message, data }
 - No streaming over MCP (tools return complete results); realtime is via REST SSE

---

## Implementation Plan (Phased)

Phase 0 – Scaffolding (PR1)
- Add `src/services/` with TaskService and ProjectService skeletons
- Add `src/api_types.rs` DTOs and serde derives
- Wire services to storage and convert Task <-> TaskDTO
- Unit tests for services
 - Add optional `schema` cargo feature and `schemars` derive on DTOs to generate JSON Schemas for third‑party devs
 - Add a tiny schema export harness (dev-only) that writes `docs/schemas/*.json`

Phase 1 – Web API + SSE (PR2)
- Extend `api_server.rs` to parse method, path, query, JSON body
- Add router with (method, pattern) matching
- Implement action-style endpoints for tasks and projects using services
- Implement SSE `/events` feed with full payloads for created/updated (id-only for deleted), multi-client broadcast
- Add filesystem watcher (notify) with configurable debounce (default 100ms when unset) and change normalization
- Add CORS allow-all headers and OPTIONS handling (preflight)
- Add tests: in-process HTTP request simulation and simple SSE test harness
 - Add config endpoints: `GET /api/config/show`, `POST /api/config/set` (global and project)
 - Emit `config_updated` SSE after successful writes

- Phase 2 – MCP minimal (PR3)
- [x] Implement MCP server with JSON-RPC over stdio (initialize, tools/list, tools/call)
- [x] Tools: task_create/get/update/delete/list; project_list/stats; config_show/set (no streaming)
- [x] Accept both underscore and slash method names
- [x] Add logging capability and logging/setLevel handler
- [ ] Map DTOs and errors; tests for basic tool calls (add a small in-proc harness)
 - Add config tools: `config/show`, `config/set` (unary)

Phase 3 – Hardening & DX (PR4)
- Input validation and helpful error messages
- CORS config surface (origins/methods/headers), default remain permissive
- Optional streaming list via SSE for `/tasks` based on `Accept: text/event-stream` (if not in PR2)
- Logging polish: BEGIN/END markers per request and per SSE broadcast

Optional Phase – Swap to a tiny HTTP crate later
- If manual HTTP parsing becomes a maintenance burden, consider `tiny_http` or `hyper`/`axum` with a feature flag

---

## Data Mapping Details

- Task file has no embedded ID; derive id from directory and filename (existing behavior)
- TaskDTO.id = formatted "{project_prefix}-{numeric_id}"
- ProjectDTO.name = directory name under tasks root; prefix = same
- TaskCreate.project (optional):
  - If provided, use it to resolve project prefix
  - Else derive from global config default_prefix or folder detection (existing logic)
  - SSE events for create/update include full TaskDTO payload; delete includes id only

---

## Error Handling & Status Codes

- Map storage errors to user-facing errors without leaking paths
- NOT_FOUND for unknown IDs or project mismatches
- INVALID_ARGUMENT for bad enums/filters
- INTERNAL for unexpected errors (include a short correlation id in logs later)
 - Proper HTTP status codes for REST; SSE emits error events for stream failures where feasible

---

## Testing Strategy

- Services unit tests (pure): happy path + edge cases (missing project, invalid enum)
- Web API integration tests: craft raw HTTP requests and parse JSON
- SSE tests: connect to `/events`, assert event types and payload shapes; simulate file changes and verify broadcasts
- MCP tests: send JSON-RPC requests and assert tool results
- Reuse fixtures/builders from existing test framework where possible

---

## Migration & Compatibility

- CLI remains source of truth for behavior; services reuse the same validation paths where possible
- Keep static web serving unchanged; mount API at `/api`
 - Developer Experience: provide permissive CORS, consistent JSON DTOs, example requests in docs, and stable event types for third-party UIs/tools

## Developer Experience

- CORS: `Access-Control-Allow-Origin: *` and permissive defaults (methods/headers) out of the box
- Consistent JSON DTOs across REST and SSE
- Example snippets (curl and EventSource) in docs; optional JSON Schemas for DTOs as a follow-up
- Health endpoint (optional): `GET /api/health` returns { ok: true }
- Discoverability (optional follow-up): static OpenAPI doc in docs/ mapping endpoints and DTOs
 - JSON Schemas: use `schemars` (behind `schema` feature) to auto-generate schemas for DTOs; publish under `docs/schemas/` for external UIs/tools

Configuration
- SSE debounce: configurable via env var `LOTAR_SSE_DEBOUNCE_MS`; per-connection override via query `debounce_ms`.
- Default debounce is 100ms when not set; no default value written to config.yml.
 - Config editing: multiple fields can be set via CLI and API; rely on git for history; write-through updates (no temp/backup files)
 - Validation: disallow unknown config keys; reuse existing config validation logic across CLI/Web/MCP
 - Project rename: only updates display name; prefix/folder rename is a separate migration and must not cause collisions

---

## Risks & Mitigations

- Manual HTTP parsing fragility → keep scope small; add tests; consider tiny_http later
- MCP protocol drift → keep tools minimal; align with docs/mcp-integration-specification.md

---

## Acceptance Criteria (Foundation)

- Services layer exposes task/project ops with tests
- Web API responds correctly to CRUD/list and project endpoints with JSON payloads and proper status lines
- MCP server supports minimal tool set with JSON-RPC over stdio
- Logging hygiene preserved; stdout JSON only for API/MCP payloads

---

## Open Questions

1) HTTP stack: stay with our minimal server for now, or is adopting a small crate (tiny_http) acceptable if we hit parsing limits?
• Answered: Small library acceptable.
2) API pathing: prefer versioned base path?
• Answered: No versioning; UI and API ship together.
3) Pagination vs streaming for `/tasks`?
• Answered: Prefer streaming while scanning the filesystem.
4) MCP tool naming convention?
• Answered: Slash-separated names (e.g., `task/create`).
5) Feature gating?
• Answered: Enabled by default in initial release.
6) Error codes policy?
• Answered: Use proper HTTP status codes; body may include details.
7) CORS policy?
• Answered: Allow-all by default; users can proxy to restrict.
8) Project discovery?
• Answered: Reuse existing CLI project resolution logic.

New clarifications
9) Delete events: Since full payload may not be available after file removal, is `task_deleted` with `{ id }` acceptable? (Current plan assumes yes.)
Yes
10) SSE list streaming: Use dedicated `/tasks/stream` (do not overload `GET /tasks`).
11) Burst behavior: We’ll debounce watcher events (~100ms) to avoid floods; any stricter coalescing desired, or is this sufficient?
should be good enough
12) Connection limits: Any expected upper bound for concurrent SSE clients (tabs/tools)? We’ll target dozens without issue on localhost.
13) MCP streaming: Confirmed not planned; tools will be unary. Realtime consumers should use REST SSE.
Yeah that target sounds alright. I wouldn't put too much effort into optimizing this.

Config-specific
14) Project rename semantics: Answered — display name only; prefix/folder rename is separate. Also validate to prevent renaming to a value that matches any existing project prefix.
15) Config key validation: Answered — restrict to known keys and reuse existing validation; reject unknown keys.
16) Atomicity & backup: Answered — no atomic writes/backups required; rely on git; write-through updates are acceptable.

---

## Work items checklist

PR1 – Services & DTOs
- [ ] Create `src/api_types.rs` with DTOs (serde, optional schemars) and error payload
- [ ] Create `src/services/task_service.rs` and `src/services/project_service.rs` skeletons
- [ ] Map storage to DTOs (id derivation) and implement: create/get/update/delete/list, project list/stats
- [ ] Unit tests for services (happy path + basic edge cases)
- [ ] Add `schema` feature and dev-only schema export to `docs/schemas/`

PR2 – Web API & SSE
- [x] Extend `api_server.rs` router: method, path, query, JSON body parsing
- [x] Implement REST endpoints: POST/GET/PATCH/DELETE `/api/tasks`, GET `/api/tasks`, GET `/api/projects`, GET `/api/projects/{name}/stats`
- [x] Implement SSE endpoints: GET `/api/events`, GET `/api/tasks/stream`
- [ ] Watcher: notify integration, configurable debounce (default 100ms), change normalization
- [x] Broadcast manager for multi-client SSE; full payloads for create/update, id-only for delete
- [x] CORS allow-all; OPTIONS preflight handling
- [x] Parameter parity: map CLI long options to query/body keys (snake_case)
- [x] Tests for SSE streams (basic); REST route tests exist

PR3 – MCP (Unary)
- [x] Implement minimal MCP server with JSON-RPC over stdio (scaffold)
- [ ] Tools: task/create,get,update,delete,list; project/list,stats (no streaming)
- [ ] Map DTOs and errors; tests for basic tool calls

PR4 – Hardening & DX
- [ ] Input validation and helpful error messages
- [ ] Request logging polish (BEGIN/END per request and per SSE broadcast)
- [ ] Optional OpenAPI doc and examples; publish JSON Schemas
- [ ] Performance passes and small fixes