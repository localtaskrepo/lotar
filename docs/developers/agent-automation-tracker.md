# Agent Automation & Instrumentation — Feature Tracker

Status overview for the automation and agent orchestration feature.

## Implementation Status

All planned features are implemented. See `docs/help/agent.md` and `docs/help/automation.md` for user-facing docs, `docs/developers/agent-integrations-plan.md` for design details.

### Core

- [x] Agent runners — Copilot (default), Claude, Codex, Gemini with stream-JSON parsers
- [x] Agent profiles — Named profiles in config (short-form or detailed with args/env/tools/mcp/instructions)
- [x] Job lifecycle — Queued → Running → Completed/Failed/Cancelled, in-memory registry, log persistence
- [x] File-backed agent queue — Jobs enqueue to OS cache dir, background `lotar agent worker` dequeues
- [x] Orchestrator modes — Server (in-memory), Standalone (file queue), Worker (background dequeuer)
- [x] Git worktree isolation — Per-ticket worktrees with branch prefix, parallel limits, cleanup config
- [x] Agent context — Per-ticket `.context` files for resume/continue
- [x] Project templates — `agent-pipeline` and `agent-reviewed` (default to Copilot)
- [x] `command` runner — Arbitrary commands as agent jobs with full lifecycle
- [x] Send message to running agents — Runner-specific stdin formatting
- [x] Agent skill template — Built-in instructions for agents using lotar

### Automation Engine

- [x] Rules engine — Field matching, change detection, regex, `all`/`any`/`not`, list operators, `exists`
- [x] Event-based `on` hooks — `created`, `updated`, `assigned`, `commented`, `sprint_changed`, job lifecycle
- [x] Template variable expansion — `${{ticket.*}}`, `${{previous.*}}`, `${{agent.*}}`, `${{comment.*}}`, `${{project.*}}`
- [x] Actions — `set` fields, `add`/`remove` tags, `run` shell commands, `comment`, sprint actions, relationship actions
- [x] Async `run` actions — Background spawn by default, `wait: true` for blocking
- [x] Assignment strategies — `@round_robin`, `@random`, `@least_busy`
- [x] Max-iterations safety net — Per-ticket job count limit (default 10)
- [x] Cooldown / debounce — Per-rule `cooldown` duration
- [x] Date condition operators — `before`, `within`, `older_than`
- [x] Sprint conditions & actions — `sprint: { equals: "@active" }`, `add: { sprint: "@active" }`
- [x] Multi-phase pipelines — Job completion chaining via automation rules
- [x] LOTAR_* env vars — Injected into all spawned processes (agents, commands, `run` actions)

### Interfaces

- [x] CLI — `agent run`, `status`, `logs`, `cancel`, `check`, `list-running`, `list-jobs`, `worktree list/cleanup`, `worker`, `queue`
- [x] CLI — `automation simulate` / `dry-run`
- [x] REST API — Job create/list/get/logs/cancel/cancel-all/message (`src/routes/jobs.rs`), queue stats, automation inspect/set/simulate (`src/routes/automation.rs`)
- [x] SSE events — `agent_job_started`, `agent_job_progress`, `agent_job_message`, `agent_job_input`, `agent_job_completed`, `agent_job_failed`, `agent_job_cancelled` (payloads in `docs/developers/sse.md`). The push-only kinds `agent_job_raw`, `agent_job_init`, and `agent_job_result` consume sequence numbers and appear in the REST log (`GET /api/jobs/logs`) but are never streamed over SSE.
- [x] MCP tools — Job creation and management (`agent_run`, `agent_status`, `agent_cancel`, `agent_list_jobs`, `agent_send_message`)
- [x] Web UI — Agent Jobs page with queue stats, job creation, profile picker, live log viewer, cancel/remove
- [x] Fix agent-in-members bug — `@`-prefixed agent profiles filtered from member lists

## Bugs

- [x] **Agent profile names added to member list** — Fixed in `missing_members_for_task` and Vue `preloadPeople`.
- [x] **CLI `add` missing automation trigger** — `AddHandler` bypassed `AutomationService`. Fixed: automation now fires after `storage.add()`.
- [x] **Home config interference in tests** — `~/.lotar` overriding workspace config. Fixed: `LOTAR_IGNORE_HOME_CONFIG=1` in smoke env.
- [x] **`lotar init --project` broken** — Fixed: project init writes the project config under the tasks root and `ensure_global_config_has_default` always sets `default.project` in the global config (`src/cli/handlers/config/init/mod.rs`).
- [x] **CLI single-field commands skip automation** — Fixed: `lotar priority`, `lotar status`, `lotar assignee`, `lotar due`, and `lotar effort` route through `TaskService::update` and fire automation; covered by `tests/cli_single_field_automation_test.rs`.

