# Agent CLI integration plan

## Goal
Enable LoTaR to orchestrate installed agent CLIs (Copilot CLI, Claude Code, Codex CLI, Gemini CLI) and capture reliable job status/progress without relying on the model to self-report.

## Non-goals (initial)
- Building new models or custom runtimes.

## Key concepts
- **Runner**: Adapter that launches a specific CLI and parses its output.
- **Job**: A persisted unit of work with lifecycle, logs, and ticket linkage.
- **Event stream**: Normalized progress events used by UI and API.

## Normalized event schema
- `job_id`
- `runner` (copilot|claude|codex|gemini)
- `event_type` (init|progress|tool_call|message|completed|failed)
- `timestamp`
- `payload` (text, usage, raw)

## Runner event mapping (observed)
### Claude Code
- Requires `--verbose` with `--output-format stream-json`.
- `type=system/subtype=init` → `init`
- `type=stream_event` + `content_block_delta` → `progress`
- `type=assistant` → `message`
- `type=result` + `subtype=success|error` → `completed` / `failed`

### Codex CLI
- Use `codex exec --json` (JSONL).
- `thread.started` → `init`
- `item.completed` (agent_message/reasoning) → `progress` / `message`
- `turn.completed` → `completed` (usage)

### Gemini CLI
- Use `--output-format stream-json`.
- Filter non‑JSON lines (IDE warnings).
- `type=init` → `init`
- `type=message` + `delta=true` → `progress`
- `type=result` + `status=success|error` → `completed` / `failed`

### Copilot CLI
- Use `-p --output-format stream-json`.
- `type=system/subtype=init` → `init`
- `type=message` + `delta=true` → `progress`
- `type=message` (no delta) → `message`
- `type=result` → `completed` / `failed`

## Job lifecycle
1) `queued`
2) `running` (first valid event or process start)
3) `completed` or `failed` (final event or non‑zero exit)
4) `cancelled` (user action)

## Proposed API surface
- `POST /api/jobs` → create job (runner, task/ticket, prompt, context)
- `GET /api/jobs` / `GET /api/jobs/get` → list jobs + fetch job status
- `GET /api/events?kinds=agent_job_*` → stream job events (SSE)
- `POST /api/jobs/cancel` → terminate

## Storage
- Job record (status, timestamps, runner, ticket id, exit code)
- Minimal context storage per ticket (e.g., `.context` with last N messages)
- Optional event log (bounded) for troubleshooting; avoid full raw logs by default
- **No job persistence in repo**: job state is ephemeral; web UI can cache last-seen state in browser storage only.

## UI
- Job panel for running/completed jobs
- Per‑job timeline of events
- Ticket view shows related job status

## Open questions
- Do we need a per‑job events endpoint beyond SSE?
- How should the UI present job timelines + ticket status at a glance?

## Recommended interaction model
### Primary integration goals
- Make LoTaR the **source of truth** for job state and ticket updates.
- Treat agent CLIs as **stateless workers**: spawn, stream events, record output.
- Never rely on agents to self-report status; use stream + process lifecycle.

### Supported operations (v1)
- **Run job**: create a job with runner + prompt + ticket context.
- **Stream status**: live events (SSE) or polling for incremental progress.
- **Cancel job**: terminate process and record outcome.
- **Summarize job**: persist final message + usage + references to ticket changes.

### Supported operations (v2)
- **Pause/Resume**: if runner supports session resumption.
- **Retry**: replay last prompt with updated context.
- **Fork**: clone job with different runner/model.
- **Review**: produce a review-only run (no edits) with stricter permissions.

## Ticket assignment constraint
- One ticket may be assigned to **one active agent job** at a time.
- Parallel jobs require separate tickets.

## Agent assignment model (assignee-as-profile)
- Agent profiles can be used directly as assignees (e.g., `@claude-review`, `@gemini-merge`, `@openai-max`).
- Assigning a ticket to `@<profile-name>` **sets the agent profile** and **automatically queues a job**.
- When a job starts, LoTaR can optionally set the ticket status (configurable via automation rules).
- When a job completes or needs review, LoTaR can optionally reassign back to a human (assignee or reporter).

## How to leverage LoTaR for best integration
### Context packaging
- Always pass **ticket ID + snapshot** (title, description, tags, status, refs).
- Provide **workspace pointers** (paths, linked files, recent changes).
- Include **job constraints** (allowed tools, max runtime, approval mode).

