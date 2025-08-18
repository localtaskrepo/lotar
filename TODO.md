Features:
- Due Date
- Task relationship queries and graphs
- Comments
- Audit Log
- Shell completion with install command
- Git Hooks for scanner (once implemented)
- Project members property (for auto fill in web interface)
- Allow env override for all config values that are in all other chains
- Publish to docker hub, homebrew, npm?, ...
- VSCode Plugin
- IntelliJ Plugin
- Show source code snippets (e.g. around TODOs) in web ui and cli

Chores:
- Replace parcel with vite
- Check if we're Windows compatible
- Test release workflow
- There are a lot of _ variables. We should check if they are all needed
- Check if any of the auto features can be applied to MCP and web endpoints (or they already are)

Bugs:
- We have an operation that creates an empty config.yml and nothing else 

---

# Implementation Roadmap — Smart Detection & Integrations (Aug 2025)

Legend: [ ] = TODO, [x] = Done, [~] = In Progress

## Phase 1 — Detection foundation
- [x] Detector trait and registry (Signals with value, confidence, provenance)
- [x] Git detector: repo root, current branch, remotes, user.name/email (repo-local > global)
	- Implemented: user.name/user.email + branch + remotes; repo root discovery (details exposed via explain)
- [x] Project-config detector: package.json, Cargo.toml, .csproj (authors)
- [x] System detector: OS username/hostname (fallback only)
- [x] Merge policy (identity): config.default_reporter → project manifest author → git user.name/email → system env
- [x] Configurable defaults override auto-detected values via feature toggles
- [x] whoami --explain shows sources and confidence per field (with toggle states and git details)
- [x] Caching and invalidation: added explain cache keyed by tasks root + env + git mtimes; invalidate function provided
- [x] Feature toggles: `auto.identity`, `auto.identity_git` added and enforced by detectors
 - [x] Config schema normalization: accept dotted and nested YAML, canonicalize to a single nested form
	 - Added `lotar config normalize` to rewrite files into the canonical nested form
 - [x] Validation: scan.ticket_patterns must be valid/non-ambiguous; clear errors/warnings
	- [x] whoami --explain indicates when smart features are disabled and defaults are used

## Phase 2 — Ownership and auto-assign
- [x] CODEOWNERS parser and path matcher (supports wildcards, directory rules)
- [x] Auto-assign on first non-initial status: CODEOWNERS owner > Git identity > system (config-gated)
- [x] Option to disable CODEOWNERS auto-assign; when off, use configured defaults and ignore detector results
- [~] Tests: precedence covered; add cases for multiple owners and no matches
- [x] Docs: using CODEOWNERS for ownership and auto-assign

## Phase 3 — Project context and monorepos
- [x] Monorepo discovery: cargo workspaces, npm/yarn/pnpm workspaces, go workspaces
	- Implemented upward detection: nearest package.json name (scope stripped), Cargo [package] name, go.mod module last segment; fallback to repo name then cwd; stops at repo root (supports .git dir/file)
- [x] Derive project id/name: prefer manifest names; fallback to repo name or project root directory name
	- Covered by the detection above; prefix generation unchanged
- [x] Derive labels/tags from paths (e.g., packages/foo => label: foo)
	- Heuristic: packages/<name>, apps/<name>, libs/<name>, services/<name>, examples/<name>; hidden names ignored; no generic leaf-dir fallback
- [x] Tests: nested workspaces, worktrees (/.git file), submodule-like structures
- [x] Docs/help: updated behavior and precedence; added monorepo-aware auto-tag notes

## Phase 4 — Branch/PR awareness
- [x] Branch conventions: infer default task type from branch (feat/feature → Feature; fix/bugfix/hotfix → Bug; chore/docs/refactor/test/perf → Chore), gated by `auto.branch_infer_type`
- [~] Config-aware mapping and aliases: type inference respects configured types with graceful fallback; status/priority inference and alias table pending
cra- [x] Flags/docs to opt-in/out via config (`auto.branch_infer_type`), with tests and help docs

