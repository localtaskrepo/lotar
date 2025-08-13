Features:
- Due Date
- Assignee, Reporter
- Source Code Scanning
- Task relationship queries and graphs
- Task contexts (attachments?)
- Comments
- Audit Log
- Set custom properties from environment variable (e.g. tests = .lotar -> myid@company.com)
- Shell completion
- Git Hooks for scanner (once implemented)
- Project members property (for auto fill in web interface)

Bugs:
- project templates need to be reviewed and updated for the latest features

Chores:
- TODO: Replace parcel with vite
- Add shortcuts for arguments with -- (e.g. --project shoud have -p as well)

---

# Prioritized Implementation Plan (Aug 2025)

## Phase 1 — UX clarity and low-risk enhancements
- Config precedence unification (DONE)
	- Enforced order: CLI > env > home > project > global > defaults across CLI/REST/MCP.
	- Updated docs in `docs/help/config.md`, `docs/help/main.md`, and `docs/help/whoami.md`.
	- Tests green; logs clean in test runs.
- CLI: whoami command (DONE)
	- Prints resolved current user and source chain (config → git → system); supports --format=json.
- Explain flags (IN PROGRESS)
	- Config show supports --explain with value sources; extend to more commands as needed.
- Dry-run support (DONE for add/status/edit/delete)
	- --dry-run previews changes without side-effects; now returns structured JSON when --format=json.
- Docs: precedence reference (DONE) and dedicated page (TODO)
	- Basic precedence notes added to command docs; create a standalone “Resolution & Precedence” page with diagram and link from add/status/config.

## Phase 2 — Config flexibility and ergonomics
- Project- and home-level toggles
	- Add auto_set_reporter and auto_assign_on_status to ProjectConfig; merge order aligned with unified precedence:
		CLI > env > home > project > global; default true when unspecified.
- @me alias
	- Allow in config values and CLI inputs for reporter/assignee/default_reporter.
	- Resolve at runtime to current user.
- Auto-assign first-change semantics
	- If assignee is unset and status changes from default/first status to non-default/first status for the first time:
		set assignee = resolved reporter (from config or auto-detected) if set if present; else fall back to resolved current user.
	- Gate by auto_assign_on_status.

## Phase 3 — API/MCP and observability
- OpenAPI + docs refresh (IN PROGRESS)
	- Update descriptions/examples for new flags, @me, project-level toggles, first-change rule, and JSON dry-run previews.
- SSE actor attribution
	- Include triggered_by on task/config events; document precedence and sources.

## Phase 4 — Performance and architecture
- Caching
	- Cache resolved config and identity in-process; invalidate on file change in serve mode.
- Lazy probing
	- Only read .git/config when needed (no default_reporter and no system username).

## Phase 5 — Smart detection and integrations
- Additional sources for project/user
	- Project: package.json, Cargo.toml, pyproject.toml, go.mod, .csproj, etc.
	- Identity: optional providers (e.g., gh config, project developer info).

## Testing & diagnostics (cross-cutting)
- Precedence matrix tests for default_reporter/default_assignee (global/home/project/env).
- Expand tests to validate new home-over-project precedence and env overrides.
- Golden tests for --explain output.
- Concurrency/env mutex coverage for new envs.
- Property tests for @me expansion and identity normalization.

## Acceptance gates per phase
- Build passes, tests green, docs updated, and CLIs have help entries.