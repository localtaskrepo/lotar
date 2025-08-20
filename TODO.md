Features:
- Task relationship queries and graphs
- Effort
- Shell completion with install command
- Git Hooks (e.g. for scanner)
- Project members property (for auto fill in web interface)
- Allow env override for all config values that are in all other chains
- Publish to docker hub, homebrew, npm?, ...
- VSCode Plugin (Contexts?, Issue updates, in-editor quick hints for TODOs with references/quick create dialogs)
- IntelliJ Plugin
- Show source code snippets (e.g. around TODOs) in web ui and cli
- Statistics for analysis
- lock issue file in git if in progress? (or provide a command e.g. lotar task un-/lock <Task-ID>)

Chores:
- Replace parcel with vite
- Check if we're Windows compatible
- Test release workflow
- Check if any of the auto features can be applied to MCP and web endpoints (or they already are)
- Tests names are a mess
- Config validation may need an update

Bugs:
- We have an operation that creates an empty config.yml and nothing else
- CI job is failing because of clippy for some reason
- Help output shows raw markdown (Maybe we should split docs from direct help and more detailed help linked to)
- `lotar scan src` in this project throws an error

---

– Shared relative date/time parser for due & stats windows (single source of truth)
	• Added utils/time.rs: parse_human_datetime_to_utc() and parse_since_until()
	• Next: refactor due-date handler to reuse it; use for stats --since/--until

# Implementation Roadmap

Legend: [ ] = TODO, [x] = Done, [~] = In Progress

## Feature: Comments

- [x] CLI: support positional text, -m/--message, -F/--file, and stdin
- [x] Storage: TaskComment drops author (use git blame/history for attribution)
- [x] Handler: richer JSON with added_comment {date,text}
- [x] Tests: positional, -m flag, and empty-content error
- [ ] Docs: help page for `comment` with examples and JSON output

## Backlog
- [ ] Include README and LICENSE in archives
- [ ] Universal macOS binary via lipo (optional)
- [ ] Post-upload verification job (download & verify checksum/signature)
- [ ] Package managers: Homebrew, Scoop, cargo-binstall, Docker image
