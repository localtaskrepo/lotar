Features:
- Task relationship queries and graphs
- Sprints
- Shell completion with install command
- Git Hooks (e.g. for scanner)
- Project members property (for auto fill in web interface)
- Allow env override for all config values that are in all other chains
- Publish to docker hub, homebrew, npm?, ...
- VSCode Plugin (Contexts?, Issue updates, in-editor quick hints for TODOs with references/quick create dialogs)
- IntelliJ Plugin
- Show source code snippets (e.g. around TODOs) in web ui and cli
- lock issue file in git if in progress? (or provide a command e.g. lotar task un-/lock <Task-ID>)
- custom properties can be used to filter and query. Custom properties are accessed like any other property (no custom: prefix anywhere)
- Localization

---------- NEXT ----------
- Tests names are a mess, way to many single test files, need to speed up whole suite
--------------------------

Chores:
- Replace parcel with vite
- Check if we're Windows compatible
- Test release workflow
- Check if any of the auto features can be applied to MCP and web endpoints (or they already are)
- Config validation may need an update
- properties that don't have any special functions associated with them (e.g. categories) should be custom properties that just allow generic querying by matching terms like all custom properties should support. Only when we add special function should we promote them to standard fields.
- we have src/utils_git.rs, why is this not in src/utils/git.rs?

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

## Feature: Effort

Goals
- Support multiple effort systems (time and points first; mixed allowed) with a single normalized parser.
- Provide powerful, generic stats by status/assignee/type/category/project/tags/project-declared fields.
- No special "done" or "sprint" configuration in this phase; rely on filters (status=…) and project-declared fields.
- Project-declared fields are first-class: stored at top-level in task files; no legacy bags or formats.
- Unified keys everywhere (no prefixes): users write the same key for built-ins and project defined custom fields.

Milestones
- [x] M1: Core Effort util (parse/normalize)
	- [x] Add `src/utils/effort.rs` with:
		- [x] EffortParsed/EffortKind (time_hours|points) and canonical string formatter
		- [x] Parse inputs: minutes(m), hours(h), days(d), weeks(w); combined (e.g., "1d 2h", "90m"); decimals; trim; lowercase; reject negatives
		- [x] Parse points: `p|pt|pts` and bare numbers; reject mixing with time within one expression
	- [x] Replace `cli/validation::validate_effort` to delegate to the new util (BC: accept only h/d/w for now)
	- [x] Normalize effort on write (add/edit/scan/API) to canonical form; preserve semantics
	- [x] Backward compatibility: prior strings remain valid and parseable
	- [x] Unit tests for util (minutes/combined/points/mixed invalid)

- [x] M2: Generic stats (grouping, filters, windows)
	- [x] Extend `stats effort` args:
		- [x] `--by <key>` where <key> is any built-in or project-declared field (status|assignee|type|category|project|tags|sprint|…)
		- [x] `--where key=value` (repeatable). Keys resolve to built-ins first, then project-declared fields. For tags accept `tag=<val>` alias.
		- [x] `--unit` hours|days|weeks|points|auto
		- [x] `--since` / `--until` using the shared relative date/time parser in utils/time.rs
		- [x] `--transitions` (opt-in): filter tasks that changed into a status within the window via git history
	- [x] Implement aggregation basics with unified field resolver; includes hours/points handling and JSON/text outputs
	- [x] Performance: guardrail for large datasets (aggregation cap via LOTAR_STATS_EFFORT_CAP)

- [x] M3: CLI UX for effort and lists
	- [x] Add `lotar task effort <ID> [<effort NEW>] [--clear] [--dry-run] [--explain]`
		- [x] Wire CLI args (subcommand added) and top-level `lotar effort <ID> [NEW]`
		- [x] Implement view current effort if NEW not provided
		- [x] Implement set/clear effort with normalization; support `--dry-run` and `--explain`
	- [x] List enhancements:
		- [x] `--sort-by <key>` unified (added effort and custom field keys via `field:<name>`) with reverse support
		- [x] `--where key=value` unified filtering for list (built-ins and custom fields with fuzzy matching)
		- [x] `--effort-min` / `--effort-max` (accept effort formats; compares within same kinds time/points)

- [x] M4: Project-declared fields as first-class
	- [x] Collision guard: when project config declares a field that matches a built-in, error and guide to use the built-in
	- [x] Writes: `--field key=value` errors for reserved built-in names; otherwise key must be declared in project config
	- [x] Filtering/grouping/sorting parity: users pass plain keys; resolver handles built-in vs project fields (docs updated)
	- [x] API: ensure server-side filters accept property names uniformly (built-in and project-declared); supports @me and fuzzy matching; OpenAPI/help updated; tests added

- [ ] M5: Scanner integration
	- [x] Validate/normalize inline `effort` via Effort util
	- [x] Accept minutes and combined time inputs in inline attributes

- [~] M6: Tests
	- [x] Effort util unit tests: minutes/combined, points, mixed-invalid, canonicalization basics
	- [x] CLI tests: add/edit normalization to canonical form
	- [x] Stats tests: `--unit` (hours/days/weeks/points/auto) and `--where` filters (including @me), grouping by tag/assignee
	- [x] List: `--sort-by effort` coverage; `--effort-min/max` filters coverage
	- [x] Stats: `--since/--until` window with `--transitions` coverage (git-derived)
	- [ ] Scanner: inline effort variations
	- [x] Collision tests: custom field name colliding with built-ins rejected in CLI/config

- [~] M7: Docs
	- [x] New `docs/help/effort.md`: formats (time & points), normalization, examples
	- [x] Updated Add/Edit/List/Stats docs: unified keys (`--by <key>`, `--where key=value`, `--sort-by <key>`), effort min/max, units (incl. auto)
	- [x] API docs: `/api/tasks/list` flexible filters with @me and fuzzy matching noted in help and OpenAPI
	- [x] Stats docs: expand transitions/window examples and response fields per unit

- [ ] M8: Quality gates
	- [ ] Clippy/rustfmt clean; ensure no panics on malformed inputs
	- [ ] Backward compatibility validated on existing tasks with effort strings
	- [ ] Smoke performance run on a repo with 5k tasks (stats and list paths)

Non-goals (this phase)
- No dedicated "done" status configuration; users filter by `status=<value>` themselves
- No sprint configuration; users use any property via unified keys (`--where` / `--by` / `--sort-by`)
- No legacy formats or config support; we can break to improve design since unreleased

Acceptance criteria
- Adding/editing/scanning effort normalizes values consistently; invalid inputs get clear errors
- `stats effort` supports grouping, generic filters, unit control, and time windows; works without any done/sprint config
- Custom fields can be used everywhere users can use built-ins for filtering and grouping, with safe collision checks
- Tests and docs cover time-and-points, mixed datasets, and status-in-window queries

## Backlog
- [ ] Include README and LICENSE in archives
- [ ] Universal macOS binary via lipo (optional)
- [ ] Post-upload verification job (download & verify checksum/signature)
- [ ] Package managers: Homebrew, Scoop, cargo-binstall, Docker image