### Tooling and policy
- Use MCP tools for **ticket updates** and **comments** to keep audit trails.
- Gate tool usage by runner policy (`allowed-tools`, approvals).
- Record tool calls into job events for traceability.

### Tool call handling
- Treat tool calls as **messages** in the event stream.
- Surface tool output only when users expand status/details.

### Event normalization
- Convert runner-specific events into `init`, `progress`, `message`, `completed`, `failed`.
- Extract **progress text** from streamed deltas where available.
- Persist **final summary** + usage for cost reporting.

### Ticket integration points
- Auto-add a job reference to the ticket (and optional comment).
- Update ticket status on job lifecycle events if configured (e.g., `on_start` → `in_progress`, `on_success` → `needs_review`).
- Attach output artifacts (patch summaries, file lists, logs).

## Recommended surface area in LoTaR
### CLI
- `lotar agent run --runner <copilot|claude|codex|gemini> --ticket <id> --prompt <text>`
- `lotar agent status <job-id>`
- `lotar agent logs <job-id>`
- `lotar agent cancel <job-id>`
- `lotar agent list-running`
- `lotar agent check --status <value> [--assignee <handle>]` (candidate: git hook guard)

### REST
- `POST /api/jobs` (runner, prompt, ticket_id, context)
- `GET /api/jobs` / `GET /api/jobs/get` (status + summary)
- `GET /api/events?kinds=agent_job_*` (SSE)
- `POST /api/jobs/cancel`

### UI
- Job list panel with live status.
- Per-ticket “Run agent” action.
- Inline job timeline on ticket detail.
- Assignee selector supports agent profiles (e.g., `@claude-review`).

## UI workflow (parallel jobs + worktrees)
### Job dashboard
- Global list of active + recent jobs with filters (runner, ticket, status).
- Per-job status chips: `queued`, `running`, `waiting`, `completed`, `failed`, `cancelled`.
- Live progress stream with truncation + expandable raw logs.

### Worktree awareness
- Each job can optionally attach to a **dedicated worktree**.
- Display worktree path and branch name (if used).
- Surface merge/review readiness: `changes pending`, `ready for review`, `merge blocked`.

### Ticket-centric view
- Show related jobs on the ticket detail page.
- Highlight most recent job outcome and artifacts (summary, file list, patch).
- Allow “re-run with same config” and “fork to new runner”.

### Merge/review signals (derived)
- **Ready for review**: job completed + patch exists + no conflicts detected.
- **Merge blocked**: job completed but conflicts or failed checks.
- **No changes**: job completed with empty diff.

### Safety + approvals
- Show when a job is waiting on approval (if runner requires it).
- Provide “approve next step” and “cancel” actions.

### Control actions (pause, stop, message)
- **Stop**: always supported by terminating the process and marking the job `cancelled`.
- **Pause**: best‑effort; requires keeping the process alive and suspending I/O (or OS‑level pause). Not guaranteed across runners.
- **Send message**: only feasible if the runner supports interactive stdin or session continuation.

#### Feasibility by runner
- **Claude Code**: can be run in interactive mode and supports session IDs; LoTaR could keep stdin open for “send message” or resume via `--resume` to continue a thread.
- **Codex CLI**: supports `resume`/`fork` and `exec` with JSONL output; “send message while running” would require interactive mode. Otherwise LoTaR can emulate by spawning a new turn with `resume`.
- **Gemini CLI**: supports session resume; streaming mode is one‑shot. “Send message while running” likely requires interactive mode with stdin open.

#### Recommended UX
- Provide **Stop** universally.
- Provide **Send message** only when the runner session is interactive or resumable, otherwise offer **“Continue as new job”**.

## Implementation notes (stop/pause/message)
### Process model
- Spawn runners under a **job supervisor** that owns the process, stdin, stdout, stderr.
- Record the **session id** (when provided) on first event.
- Use a lightweight **wrapper process** (distinct name) so the OS process list can be used for `list-running` without persistence.

### Stop
- Send `SIGTERM` (graceful) then `SIGKILL` after timeout.
- Mark job `cancelled` and write a final event.

### Pause (best‑effort)
- Use OS‑level `SIGSTOP` / `SIGCONT` for supported platforms.
- If paused, stop reading stdout and mark job `waiting`.
- Resume by `SIGCONT` and continue stream parsing.

