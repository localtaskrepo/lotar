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
- VSCode Plugin (Contexts?, Issue updates, in-editor quick hints for TODOs with references/quick create dialogs)
- IntelliJ Plugin
- Show source code snippets (e.g. around TODOs) in web ui and cli

Chores:
- Replace parcel with vite
- Check if we're Windows compatible
- Test release workflow
- Check if any of the auto features can be applied to MCP and web endpoints (or they already are)
- Tests names are a mess
- Config validation may need an update

Bugs:
- We have an operation that creates an empty config.yml and nothing else

---

# Implementation Roadmap — Smart Detection & Integrations (Aug 2025)

Legend: [ ] = TODO, [x] = Done, [~] = In Progress

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
- [x] Honor `.lotarignore` with fallback to `.gitignore`
- [x] Wire `--include/--exclude` filtering in file collector
- [x] Support additional signal words (FIXME, HACK, BUG, NOTE) via configurable `scan.signal_words`
- [x] In-place mutation for key insertion + `--dry-run` unified diffs
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