## Known Gotchas

- **Config format**: Custom statuses must use `issue: states: [...]`, not `statuses: [...]` at the top level. Same for `issue: priorities:` and `issue: types:`. See `src/config/normalization.rs`.
- **`MatchMode::OnChange`**: For `updated` events, each field in `when` conditions must have actually changed in the update. Single-field CLI commands only include that one field in the changeset. For `created` events, all fields count as "new".
- **`default: project:`**: The config's default project is only used in certain code paths. `detect_project_name()` returns a value first (from env → project files → git repo → directory name → "default"). Use `-p <project>` to force prefix.
- **Wrapper process discovery is system-wide**: `running_job_for_ticket()` scans all system processes for `lotar-agent-wrapper` by design (wrapper-based discovery lets `list-running` work without persistence; see `src/cli/handlers/agent.rs`). Ticket matching is alias-aware (DEV-56). Concurrent runs that spawn wrappers can observe each other's jobs.

---

## Test Coverage

### Rust Unit/Integration Tests

Counts are static source counts (`#[test]` / `#[tokio::test]`), not run results; git-gated tests are included in the count.

- 1498 test functions across `src/` and `tests/`
- 35 agent automation integration tests (`tests/agent_automation_integration_test.rs`): lifecycle, failure, cancel, multi-phase chaining
- Related suites: `tests/agent_safety_test.rs`, `tests/cli_agent_check_test.rs`, `tests/cli_single_field_automation_test.rs`, `tests/dev59_automation_composition_test.rs`, `tests/dev102_automation_noop_test.rs`
- `on.commented` is implemented and covered: `AutomationEvent::Commented` exists in `src/services/automation_service.rs` and is dispatched by `TaskService::add_comment_with_context` (`src/services/task_service.rs`); automation's own comment actions use `CommentContext::automation()` and never re-dispatch it (recursion guard). Tested in `tests/dev60_shared_mutation_test.rs` and the CLI comment tests.

### Smoke Tests (278 tests across 71 files; static `it(` count)

#### CLI Automation (`cli.automation.smoke.spec.ts`) — 16 tests
| Test | Status |
|------|--------|
| `on.created` event fires and sets fields | ✅ |
| `on.updated` event fires when status changes | ✅ |
| `on.assigned` event fires | ✅ |
| `comment` action adds comments with template expansion | ✅ |
| Tag `add`/`remove` actions | ✅ |
| `all`/`any` logical combinators in conditions | ✅ |
| Regex conditions (`matches`) | ✅ |
| `changes` conditions with `from`/`to` (positive + negative) | ✅ |
| `max_iterations` safety net tags ticket | ✅ |
| `automation simulate` match and no-match | ✅ |
| `run` action executes shell commands | ✅ |
| Template expansion `${{ticket.*}}` in comments | ✅ |
| `${{previous.*}}` template variables in update events | ✅ |
| Async `run` action does not block task update | ✅ |

#### CLI Config & Agent Profiles (`cli.config-agent.smoke.spec.ts`) — 4 tests
| Test | Status |
|------|--------|
| `config show` includes worktree cleanup settings | ✅ |
| `config show` includes agent profiles | ✅ |
| Automation loaded from project-specific path | ✅ |
| Project-specific automation rules fire correctly | ✅ |

#### CLI Relationships (`cli.relationships.smoke.spec.ts`) — 5 tests
| Test | Status |
|------|--------|
| `task relationships` shows stored relationships | ✅ |
| JSON output includes relationship data | ✅ |
| Automation adds `depends_on` relationship | ✅ |
| Automation removes relationship on status change | ✅ |
| Reference actions add link references | ✅ |

#### CLI Agent Commands (`cli.agent-commands.smoke.spec.ts`) — 3 tests
| Test | Status |
|------|--------|
| `agent queue` shows no pending entries | ✅ |
| `agent worktree list` shows no worktrees | ✅ |
| `agent worktree cleanup` runs without error | ✅ |