### Send message (interactive)
- Keep stdin open and write a new prompt line when user submits.
- Requires the runner to be started in **interactive mode**.
- Note: some runners emit **non‑JSON** output in interactive mode, so parsing may degrade.

### Send message (resume/fork)
- If interactive mode is not available or not streamable, emulate by **resuming** a session:
	- Claude Code: `claude --resume <session_id>` or `--session-id <uuid>`.
	- Codex CLI: `codex resume --last` or `codex fork --last` then `codex exec`.
	- Gemini CLI: `gemini --resume <session>`.
- Treat this as a **new job** linked to the previous one.

## Configuration model (proposed)
### Config sources (precedence)
1) CLI flags
2) Project config (repo-scoped)
3) User config (`~/.lotar`)
4) Environment variables

### Context storage
- Default: write a per-ticket `.context` file to support resume/continue.
- Config option: allow disabling `.context` storage entirely (stateless mode).
- Docs should recommend adding `.context` to `.gitignore` if teams do not want to share context files.

### Automation & status mapping (proposed)
Jobs can trigger optional, configurable ticket updates. Status labels are project-specific, so defaults must be overridable.

Defaults:
- Auto‑queue on assignment to `@<profile-name>` (default: enabled).
- Use the project’s existing status values; add a `NeedsReview` state to the built‑in defaults.
- If a configured status is missing in a project, **skip the update** (don’t fail the job).

Example (shape only):
```yaml
agent:
	assignment:
		use_assignee_profile: true
	automation:
		on_start:
			set_status: InProgress
		on_success:
			set_status: NeedsReview
			reassign_to: assignee_or_reporter
		on_failure:
			set_status: Blocked
		on_cancel:
			set_status: Todo
```

Notes:
- `reassign_to` could allow `assignee`, `reporter`, or an explicit user handle.
- Defaults should be sensible but optional; if omitted, no updates are made.

#### Automation triggers (proposed)
- Status change (e.g., `Todo` → `InProgress`, `NeedsReview` → `Done`).
- Tag/label added (e.g., `agent:auto`, `needs-refactor`).
- Priority threshold (e.g., High/Critical).
- Sprint events (added to sprint, sprint start/end).
- Due date proximity (e.g., due within N days).
- Comment command (e.g., `/agent run <profile>`).
- Branch/PR/CI activity (branch created, PR opened, CI failed).
- File path match (changes under specific directories).
- Scheduled runs (nightly triage/summarize).

#### Automation features & guardrails (proposed)
- Queueing + concurrency limits per profile.
- Retry policy with backoff (configurable max attempts).
- Auto‑handoff to reviewer on success/failure.
- Auto‑comment summary + artifacts on completion.
- Approvals required for destructive actions.
- Quotas (max jobs/day, per‑project caps).
- Dry‑run mode for rule validation.

### Agent profiles (user + project)
Define named agents to reuse in CLI/UI (similar to remotes). Profiles **extend defaults** rather than redefining required settings.

Example (shape only):
```yaml
agents:
	# Short form: all defaults for the runner
	myclaude: claude

	# Long form: only override/add values
	claude-max:
		runner: claude
		command: /opt/homebrew/bin/claude
		args:
			- "--model"
			- "sonnet"
		env:
			CLAUDE_API_KEY: "${CLAUDE_API_KEY}"
		tools:
			allowed: ["Bash", "Read", "Edit"]

	gemini-figma-mcp:
		runner: gemini
		args:
			- "--include-directories"
			- "../design-assets"
		mcp:
			allow: ["figma"]
```

### Defaults and required flags
- Required flags (e.g., JSON/stream output for parsing) are **always injected** by LoTaR.
- Agent config only **adds** extra args or overrides safe, non-essential options.
- Command discovery uses **PATH + runner defaults**, and the `command` field only extends/overrides.

### Project config use cases
- Shared team defaults (model, permissions, MCP servers, approval modes).
- Enforce guardrails for production repos.

### User config use cases
- Per-user tokens, local command paths, and personal aliases.

### Environment variables
- Used for secrets only; config refers to them via placeholders.