--- CURRENT STATUS: CLEAN UP & REFACTOR ---

## Phase 5 — Source scan (MVP)
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
- [ ] Persistent bi-directional links between TODOs and tasks/tickets (minimal schema, human-readable):
		- Ticket files store a compact list of references; source carries the ticket key; no central index
		- Optional: when VCS-host integration is enabled, post a backlink (permalink to code location) to the ticket; otherwise provide a CLI to copy/share the permalink
- [ ] CLI: show-related (list files/snippets for a task/ticket), show-snippet (render code context around a TODO)
- [ ] Tests with sample polyglot repos

#### Current stub status (as of Aug 2025)
- [x] Recursive scan of supported single-line comment styles for TODO (case-insensitive)
- [x] Outputs JSON when `--format json` is used: [{ file, line, title, annotation }]
- [x] Text output with optional `--detailed` summary
- [x] `--include/--exclude` flags wired to file collector
- [x] Recognize FIXME/HACK/BUG/NOTE (default signal words)
- [x] Respects .lotarignore with fallback to .gitignore
- [ ] Block/multiline comment parsing (planned)
- [ ] Mutation (in-place key insertion) and dry-run diffs (planned)
- [ ] Movement/relocation resilience and re-anchoring (planned)

### Minimal ticket schema (simplified; typed references)

```yaml
ticket: DEMO-123
title: Improve retry logic
status: InProgress
references:
	# Built-in: code references (human-readable, no extra index)
	- code: packages/api/src/retry.ts#L118
	# optional symbol hint form
	- code: packages/api/src/retry.ts::fetchWithRetry#L118
	# Future: other reference types (not enforced yet)
	# - figma: https://www.figma.com/file/...
	# - jira: DEMO-999
```

Notes:
- Path and symbol are alternatives; include symbol only when helpful. Line is an advisory hint.
- The ticket id (DEMO-123) is the authoritative key; no separate pattern/last_seen fields.
- Audit trail will be added later as its own feature.