#### REST API Automation (`api.automation.smoke.spec.ts`) — 11 tests
| Test | Status |
|------|--------|
| `GET /api/automation/show` returns current rules | ✅ |
| `GET /api/automation/show?project=X` returns project-scoped rules | ✅ |
| `POST /api/automation/set` saves valid YAML | ✅ |
| `POST /api/automation/set` rejects invalid YAML and cooldowns without rewriting rules | ✅ |
| `POST /api/automation/simulate` returns matching rule | ✅ |
| `POST /api/automation/simulate` with no match returns empty | ✅ |
| Round-trip: set rules → create task → verify automation fired | ✅ |
| Cooldown prevents re-firing within window (server mode) | ✅ |
| Monitor counterfactual: wait-then-read wedges a chatty stderr child, concurrent drain completes | ✅ |
| Server async run drains a chatty stderr child without wedging the monitor | ✅ |
| Server async run reports descendant-held stderr within the bounded grace | ✅ |

#### MCP Agent Tools (`mcp.agent-tools.smoke.spec.ts`) — 9 tests
| Test | Status |
|------|--------|
| `agent_list_jobs` returns empty list | ✅ |
| `agent_status` error for missing id | ✅ |
| `agent_status` tool failure for non-existent job | ✅ |
| `agent_cancel` tool failure for non-existent job | ✅ |
| `agent_run` error for missing required params | ✅ |
| `agent_send_message` error for missing params | ✅ |
| Agent tools appear in tools/list | ✅ |
| `agent_run` with command runner creates and starts a job | ✅ |
| `agent_status` returns details for an existing job | ✅ |

#### MCP High-Value (`mcp.high-value.smoke.spec.ts`) — includes agent tests
| Test | Status |
|------|--------|
| Sprint list/create/update + analytics via MCP | ✅ |
| `task_bulk_update` patches multiple tasks | ✅ |

#### API Agent Jobs (`api.agent-jobs.smoke.spec.ts`)
| Test | Status |
|------|--------|
| Mock agent job creation → completion → log capture | ✅ |
| Job status transitions via REST API | ✅ |

**Known flake source**: `api.agent-jobs.smoke.spec.ts` spawns `lotar-agent-wrapper` processes visible system-wide. When running concurrently, `lotar status` in CLI automation tests scans all processes and may find these wrappers, failing with an "active agent job" error. This is inherent to wrapper-based discovery in `running_job_for_ticket` (system-wide process scan, alias-aware ticket matching per DEV-56), not a persistence bug.

#### WebUI Agent Jobs (`ui.agent-jobs.smoke.spec.ts`) — 6 tests
| Test | Status |
|------|--------|
| Renders page heading and queue stats | ✅ |
| Shows completed job in job list | ✅ |
| Completed filter removes non-matching jobs and keeps matching ones | ✅ |
| Streams log entries live while a job is running | ✅ |
| Groups multiple jobs for the same ticket into one card (retry: 2) | ✅ |
| Stops queued and running jobs only after accessible confirmation | ✅ |

#### WebUI Automation (`ui.automation.smoke.spec.ts`) — 6 tests
| Test | Status |
|------|--------|
| Cannot save old rules while a new project load is pending or failed | ✅ |
| Renders rules from automation.yml | ✅ |
| Simulator tab renders and shows empty state for no match | ✅ |
| Rules and Simulator tabs toggle content | ✅ |
| Creates a rule through the guided dialog | ✅ |
| Guided dialog shows project-aware suggestions | ✅ |

### Remaining Test Gaps

The following test scenarios are NOT yet covered. Most require complex setup (sprints, date conditions) or worktree infrastructure (agent worktree with completed job). `on.commented` is no longer a gap: it is implemented and covered (see Rust tests above).

#### Complex setup (deferred)

| Test | Category | Notes |
|------|----------|-------|
| Assignment strategy `@round_robin` | CLI automation | Need members config + multiple task creates; verify rotation |
| Date condition `older_than` | CLI automation | Need task with backdated `created` field |
| Sprint condition `equals: "@active"` | CLI automation | Need sprint create + task sprint membership + automation rule |
| `agent_send_message` to running job | MCP | Need slow `command` runner to keep job alive during message send |
| Agent worktree list/cleanup with real worktrees | CLI agent | Need git repo + completed worktree job to have something to list/clean |
| Project selector scopes rules in UI | WebUI automation | Need multi-project setup with separate automation files |

