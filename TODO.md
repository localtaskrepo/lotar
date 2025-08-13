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
- Git Hooks

Bugs:
- We're using default_project instead of default_prefix in the global config.
- project templates need to be reviewed and updated for the latest features

Chores:
- TODO: Replace parcel with vite
- Add shortcuts for arguments with -- (e.g. --project shoud have -p as well)

---

# Prioritized Implementation Plan (Aug 2025)

## Phase 1 — UX clarity and low-risk enhancements
- CLI: whoami command (DONE)
	- Prints resolved current user and source chain (config → git → system); supports --format=json.
- Explain flags (IN PROGRESS)
	- Config show supports --explain with value sources; extend to more commands as needed.
- Dry-run support (DONE for add/status/edit/delete)
	- --dry-run previews changes without side-effects; now returns structured JSON when --format=json.
- Docs: precedence diagram (TODO)
	- Create “Resolution & Precedence” page; link from add/status/config docs.

## Phase 2 — Config flexibility and ergonomics
- Project- and home-level toggles
	- Add auto_set_reporter and auto_assign_on_status to ProjectConfig; merge order:
		project > home > global; default true when unspecified.
- @me alias
	- Allow in config values and CLI inputs for reporter/assignee/default_reporter.
	- Resolve at runtime to current user.
- Auto-assign first-change semantics
	- If assignee is unset and status changes from TODO to non-TODO for the first time:
		set assignee = reporter if present; else fall back to resolved current user.
	- Gate by auto_assign_on_status.

## Phase 3 — API/MCP and observability
- OpenAPI + docs refresh (IN PROGRESS)
	- Update descriptions/examples for new flags, @me, project-level toggles, first-change rule, and JSON dry-run previews.
- MCP tools
	- Add whoami and config_show; optional config_suggestions for enums.
- SSE actor attribution
	- Include triggered_by on task/config events; document precedence and sources.

## Phase 4 — Performance and architecture
- Caching
	- Cache resolved config and identity in-process; invalidate on file change in serve mode.
- Lazy probing
	- Only read .git/config when needed (no default_reporter and no system username).
- Feature gates
	- Ensure CLI/REST/MCP can be built separately to keep binaries small.

## Phase 5 — Smart detection and integrations
- Additional sources for project/user
	- Project: package.json, Cargo.toml, pyproject.toml, go.mod, .csproj, etc.
	- Identity: optional providers (e.g., gh config) behind feature gates.
- Git hooks helpers
	- Samples for commit-msg or post-merge to update tasks.

## Testing & diagnostics (cross-cutting)
- Precedence matrix tests for default_reporter/default_assignee (global/home/project/env).
- Golden tests for --explain output.
- Concurrency/env mutex coverage for new envs.
- Property tests for @me expansion and identity normalization.

## Acceptance gates per phase
- Build passes, tests green, docs updated, and CLIs have help entries.

## Nice-to-haves (backlog)
- Rule-based auto-assignment by tags/category/status.
- Profiles/contexts for switching home configs.