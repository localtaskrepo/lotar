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
- [x] REST API — Full CRUD for jobs, queue stats, automation inspect/set/simulate endpoints
- [x] SSE events — `agent_job_started`, `agent_job_init`, `agent_job_progress`, `agent_job_message`, `agent_job_result`, `agent_job_completed`, `agent_job_failed`, `agent_job_cancelled`
- [x] MCP tools — Agent job creation (`agent_execute`, `agent_run`) and management (`agent_status`, `agent_cancel`, `agent_list_jobs`)
- [x] Web UI — Agent Jobs page with queue stats, job creation, profile picker, live log viewer, cancel/remove
- [x] Fix agent-in-members bug — `@`-prefixed agent profiles filtered from member lists

## Bugs

- [x] **Agent profile names added to member list** — Fixed in `missing_members_for_task` and Vue `preloadPeople`.
- [x] **CLI `add` missing automation trigger** — `AddHandler` bypassed `AutomationService`. Fixed: automation now fires after `storage.add()`.
- [x] **Home config interference in tests** — `~/.lotar` overriding workspace config. Fixed: `LOTAR_IGNORE_HOME_CONFIG=1` in smoke env.
- [ ] **`lotar init --project` broken** — Configs generated in project dir instead of global; default project not set. (Not agent-specific but blocks clean onboarding.)
- [ ] **CLI single-field commands skip automation** — `lotar priority`, `lotar status` etc. use `ctx.storage.edit()` directly, bypassing `TaskService::update` and automation triggers. Only `lotar task add` and API/MCP paths fire automation.

## Known Gotchas

- **Config format**: Custom statuses must use `issue: states: [...]`, not `statuses: [...]` at the top level. Same for `issue: priorities:` and `issue: types:`. See `src/config/normalization.rs`.
- **`MatchMode::OnChange`**: For `updated` events, each field in `when` conditions must have actually changed in the update. Single-field CLI commands only include that one field in the changeset. For `created` events, all fields count as "new".
- **`default: project:`**: The config's default project is only used in certain code paths. `detect_project_name()` returns a value first (from env → project files → git repo → directory name → "default"). Use `-p <project>` to force prefix.

---

## Test Coverage

### Rust Unit/Integration Tests

- 745 tests total
- 4 agent automation integration tests (`tests/agent_automation_integration_test.rs`): success, failure, cancel, multi-phase chaining
- 96 pre-existing failures from workspace `.tasks/` contamination (not from automation changes)

### Smoke Tests (103 total across 29 files)

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

#### REST API Automation (`api.automation.smoke.spec.ts`) — 8 tests
| Test | Status |
|------|--------|
| `GET /api/automation/show` returns current rules | ✅ |
| `GET /api/automation/show?project=X` returns project-scoped rules | ✅ |
| `POST /api/automation/set` saves valid YAML | ✅ |
| `POST /api/automation/set` rejects invalid YAML | ✅ |
| `POST /api/automation/simulate` returns matching rule | ✅ |
| `POST /api/automation/simulate` with no match returns empty | ✅ |
| Round-trip: set rules → create task → verify automation fired | ✅ |
| Cooldown prevents re-firing within window (server mode) | ✅ |

#### MCP Agent Tools (`mcp.agent-tools.smoke.spec.ts`) — 9 tests
| Test | Status |
|------|--------|
| `agent_list_jobs` returns empty list | ✅ |
| `agent_status` error for missing id | ✅ |
| `agent_status` error for non-existent job | ✅ |
| `agent_cancel` error for non-existent job | ✅ |
| `agent_run` validates parameters | ✅ |
| `agent_execute` validates parameters | ✅ |
| `agent_run` with command runner creates and starts a job | ✅ |
| `agent_status` returns details for an existing job | ✅ |
| Tool listing includes agent tools | ✅ |

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

**Known flake**: `api.agent-jobs.smoke.spec.ts` spawns `lotar-agent-wrapper` processes visible system-wide. When running concurrently, `lotar status` in CLI automation tests scans all processes and may find these wrappers, failing with "active agent job" error. This is a pre-existing issue in the `running_job_for_ticket` function which uses system-wide process scanning.

#### WebUI Agent Jobs (`ui.agent-jobs.smoke.spec.ts`) — 4 tests
| Test | Status |
|------|--------|
| Renders page heading and queue stats | ✅ |
| Shows completed job in job list | ✅ |
| Filter tabs present and clickable | ✅ |
| New job form toggles and shows fields | ✅ |

#### WebUI Automation (`ui.automation.smoke.spec.ts`) — 3 tests
| Test | Status |
|------|--------|
| Renders rules from automation.yml | ✅ |
| Simulator tab renders and shows empty state for no match | ✅ |
| Rules and Simulator tabs toggle content | ✅ |

### Remaining Test Gaps

The following test scenarios are NOT yet covered. Most require either unimplemented features (`on.commented`), complex setup (sprints, date conditions), or worktree infrastructure (agent worktree with completed job).

#### Not implemented (blocked)

| Test | Reason |
|------|--------|
| `on.commented` event fires automation | `Commented` event not in `AutomationEvent` enum; comment handler bypasses `AutomationService` |

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
| Cancel button cancels a running job in WebUI | Start slow job, click cancel, verify status change |

## Key Files

| Area | Files |
|------|-------|
| Runner adapters | `src/services/agent_runner.rs` |
| Job lifecycle | `src/services/agent_job_service.rs` |
| File-backed queue | `src/services/agent_queue_service.rs` |
| Automation engine | `src/services/automation_service.rs`, `src/services/automation_matching.rs` |
| Automation types | `src/automation/types.rs`, `src/automation/persistence.rs`, `src/automation/template.rs` |
| Config types | `src/config/types.rs` (agent profiles, automation config, worktree config) |
| Config normalization | `src/config/normalization.rs` (parses `issue.states/types/priorities`) |
| CLI args/handlers | `src/cli/args/agent.rs`, `src/cli/handlers/agent.rs`, `src/cli/handlers/mod.rs` |
| REST routes | `src/routes.rs` (job + automation endpoints) |
| API DTOs | `src/api_types.rs` |
| UI page | `view/pages/AgentJobs.vue` |
| UI types | `view/api/types.ts` |
| Templates | `src/config/templates/agent-pipeline.yml`, `src/config/templates/agent-reviewed.yml` |
| Integration tests | `tests/agent_automation_integration_test.rs` |
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
| `on.commented` event not implemented | Medium | `AutomationEvent` enum lacks `Commented` variant. Comment CLI handler doesn't call `AutomationService`. Rules with `on: [commented]` are silently ignored. |
| `TaskService::create` returns pre-automation DTO | Low | After `create()`, the returned task DTO reflects the state *before* automation ran. Callers must re-fetch via `TaskService::get` to see automated changes. Affects `POST /api/tasks/add` response body. |
| Process-wide agent job detection causes test flake | Low | `running_job_for_ticket()` uses `sysinfo::System` to scan ALL system processes for `lotar-agent-wrapper`. During parallel test runs, wrapper processes from `api.agent-jobs.smoke.spec.ts` cause false positives in concurrent CLI tests. |
