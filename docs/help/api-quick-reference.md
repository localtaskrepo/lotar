# API Quick Reference

Endpoints with quick examples. For full schema see [OpenAPI](../openapi.json).

- POST /api/tasks/add (TaskCreate) -> { data: TaskDTO }; with `finding` -> { data: TaskDTO, finding: { created, state, id } } (201 created, 200 already reported; see [Findings](add.md#findings))
- GET  /api/tasks/list -> { data: { total, limit, offset, tasks? } }; `tasks` is a `TaskDTO[]` page and is
  omitted when the page is empty (treat missing as `[]`); empty page:
  `{ "data": { "total": 0, "limit": 50, "offset": 0 } }`, populated page:
  `{ "data": { "total": 1, "limit": 50, "offset": 0, "tasks": [ { "id": "QA-1", "title": "Wire contract tests", "status": "Todo", "priority": "Medium", "task_type": "Feature", "created": "2026-09-15T10:00:00+00:00", "modified": "2026-09-15T11:30:00+00:00" } ] } }`
- GET  /api/tasks/get?id=ID[&project=PREFIX][&include_deleted=true] -> { data: TaskDTO }; deleted tasks are hidden unless explicitly included. Every `TaskDTO` has `url` (`<server.public_url>/task/<ID>`) while [`server.public_url`](./config-reference.md) is set
- POST /api/tasks/update (TaskUpdateRequest) -> { data: TaskDTO, deferred?: JobOutcome }; `deferred` when an agent job runs on the ticket and holds the status/assignee change for its end ([details](agent.md#status-and-assignee-during-a-job)); optional `expected_modified` -> 409 CONFLICT if the task changed since
- POST /api/tasks/delete ({ id, hard?: bool }[?project=PREFIX]) -> { data: { deleted: bool, hard: bool, warnings: string[] } }; defaults to soft delete; hard deletion retains attachment blobs and warns about their references and incoming task relationships
- POST /api/tasks/restore ({ id }[?project=PREFIX]) -> { data: TaskDTO }; restores a soft-deleted task without changing `modified`
- POST /api/tasks/archive and /api/tasks/unarchive ({ id }[?project=PREFIX]) -> { data: TaskDTO }; moves the task into its project's `@archive` folder (read-only) or back, without changing `modified`
- POST /api/tasks/move ({ id, project, status?, task_type?, priority?, reporter?, assignee?, custom_fields?, dry_run? }) -> { data: { from, to, changes, dry_run, task: TaskDTO } }; moves the task to another project under the next free number there. The old ID stays a redirect: every endpoint that takes a task ID accepts it and answers for the moved task (its DTO lists `moved_from`). Values the target lacks fall back to its defaults unless set; disallowed custom fields return 400 until set or dropped (`null`). Emits `task_deleted` for the old ID (with `moved_to`) and `task_created` for the new one.
- GET /api/relationships/graph[?task_id=ID&project=PREFIX&depth=N&max_nodes=N&filters=JSON] -> { data: RelationshipGraphResponse }; typed prerequisite-to-dependent edges with provenance, explicit unresolved nodes, diagnostics and completeness/truncation. `filters` is a JSON-encoded string-to-string map with the `/api/tasks/list` grammar (including smart/custom filters, ordering and `query` in the web search syntax); invalid values return 400. Matching tasks are selected before the node bound, while referenced nonmatching endpoints and a filtered-out focus remain explicit context (`matches_filter: false`). Resolvable nodes include a compact `task` metadata summary, not full descriptions/comments/history. The TypeScript client encodes the map; MCP `relationship_graph` accepts the map as an optional object.
- GET /api/relationships/check[?project=PREFIX] -> { data: RelationshipCheckResponse }; read-only findings and snapshot revision
- GET /api/relationships/cross-repo?refs=REPO:ID[,REPO:ID...] -> { data: { [ref]: CrossRepoTask } }; tasks in another repo's LoTaR that `depends_on` and `related` name, read now: `reference`, `title`, `status`, `done` and `url`, or `unresolved` with the reason. Keyed by each reference as requested; at most 100.
- POST /api/relationships/mutate (RelationshipMutationRequest) -> { data: RelationshipMutationResponse }; one atomic add/remove/change-kind operation, maintaining reciprocals without replacing unrelated categories
- POST /api/relationships/repair (RelationshipRepairRequest) -> { data: RelationshipRepairResponse }; preview by default; applying conservative repairs requires `apply: true` and the unchanged preview `revision`
- POST /api/tasks/references/file/add (FileReferenceAddRequest) -> { data: FileReferenceAddResponse }; 400 when the path resolves inside the managed attachments store
- POST /api/tasks/references/file/remove (FileReferenceRemoveRequest) -> { data: FileReferenceRemoveResponse }; repository files are never deleted
- POST /api/tasks/attachments/upload ({ id, filename, content_base64 }) -> { data: { stored_path, attached, task, metadata_removed?, warnings? } }; uploads attach typed `attachment` references. JPEG/PNG/WebP images lose camera, location and other personal metadata before storing (`metadata_removed: true`; pixels are not re-encoded, orientation and colour profiles stay); an image extension on non-image bytes adds a warning
- POST /api/tasks/attachments/reorder ({ id, attachments: [stored names] }) -> { data: task }; must list every attachment of the task once; the first image is the web UI's card cover
- GET  /api/branding -> { data: { theme?, theme_source?, repository_theme?, theme_file?, name?, repository?, logo?, logo_dark?, icon?, detected_icon?, problems? } }; `theme` is `skin` or `skin:palette` (or `custom`); `theme_file` is the repository skin when usable; `theme_source` is `home` (~/.lotar, which wins) or `repository`; `repository_theme` is the repository's value when ~/.lotar overrides it; image fields are URLs (with a content version) present only when the configured file exists inside the repository and is an image; `repository` is the repository's folder name (the tab title without `name`); without `ui.icon`, `icon` is the repository's own icon when one is found (`detected_icon` names the file: `favicon.svg|ico|png` or `icon.svg|png` at the root or in `public/`, `static/`, `assets/`, `app/`, `src/`, `src/app/`, `web/`); `problems` says why one is not shown
- GET  /api/branding/{logo|logo-dark|icon} -> the configured image, or the repository's own icon for `icon` (no-cache + ETag; SVG sandboxed by CSP); 404 when not configured or not servable
- GET  /api/branding/theme.css -> the repository skin reduced to theme variables, scoped to `[data-theme-name='custom']` (empty stylesheet without one; no-cache + ETag)
- POST /api/branding/upload ({ image: logo|logo-dark|icon|theme, filename, content_base64 }) -> { data: { path } }; stores the image (≤ 2 MiB, metadata stripped) as `.tasks/@branding/<image>.<ext>`, or a `.css` skin with at least one theme variable (≤ 64 KiB) as `.tasks/@branding/theme.css` and returns its repository path for `ui.logo` / `ui.logo_dark` / `ui.icon`; settings are saved separately
- POST /api/attachments/info ({ project?, attachments: [stored names] }) -> { data: { items: [{ attachment, size, width?, height? }] } }
- POST /api/tasks/attachments/remove ({ id, stored_path }) -> { data: { task, deleted, still_referenced } }; 400 when the task lacks the managed `attachment` reference (blob untouched)
- GET  /api/attachments/get?path=<stored-leaf>[&project=PREFIX][&download=1|true] -> file bytes (Content-Disposition inline/attachment); 404 when absent
- GET  /api/attachments/h/<32-hex-hash>/<filename>[?project=PREFIX][&download=1|true] -> file bytes looked up by content hash; cached as immutable with the hash as ETag (If-None-Match -> 304). Attachment responses send `nosniff`, SVGs are sandboxed by CSP, and HTML is served as `application/octet-stream`. `&w=<px>` returns a downscaled preview (snapped to 96/240/480/960/1600; generated on demand and kept in memory only, idle entries dropped after an hour). HEAD works like GET without the body; single `Range: bytes=` requests get 206 (video/audio streaming and seeking); non-ASCII names use RFC 5987 `filename*`
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
- GET  /api/agents/profiles[?project=PREFIX] -> { data: { profiles: AgentProfileInfo[], installed_runners: string[], project } } (name, runner, model?, effort?, permissions), sorted by name; without `project` the default project is used
- POST /api/agents/profiles (AgentProfileCreateRequest: name, runner, project?, global?, model?, effort?, permissions?) -> { data: ConfigSetResponse }; refuses a name that already exists for that scope
- POST /api/jobs (AgentJobCreateRequest; optional `model`/`effort`/`provider`/`session` override the profile for this run) -> { data: AgentJobCreateResponse }
- GET  /api/jobs?ticket_id=ID&status=STATUS -> { data: AgentJobListResponse }
- GET  /api/jobs/get?id=JOB_ID -> { data: AgentJobStatusResponse }
- POST /api/jobs/cancel ({ id }) -> { data: AgentJobCancelResponse }
- POST /api/jobs/cancel-all -> { data: AgentJobCancelAllResponse }
- GET  /api/jobs/logs?id=JOB_ID -> { data: AgentJobLogsResponse }
- GET  /api/automation/show[?project=PREFIX] -> { data: AutomationInspectResponse }
- POST /api/automation/set (AutomationSetRequest) -> { data: AutomationSetResponse }
- POST /api/sprints/create (SprintCreateRequest) -> { data: SprintCreateResponse }; configured sprint defaults apply unless `skip_defaults: true` (see `applied_defaults`)
- POST /api/sprints/update (SprintUpdateRequest) -> { data: SprintUpdateResponse }; sprint must be >= 1 (0 -> 400); omitted fields are unchanged, `null` clears `capacity_points`/`capacity_hours`/`actual_started_at`/`actual_closed_at` while `null` on single-value fields is treated as omitted; 404 for unknown sprint ids
- POST /api/sprints/delete ({ sprint, cleanup_missing? }) -> { data: SprintDeleteResponse }; sprint must be >= 1; 404 for unknown sprint ids; tasks keep their data
- POST /api/sprints/add | /api/sprints/remove (SprintAssignmentRequest: { sprint?, tasks?, query?, allow_closed?, force_single?, cleanup_missing? }) -> { data: SprintAssignmentResponse }; `query` also acts on the tasks it selects (web search syntax, the shared task query)
- GET /api/sprints/backlog[?status&tags&assignee&project&query&limit&cleanup_missing] -> { data: SprintBacklogResponse }; tasks in no sprint, selected with the `/api/tasks/list` grammar (including `query`; `tag` is an alias of `tags`), in manual backlog order unless `sort_by` is given; invalid values return 400
- GET /api/releases/list -> { data: ReleaseListResponse }; releases in version order with state, dates and task counts ([Releases](./releases.md))
- GET /api/releases/get?release=<id|#id|version|label> -> { data: ReleaseGetResponse }; the release, its tickets and history; 404 for an unknown id
- GET /api/releases/backlog[?project&query&...&limit&offset] -> { data: ReleaseBacklogResponse }; open tickets in no release, narrowed with the `/api/tasks/list` grammar
- POST /api/releases/create (ReleaseCreateRequest: { version, label?, notes?, planned_date?, released_at? }) | /api/releases/update (ReleaseUpdateRequest: { release, ...fields }; empty string clears) | /api/releases/release ({ release, at? }) | /api/releases/reopen ({ release }) -> { data: ReleaseMutationResponse }
- POST /api/releases/delete ({ release }) -> { data: ReleaseDeleteResponse }; takes the release off its tickets
- POST /api/releases/add | /api/releases/move | /api/releases/remove (ReleaseAssignmentRequest: { release, tasks?, query? }) -> { data: ReleaseAssignmentResponse }; add keeps a ticket's other releases, move replaces them
- POST /api/automation/run ({ rules?, query?, dry_run? }) -> { data: { dry_run, rules, checked, entries: [{ rule, task, actions, warnings?, error? }] } }; runs the `on.manual` actions of the named rules (all rules with one when empty) on each task their conditions match now; `query` uses the task-list search syntax; this is how automations run on a schedule (call it from cron)
- POST /api/automation/simulate ({ ticket_id, event, project?, rule? }) -> { data: { matched, rule_name, actions, task_before, task_after, warnings } }; `event` is one of created, updated, assigned, commented, sprint_changed, release_changed, job_started, job_completed, job_failed, job_cancelled, manual, unblocked (case-insensitive legacy aliases accepted); `rule` simulates that unsaved rule object instead of the saved ones; automation scope follows the ticket's project (`project` is ignored); `rule_name`/`task_after` are `null` when no rule matched; `warnings` lists preview parts only decided at run time
- POST /api/jobs/message (AgentJobMessageRequest) -> { data: AgentJobMessageResponse }
- GET  /api/events -> text/event-stream (see SSE Events)

More endpoints (request and response shapes are in [OpenAPI](../openapi.json)):

Tasks
- POST /api/tasks/status ({ id, status }) -> { data: TaskDTO, deferred?: JobOutcome }; same validation and agent-job handling as `lotar status`
- POST /api/tasks/comment ({ id, text }) -> { data: TaskDTO }
- POST /api/tasks/comment/update ({ id, index, text }) -> { data: TaskDTO }; `index` is the comment's position, starting at 0
- GET  /api/tasks/history?id=ID[&limit=N] -> { data: commits }; recent git commits that changed the task file
- GET  /api/tasks/commit_diff?id=ID&commit=SHA -> { data }; the task file's diff at that commit
- GET  /api/tasks/suggest?q=TEXT[&project=PREFIX][&limit=N] -> { data: suggestions }; tasks whose ID or title match, for pickers
- GET  /api/tasks/export -> the filtered task list as a file; takes the same filters as `/api/tasks/list`
- POST /api/tasks/references/link/add | /link/remove and /code/add | /code/remove -> add or remove a link or code reference
- GET  /api/tasks/stream -> alias of `/api/events`

Projects, people and config
- GET  /api/projects/list[?limit=N&offset=N] -> { data: projects }
- GET  /api/projects/stats?project=PREFIX -> { data: stats }
- POST /api/projects/create ({ name, prefix?, values? }) -> { data }; creates the project config
- GET  /api/whoami -> { data: string }; the identity used for `@me`
- GET  /api/config/inspect[?project=PREFIX] -> { data: ConfigInspectResult }; effective values with the source of each (there is no CLI `config inspect`; use `lotar config show --explain`)

Sprints
- GET  /api/sprints/list[?limit=N&offset=N] -> { data: sprints }
- GET  /api/sprints/summary?sprint=ID, /api/sprints/burndown?sprint=ID -> { data }; sprint analytics
- GET  /api/sprints/velocity[?limit=N&include_active=true&metric=...] -> { data }
- POST /api/sprints/order ({ task, sprint?, before?, after?, allow_closed? }) -> { data: TaskOrderResponse }; moves a task before or after another in the backlog or a sprint
- GET  /api/sprints/backlog/order -> { data: TaskOrderResponse }; the current backlog order

Activity and references
- GET  /api/activity/feed, /api/activity/series, /api/activity/authors, /api/activity/changed_tasks[?since&until&project...] -> { data }; activity for the Insights pages
- GET  /api/references/files?q=TEXT[&limit=N] -> { data }; repository file paths for reference pickers
- GET  /api/references/snippet?code=PATH#LINES[&context=N] -> { data }; the lines a code reference points to
- GET  /api/openapi.json -> this API's OpenAPI document

Notes
- Graph and repair: `/api/relationships/graph` takes `kinds=dependency,hierarchy` (omit for all, empty for none), `include_isolated=true|false`, `depth` (0-8 steps from the focus task) and `max_nodes` (never exceeded; `truncated` says when tasks were left out). To fix one problem, call `/api/relationships/repair` with `{ "issue_ids": ["<id>"], "apply": false }` to preview, then again with `apply: true` and the returned `revision`. The server checks again and refuses stale or unsafe fixes. Details: [relationship graph internals](../developers/api-quick-reference.md#relationship-graph-and-repair).
- CLI equivalents: `lotar task relationships graph --relationship-kind dependency --include-isolated false --filter 'status:todo tag:ui'` and `lotar task relationships repair --issue-id <id>` (then `--apply --expected-revision <token>`).
- List and export accept `archived=no|yes|all` (default `no`: archived tasks are left out). Archived tasks are still returned by id and carry `archived: true`; any request that would change one returns 400 unless it adds `?force_archived=true`.
- List and export accept `deletion=active|deleted|all` (default `active`). Blank or invalid values return 400. Delete/restore preserve `modified` and append lifecycle history; deleted tasks require restoration before ordinary mutations.
- People fields accept `@me`.
- /api/tasks/list accepts additional query keys beyond the documented ones: declared custom field names can be used directly (e.g., `?sprint=W35`). Values support CSV and fuzzy matching (case/sep-insensitive).
- Task mutations (add/update/status) validate `status`/`priority`/`type` strings against the target project's resolved configuration, so project-only enum values are accepted and out-of-set values return 400.
- `/api/tasks/add` accepts `status` for atomic initial status plus `custom_fields` (object) and `acceptance_criteria[]`; legacy `fields` key/value payloads still work with `custom_fields` winning per key.
- Patch null semantics are explicit: omitted = no-op, `null` = clear, value = set. Empty string clears clearable scalars (reporter/assignee/due_date/effort/description); empty array/object clears `tags`/`acceptance_criteria`/`relationships`/`custom_fields`/`sprints`. `title`/`status`/`priority`/`type` treat `null` as omitted and blank titles are rejected. List and map patches replace the whole value; an explicit `tags` clear does not re-apply configured `default_tags`.
- `custom_fields` replace-all patches keep keys already present on the task even when removed from project config, canonicalize configured keys to their configured spelling, and reject brand-new undeclared keys. `sprints` entries must be positive integers; invalid entries are rejected (REST and MCP).
- Validation errors return 400 with INVALID_ARGUMENT.
- Task/sprint domain failures share `{ error: { code, message } }`: invalid input, ambiguous/malformed IDs, and path/journal/cross-root safety refusals return `400 INVALID_ARGUMENT`; missing tasks/sprints/projects return `404 NOT_FOUND`; I/O, serialization, and index failures return `500 INTERNAL`. Unknown positive sprint IDs return 404 for update/delete/summary/burndown and task creation with sprint membership; zero/invalid IDs still return 400.
- `assignee=@me` that cannot be resolved fails closed with a 400 error rather than returning an unfiltered list.
- `/api/tasks/export` supports the exact `/api/tasks/list` query grammar, including the `due`/`due_within`/`recent`/`needs`/`has`/effort smart filters and `sort_by`/`order`: the export applies the complete filter set and the requested global order (default `modified` desc, canonical-ID ascending tiebreak); pagination params are ignored (no page slicing).
- `/api/tasks/list` and `/api/tasks/export` take the same parameters, the same as `lotar list` and MCP `task_list` (see [list](list.md#filters-in-the-query)). Invalid values return `400 INVALID_ARGUMENT`; they are never ignored or clamped. Leave a parameter out rather than sending it blank.

  | Parameter | Values |
  | --- | --- |
  | `sort_by` | `priority`, `status`, `effort`, `due-date`, `created`, `modified`, `assignee`, `reporter`, `title`, `type`, `project`, `id`, `tags`, `sprints`, `relevance` (best match for `q`), or `custom:<name>` / `field:<name>`. Priority and status sort in configured order. |
  | `order` | `asc` or `desc` (default `desc`) |
  | `limit` | 1-1000 |
  | `query` | Filters and search text in the web search-box syntax, e.g. `status:todo prio:high has:tag -tag:wip "login redirect"`; a leading minus excludes |
  | `q` | Plain search text only (see [search text](list.md#search-text)) |
  | `has` | `attachment`, `image`, `link`, `tag`; tasks must have every listed kind |
  | `needs` | `effort`, `due`: tasks missing that value |
  | `recent` | `Nd` or `Nw`: changed in the last N days or weeks |
  | `due`, `due_within` | Due bucket, or due from today through today + N days |
  | `effort_min`, `effort_max` | Inclusive bounds such as `2h`, `1d`, `3pt`; time and points never compare; tasks without effort are left out |
  | `assignee` | A person, `@me`, or `__none__` for unassigned |

- Enum filters (`status`, `priority`, `type`) validate against the explicit `project`'s resolved configuration when a project is requested, so project-only enum values are accepted; queries without `project` validate against the base config.

See also: [Identity & Users](./identity.md), [Task Model](./task-model.md), and [SSE Events](./sse.md).