## Technical spec (implementation plan)
### Integration points in the codebase
- **Config**: extend `src/config/types.rs` + `src/config/operations.rs` + `src/config/resolution.rs` + `src/config/env_overrides.rs` to add agent profiles + context storage flag.
- **REST**: add job endpoints in `src/routes.rs` and DTOs in `src/api_types.rs`; update `docs/openapi.json` and `view/api/types.ts`.
- **SSE**: reuse `src/api_events.rs` to emit `agent_job_*` events; `src/web_server.rs` already forwards events.
- **CLI**: add `lotar agent` args under `src/cli/args/` and handlers under `src/cli/handlers/`.
- **Services**: add `agent_job_service` (job lifecycle + supervisor), `agent_runner` (CLI adapters), and `agent_context_service` (per-ticket `.context` storage).
- **UI**: add job dashboard + ticket job widgets under `view/pages/` + `view/components/` and reuse `view/composables/useSse.ts` for live status.

### Runner architecture
- Define a `Runner` trait with `spawn(job, context)` → `JobHandle` + `EventStream`.
- Implement runners for Copilot/Claude/Codex/Gemini with default args injected by LoTaR.
- Parse stdout as JSON/JSONL; Gemini requires non‑JSON line filtering.
- Collect `session_id` when available for resume flows.

### Job supervisor and storage
- Maintain an in‑memory job registry for active jobs (status, pid, timestamps).
- Avoid on-disk persistence; use wrapper-based process discovery plus UI local storage cache.
- Emit job events via `api_events::emit` for SSE consumers.

### Context storage (.context)
- Store per-ticket context as JSON or YAML under a safe subdir (e.g., `.tasks/.context/<ticket>.json`).
- Use a service pattern similar to `SyncReportService` for path validation + pruning.
- Allow `context.enabled=false` to run in stateless mode.

### Worktrees (optional, phase 2)
- If enabled, create a worktree per job and pass the path as runner working dir.
- Record worktree info in job metadata for UI display and cleanup.

### Refactor opportunities
- Centralize SSE event emission helpers alongside `api_events::emit_*` (mirror sync events).
- Reuse sync report path validation patterns for `.context` handling.
- Mirror the `SyncService` run-id + progress event pattern for job lifecycle events.

### Alternatives considered
- **In‑process jobs** vs **daemon runner**: in‑process is simpler but loses jobs when server stops.
- **In‑memory only** vs **file‑backed job index**: memory avoids disk writes but loses history.
- **Interactive stdin** vs **resume/fork**: interactive is richer but harder to parse; resume/fork is safer.

## Next steps
- Decide whether a per-job events endpoint is needed beyond SSE.
- Expand the UI with job detail/timeline and ticket-level job status.

## Implementation task list (tracking)
### Completed
- [x] Define job DTOs and API routes (POST/GET/stream/cancel)
	- Notes: update `src/api_types.rs`, `docs/openapi.json`, `view/api/types.ts`, and `src/routes.rs`; streaming uses `/api/events` SSE.
- [x] Create job supervisor + registry (in-memory)
	- Notes: include PID, timestamps, status, session id, and ticket id.
- [x] Add SSE events for agent jobs (`agent_job_*`)
	- Notes: emit via `api_events::emit` and document event payloads.
- [x] Implement `.context` storage service
	- Notes: store in `.tasks/.context/<ticket>.json`, prune to last N messages, opt-out flag.
- [x] Add config schema for agent profiles + context toggle
	- Notes: extend config types, resolution, env overrides, and validation.
- [x] Add agent automation defaults (auto-queue + status/reassign)
	- Notes: configurable per project; skip missing statuses.
- [x] Implement Codex runner adapter
	- Notes: `codex exec --json`, parse JSONL, handle exit codes.
- [x] Add CLI commands (`lotar agent run/status/logs/cancel/list-running/check`)
	- Notes: wire to job service and surface job ids.
- [x] Add minimal UI job panel
	- Notes: use `useSse` to show active jobs and per-ticket status.
- [x] Add tests
	- Notes: unit tests for event parsing, context storage, and REST handlers.
- [x] Document `.context` + gitignore guidance
	- Notes: update docs/help if needed.
- [x] Implement wrapper process discovery for `list-running`
	- Notes: wrapper forwards signals and makes OS-level process scans possible.

### Follow-ups
- [ ] Decide whether to add a per-job events endpoint (`/api/jobs/events`) or keep SSE-only and update docs accordingly.
- [ ] Add job detail/timeline UI plus ticket-level job status widgets.
- [ ] Implement web UI local storage cache for last-seen jobs (non-authoritative).