### Scan behavior specification
- [ ] Detection
	- [ ] Find TODOs and configurable signal words in comments (language-aware comment parsing)
	- [ ] Optional: treat known ticket types (e.g., DEMO-123 patterns) as signal words when enabled
	- [ ] Broad language support via a comment-format catalog (C/CPP/C#, Java, JS/TS, Python, Go, Rust, Ruby, Shell, Markdown, HTML, YAML/TOML/JSON comments)
- [ ] Configuration
	- [ ] `scan.signal_words: string[]` (default: ["TODO","FIXME"]) and per-project overrides
	- [ ] `scan.ticket_patterns: [string|regex]` templates/regex to detect keys (e.g., `<PREFIX>-<ID>`, `[ticket=<ID>]`, `$<PREFIX>-<ID>`) 
	- [ ] `scan.enable_ticket_words: bool` (default: false) to also trigger on ticket keys
	- [ ] `scan.apply_in_place: bool` (default: true) controls auto-modification of source; `--dry-run` prints proposed edits only
	- [ ] `scan.on_todo_deleted.enabled: bool` (default: true) and `scan.on_todo_deleted.status: string` to set ticket status when a linked TODO disappears
	- [ ] Validation: if configured status is not in project statuses, emit warning and skip automation
	- [x] Include/exclude globs (respect .gitignore by default, if .lotarignore exists use that instead)
	Note: current stub detects only TODO and does not yet honor these configuration values.
- [ ] Mutation
	- [ ] When a TODO without a ticket key is found:
		- If a signal word (issue type) is matched on the line, insert ` (KEY)` immediately after the matched token (default rule); idempotent
		- Otherwise, append a compact key marker (default: `[ticket=KEY]`) at a sensible spot at end-of-token; idempotent
		- `--dry-run` lists locations and diffs
	- [ ] Preserve formatting and comment style; avoid reflowing code

#### Ticket signal words and formatting
- [ ] Recognize issue-type signal words and inject the ticket key (simplified default)
	- Supported patterns (configurable, case-insensitive): `@Type`, `Type`, with optional nearby punctuation like `:`, `-`, wrappers like `<Type>`
	- Type must map to a configured project type (via alias table); otherwise warn and skip
	- Default insertion (idempotent): replace the matched span M with `M (KEY)`
		- Examples:
			- `// @Bug myTitle` → `// @Bug (DEMO-123) myTitle`
			- `// Feature: myTitle` → `// Feature (DEMO-123): myTitle`
			- `# Chore - tidy` → `# Chore (DEMO-123) - tidy`
			- `/* <BUG> foo */` → `/* <BUG (DEMO-123)> foo */`
	- Configurable placement (advanced): allow a template, e.g. `scan.insertion.template = "{matched} ({key})"` (default)
		- Other examples: `{matched} ({key}):`, `{matched} ({key}) -`, `<{matched} ({key})>`
	- Multi-line description (optional): treat contiguous same-style comment lines below as description; do not inject keys there; stop on blank/non-comment/indent change
	- Ticket key format is matched from `scan.ticket_patterns`; defaults include `<PREFIX>-<ID>` and `[ticket=ID]`
	- Ensure stable edits: avoid duplicate `(KEY)` insertions on the same line
- [ ] Deletion automation
	- [ ] When a previously linked TODO is deleted, automatically set ticket status per config; warn on invalid status; if unset, do nothing
- [ ] Movement/relocation resilience (git-first, no index)
 	- [ ] Primary set = git-modified and renamed files (git status/diff); optional --full to scan all; path args to limit scope
 	- [ ] Re-anchoring order (fast → robust): same-file exact key match → nearby window around previous line → optional symbol scope → git hunk/rename remap → bounded grep within modified set
 	- [ ] Candidate tickets for a changed file are found by grepping ticket files for that file path in references (code: entries); update/remove anchors accordingly
 	- [ ] No global scan-state or per-TODO index; rely on git timestamps and rename detection for incremental scans

## Phase 6 — Release follow-ups (optional)
- [ ] Include README and LICENSE in archives
- [ ] Universal macOS binary via lipo (optional)
- [ ] Post-upload verification job (download & verify checksum/signature)
- [ ] Package managers: Homebrew, Scoop, cargo-binstall, Docker image

## Acceptance gates per phase
- [ ] Build passes (lint/tests), new tests cover behavior and edge cases
- [ ] Docs/help updated; --explain outputs verified
 - [ ] Scan MVP: idempotence tests green; dry-run JSON shape has golden tests; performance bounded by git-modified set and guardrails

## Backlog — Later additions

Scan (post-MVP):
- [ ] Honor `.lotarignore` with fallback to `.gitignore`
- [ ] Wire `--include/--exclude` filtering in file collector
- [ ] Support additional signal words (FIXME, HACK, BUG, NOTE) via configurable `scan.signal_words`
- [ ] In-place mutation for key insertion + `--dry-run` unified diffs
- [ ] Movement/relocation re-anchoring and rename/hunk remap
- [ ] Block/multiline comment parsing and basic AST anchors where feasible
- [ ] CLI: `show-related`, `show-snippet` for tickets

Ticket files & references:
- [ ] Additional typed references (e.g., `figma:`, `jira:`)
- [ ] Optional audit trail (append-only) separate from references
- [ ] VCS-host backlinking (opt-in), permalink helpers

Ownership & context:
- [ ] Advanced branch/PR features: status/priority inference; branch/commit/PR linking; alias tables and per-project overrides

Release & distribution:
- [ ] Post-upload verify job, universal macOS binary, package managers (Phase 6)

Docs & UX:
- [ ] Dedicated docs for scan behavior, references schema, and ignore rules
- [ ] Examples/playground repo for polyglot scan tests