#### WebUI Config automation editor (not tested)

| Test | Notes |
|------|-------|
| Automation YAML editor shows current rules | Navigate to config page, verify automation YAML displays |
| Save valid YAML persists rules | Edit YAML, save, reload → verify persistence |
| Save invalid YAML shows validation error | Enter malformed YAML, save → error displayed |

#### REST API edge cases (not tested)

| Test | Notes |
|------|-------|
| `POST /api/automation/set` returns warnings for unknown fields | Valid YAML with typos returns warnings |

## Key Files

| Area | Files |
|------|-------|
| Runner adapters | `src/services/agent_runner.rs` |
| Job lifecycle | `src/services/agent_job_service.rs` |
| Job event logs | `src/services/agent_log_service.rs` |
| File-backed queue | `src/services/agent_queue_service.rs` |
| Automation engine | `src/services/automation_service.rs`, `src/services/automation_matching.rs` |
| Automation types | `src/automation/types.rs`, `src/automation/persistence.rs`, `src/automation/template.rs` |
| Config types | `src/config/types.rs` (agent profiles, automation config, worktree config) |
| Config normalization | `src/config/normalization.rs` (parses `issue.states/types/priorities`) |
| CLI args/handlers | `src/cli/args/agent.rs`, `src/cli/handlers/agent.rs`, `src/cli/handlers/mod.rs` |
| REST routes | `src/routes/jobs.rs` (job endpoints), `src/routes/automation.rs` (automation endpoints) |
| API DTOs | `src/api_types.rs` |
| MCP handlers | `src/mcp/server/handlers/agent.rs` |
| UI page | `view/pages/AgentJobs.vue` |
| UI types | `view/api/types.ts` |
| Templates | `src/config/templates/agent-pipeline.yml`, `src/config/templates/agent-reviewed.yml` |
| Integration tests | `tests/agent_automation_integration_test.rs`, `tests/agent_safety_test.rs`, `tests/cli_agent_check_test.rs`, `tests/cli_single_field_automation_test.rs`, `tests/dev59_automation_composition_test.rs`, `tests/dev102_automation_noop_test.rs` |
| Smoke: CLI automation | `smoke/tests/cli.automation.smoke.spec.ts` |
| Smoke: CLI agent commands | `smoke/tests/cli.agent-commands.smoke.spec.ts` |
| Smoke: CLI config/agent | `smoke/tests/cli.config-agent.smoke.spec.ts` |
| Smoke: CLI relationships | `smoke/tests/cli.relationships.smoke.spec.ts` |
| Smoke: MCP agent tools | `smoke/tests/mcp.agent-tools.smoke.spec.ts` |
| Smoke: API agent jobs | `smoke/tests/api.agent-jobs.smoke.spec.ts` |
| Smoke: API automation | `smoke/tests/api.automation.smoke.spec.ts` |
| Smoke: UI agent jobs | `smoke/tests/ui.agent-jobs.smoke.spec.ts` |
| Smoke: UI automation | `smoke/tests/ui.automation.smoke.spec.ts` |
| User docs | `docs/help/agent.md`, `docs/help/automation.md` |
| Design doc | `docs/developers/agent-integrations-plan.md` |

## Known Bugs

| Bug | Severity | Details |
|-----|----------|---------|
| Cross-test visibility of wrapper processes | Low | `running_job_for_ticket()` scans all system processes for `lotar-agent-wrapper` by design (wrapper-based discovery, `src/cli/handlers/agent.rs`). Concurrent runs that spawn wrappers can observe each other's jobs; ticket matching is alias-aware (DEV-56). |

## Fixed (formerly tracked here)

| Former bug | Resolution |
|-----|---------|
| `on.commented` not implemented | `AutomationEvent::Commented` exists and is dispatched by `TaskService::add_comment_with_context`; automation comment actions cannot recurse (`CommentContext::automation()`). |
| `TaskService::create` returned pre-automation DTO | `create()` re-fetches the task after `apply_task_update` and returns the post-automation state (`src/services/task_service.rs`). |
| `lotar init --project` mis-placed configs / unset default project | Project config written under the tasks root; `ensure_global_config_has_default` always sets `default.project`. |
| CLI single-field commands bypassed automation | `priority`/`status`/`assignee`/`due`/`effort` route through `TaskService::update`; covered by `tests/cli_single_field_automation_test.rs`. |
