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
- Allow env override for all config values that are in all other chains
- Publish to docker hub, homebrew, npm?, ...
- VSCode Plugin
- IntelliJ Plugin
- Show source code snippets (e.g. around TODOs) in web ui

Bugs:
- project templates need to be reviewed and updated for the latest features

Chores:
- TODO: Replace parcel with vite
- Add shortcuts for arguments with -- (e.g. --project shoud have -p as well)
- Check if we're Windows compatible

---

# Implementation Roadmap — Smart Detection & Integrations (Aug 2025)

Legend: [ ] = TODO, [x] = Done, [~] = In Progress

## Phase 1 — Detection foundation
- [ ] Detector trait and registry (Signals with value, confidence, provenance)
- [ ] Git detector: repo root, current branch, remotes, user.name/email (repo-local > global)
- [ ] Project-config detector: package.json, Cargo.toml, pyproject.toml, go.mod, .csproj (project name/authors)
- [ ] System detector: OS username/hostname (fallback only)
- [ ] Merge policy: CLI > env > config (home > project > global) > Git > system (deterministic)
- [ ] Configured defaults override auto-detected values when enabled; smart detectors only apply if the corresponding feature toggle is on
- [ ] whoami --explain shows sources and confidence per field
- [ ] Caching and invalidation: keyed by repo+env; invalidate on config writes, branch change, .git/config mtime
- [ ] Feature toggles: every smart feature is disable-able and has a default value used when disabled (env/CLI override supported)

## Phase 2 — Ownership and auto-assign
- [ ] CODEOWNERS parser and path matcher (supports wildcards, directory rules)
- [ ] Auto-assign on first non-initial status: CODEOWNERS owner > Git identity > system (config-gated)
- [ ] Option to disable CODEOWNERS auto-assign; when off, use configured defaults and ignore detector results
- [ ] Tests: precedence, multiple owners, no matches
- [ ] Docs: using CODEOWNERS for ownership and auto-assign

## Phase 3 — Project context and monorepos
- [ ] Monorepo discovery: cargo workspaces, npm/yarn/pnpm workspaces, go workspaces
- [ ] Derive project id/name: prefer package/project names; fallback to repo name
- [ ] Derive labels/tags from paths (e.g., packages/foo => label: foo)
- [ ] Tests: nested workspaces, worktrees, submodules

## Phase 4 — Branch/PR awareness
- [ ] Branch conventions: feat/fix/chore => default task type/status/priority
- [ ] Config-aware mapping: statuses, types, and priorities are configurable; mapping only applies when branch tokens match configured values or a project-defined alias table
- [ ] Link tasks from branch/commit messages (e.g., LOTAR-123, #123)
- [ ] PR detection (opt-in with token): link open PR, import title/assignees/reviewers
- [ ] Flags/docs to opt-in/out per project

## Phase 5 — Source scan enrichment
- [ ] Rich TODO syntax (link-friendly):
	- Prefer stable ticket references and explicit metadata without relying on titles
	- Examples:
		- `// TODO [ticket=DEMO-123] Implement optimistic retries [assignee=@me] [due=2025-08-30] [tags=a,b]`
		- `# TODO DEMO-123: Implement optimistic retries [assignee=@me] [due=2025-08-30]`
	- Conventions:
		- Ticket key must be present as `DEMO-123` or `ticket=DEMO-123` to enable bi-directional linking
		- Optional attributes use `[key=value]` blocks; order-agnostic; safe across languages/comments
		- Title is free text and non-authoritative for linking
- [ ] Git blame to suggest reporter/owner when absent
- [ ] Scanner honors .gitignore; performance improvements on large repos
- [ ] Persistent bi-directional links between TODOs and tasks/tickets:
		- Store stable ticket keys (e.g., DEMO-123) in task metadata and maintain file+line anchors
		- Resilient anchors: compute content/AST hunks to keep links valid across refactors where possible
		- Optional: when VCS-host integration is enabled, post a backlink (permalink to code location) to the ticket; otherwise provide a CLI to copy/share the permalink
- [ ] CLI: show-related (list files/snippets for a task/ticket), show-snippet (render code context around a TODO)
- [ ] Tests with sample polyglot repos

## Phase 6 — Release follow-ups (optional)
- [ ] Include README and LICENSE in archives
- [ ] Universal macOS binary via lipo (optional)
- [ ] Post-upload verification job (download & verify checksum/signature)
- [ ] Package managers: Homebrew, Scoop, cargo-binstall, Docker image

## Acceptance gates per phase
- [ ] Build passes (lint/tests), new tests cover behavior and edge cases
- [ ] Docs/help updated; --explain outputs verified