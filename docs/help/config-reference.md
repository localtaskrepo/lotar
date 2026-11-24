# Configuration Reference

Canonical configuration groups by scope, with types and effective built-in defaults. Canonical YAML only emits non-default values; a comment-only `.tasks/config.yml` still uses defaults. Inspect `lotar config show --full --format=json` for resolved flat fields under `.config`; this CLI envelope is not REST's `.data`. YAML/dotted names below are input names, not JSON response paths. Typed fields are defined in `crates/lotar-core/src/config/types.rs`; accepted dotted aliases, settable scopes and inspect labels are cataloged in `crates/lotar-core/src/config/fields.rs`.

## Precedence
CLI > env > project > home > global > defaults (commands without a project skip the project step). See [Resolution & Precedence](./precedence.md).

## Canonical keys (nested)
- server.port: number (default 8080)
- server.public_url: string (unset by default) — the address people open the web UI at, like `https://lotar.example.com`. When set, `lotar show` and `lotar add` print each ticket's link `<public_url>/task/<ID>` (the link the web UI's "copy link" builds), the task JSON of `show`, `add`, `list`, REST and MCP has it as `url`, and an agent job's prompt has it as `- Link:` under the ticket's ID (so a T3 Code thread links back to its ticket). Unset, no link is printed or returned. It must start with `http://` or `https://` and have no `?` or `#`; a trailing `/` is dropped.
- default.project: string (default project prefix)
- default.assignee: string
- default.reporter: string
- default.priority: configured string (default Medium)
- default.status: optional configured string (unset by default; creation uses the first configured state)
- default.tags: string[]
- default.strict_members: boolean (default false)
- members: string[]
- review.reviewers: string[] (project only; default empty) — people besides the reporter who may resolve review status and assignee changes. Entries are plain names or emails, not `@me`. Set it with `--project`; see [config](./config.md).
- issue.states: string[] (e.g., [Todo, InProgress, NeedsReview, Done])
- issue.done_states: optional nonempty string[] of finished task statuses, all drawn from the effective `issue.states`. An explicit list replaces inference; omitted project values inherit, and without any explicit list LoTaR infers completion after resolving the project's workflow. See [Done States](./config.md#done-states).
- issue.types: string[] (built-in [Feature, Bug, Chore])
- issue.priorities: string[] (built-in [Low, Medium, High]; Critical requires configuration)
- issue.tags: list of tag names (default wildcard ["*"]); an entry can also be an object with properties, `{ name: ui, color: "#0ea5e9" }`. See [Tag colours](./config.md#tag-colours)
- issue.tag_properties: map of tag name to properties (`color`), for a project that colours inherited tags without its own `issue.tags` list; `config set tag_properties` writes it
- custom_fields: string[] (default wildcard ["*"]); an entry can also be an object with a type, `{ name: affects, type: version }`. See [Version fields](./config.md#version-fields)
- custom.field_properties: map of field name to properties (`type`: `text` or `version`), for a project that types inherited fields without its own `custom.fields` list; `config set custom_field_properties` writes it
  - For both lists, `["*"]` allows any value, a named list allows only those values, and `[]` allows none (for example, it turns custom fields off). In project scope a blank `config set` value clears the override, so set an explicit empty list with `lotar config set custom_fields '[]' --project=DEMO`.
- ui.custom_fields_from: `config`, `scanned` or `both` (default `both`), global or per project. Which custom fields the web UI offers as columns, card fields, filters, **Group by** swimlanes and bulk edit choices: `config` the ones `custom_fields` names, `scanned` the ones the project's tasks use, `both` the named ones first, then the used ones, most used first. Names that differ only in case are one field (the named spelling, else the most used one); `source_repo` and `Source Repo` stay two fields. With the default wildcard, `both` offers the fields the tasks use. Bulk edit offers only names the server accepts: under a named list, only those
- attachments.dir: string (default "@attachments")
- attachments.max_upload_mb: number (default 10) — `0` disables uploads; `-1` is unlimited (the server still stops at 512 MiB per upload); positive values are MiB
- sync.reports_dir: string (default "@reports")
- sync.write_reports: boolean (default true)
- agent.context_enabled: boolean (default true) — when true, LoTaR stores per-ticket `.context` files to support agent resume/continue; disable for stateless runs. If you don’t want to share context files, add `.context` to `.gitignore`.
- agent.context_extension: string (default ".context")
- agent.logs_dir: optional string (default `<tasks dir>/@agent-logs`). Job events are written to `<logs_dir>/<JOB_ID>.jsonl` so `lotar agent status`/`logs`/`list-jobs` and the Agents page find finished jobs. Relative paths resolve from the workspace root; `off` turns logging off. The folder gets a `.gitignore`; logs are kept by `agent.job_history`; see [Agent Jobs](./agent.md).
- agent.instructions: string | { file: string } (default: built-in) — prepended to agent prompts. Inline strings are used directly; file paths can be absolute or relative to the tasks directory.
- agent.worktree.enabled: boolean (default false) — when true, agent jobs run inside a git worktree.
- agent.worktree.dir: string (default computed) — optional worktree root directory; relative paths resolve from the repo’s parent directory.
- agent.worktree.build_root: string (default none) — folder for the worktrees' build output: a new worktree's `target` links to `<build_root>/<repo>/<ticket-id>`, which cleanup removes with the worktree; relative paths resolve from the repo’s parent directory. Needs `target` ignored as `/target`; see [Agent Jobs](./agent.md).
- agent.worktree.branch_prefix: string (default "agent/") — prefix used when creating agent branches.
- agent.worktree.max_parallel_jobs: optional whole number (unset means unlimited). Further jobs wait in the queue; `0` pauses new jobs.
- agent.worktree.cleanup_on_done / cleanup_on_failure / cleanup_on_cancel / cleanup_delete_branches: booleans (default false). Eligibility and clean/merged/active-job safeguards still apply; see [Agent Jobs](./agent.md).
- agent.worktree.cleanup_ignored_files: boolean (default true). When true, git-ignored files (build output) don't keep a worktree from being cleaned up; untracked and modified files always do. Set false to keep any worktree that holds ignored files.
- agent.worktree.cleanup_paths: list of paths inside the worktree (default none), like `[target]`. When a job ends and no other job runs for its ticket, LoTaR deletes these paths from the job's worktree if git ignores them and tracks nothing under them; the worktree stays. Paths must be relative, without `..` or `.git`. `config set` takes a comma-separated list or a YAML list.
- agent.worktree.waiting_run: list, program first (default none), like `[cargo, clean, --workspace]`. When a job ends with its ticket HelpNeeded or Blocked, or a job is held back for open prerequisites, LoTaR runs it in each of the ticket's worktrees, without a shell; see [Build output while a ticket waits](./agent.md#build-output-while-a-ticket-waits). From a repository's config it runs only after [`lotar trust`](./trust.md); from your home config it always runs. `config set` takes a comma-separated list or a YAML list; an empty value turns it off.
- agent.memory_gate.enabled: boolean (unset: on on macOS, off elsewhere) — while the machine is short of memory, new agent jobs wait in the queue ([memory gate](./agent.md#memory-gate)). Global config only; not settable with `config set`.
- agent.memory_gate.max_swap_percent: whole number (default 85) — swap use, in percent of the swap total, from which new jobs wait.
- agent.job_history.keep_days: whole number (default 14) — days a job log is kept after it was last written; jobs of open tickets and running jobs are always kept ([job history](./agent.md)). Global config only; not settable with `config set`.
- agent.job_history.max_mb: whole number (default 100; 0: no limit) — MiB a log folder's job logs may take; over it, the oldest removable logs go first.
- agent.memory_gate.settle_seconds: whole number (default 120) — how long memory has to stay normal before held jobs start.
- All `agent.worktree.*` settings can be changed with `config set` (global or per project) using their flat names: `agent_worktree_enabled`, `agent_worktree_dir`, `agent_worktree_build_root`, `agent_worktree_branch_prefix`, `agent_worktree_max_parallel_jobs`, `agent_worktree_cleanup_on_done`, `agent_worktree_cleanup_on_failure`, `agent_worktree_cleanup_on_cancel`, `agent_worktree_cleanup_delete_branches`, `agent_worktree_cleanup_ignored_files`, `agent_worktree_cleanup_paths` and `agent_worktree_waiting_run`, or (except `build_root` and `waiting_run`) on the Config page under Agents. An empty value clears a project override; an empty job limit means no limit.
- agent.merge: optional map (default none) — lotar merges a merge job's branch itself (rebase, checks, fast-forward, push) and starts the merge agent only for what needs judgment; see [Agent Jobs](./agent.md#lotars-own-merge). A project's `agent.merge` replaces the repository's whole. It runs commands, so `lotar trust` covers it like rules and agent profiles. Set it in the config file; `config set` doesn't take it.
  - agent.merge.checks: string[] — commands run through the shell in the ticket's worktree after the rebase, in order, under the machine's check lock ([one full check run at a time](./agent.md#lotars-own-merge))
  - agent.merge.install: optional string — runs before the checks when the rebase changed a lockfile (`package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`) or the worktree is new, e.g. `npm ci`
  - agent.merge.push: boolean (default false) — push the main checkout's branch after landing
  - agent.merge.finding_check: optional string — runs in the main checkout after landing a ticket with a finding key (`lotar add --finding`); its output must no longer contain `[<key>]`. The key is in `LOTAR_FINDING`.
- agent.automation: retired. Old files that still have this block load fine and keep it as written, but it does nothing and `lotar config validate` warns and prints the equivalent rule. Agent job status changes and reassignment are `job_started` / `job_completed` / `job_failed` / `job_cancelled` rules in the automation file; see [Automation Rules](./automation.md#agent-job-queueing). The `agent_on_*` keys of `config set` were removed.
- agents: map (named agent CLI profiles; values may be a runner string or a full object)
  - agents.<name>.runner: `claude` | `codex` | `copilot` | `gemini` | `cursor` | `amp` | `droid` | `opencode` | `kilo` | `antigravity` | `t3` (the [harnessmux](https://github.com/localtaskrepo/harnessmux) agent of that name) | `command`
  - agents.<name>.description: optional string — what the profile is for; the assignee picker shows it under the profile's name and its search matches it
  - agents.<name>.permissions: `read-only` | `workspace` | `full` (default `workspace`) — what the agent may do without asking; see [Agent Jobs](./agent.md#permissions)
  - agents.<name>.on_permission: `deny` | `allow-reads` | `allow` | `ask` (default `allow-reads` for `read-only`, else `allow`) — what a job does when the agent asks for permission; `ask` shows the question in the job view. See [Messages and questions](./agent.md#messages-and-questions)
  - agents.<name>.model / effort: optional strings — the agent's model id, and effort `low` | `medium` | `high` | `xhigh` | `max` (any value for `command`); runs and automation actions can override them. See [Agent Jobs](./agent.md#model-and-effort)
  - agents.<name>.options: map — the agent's own ACP config options, id to value (`harnessmux capabilities <agent>` lists them), for values outside `effort`'s such as `reasoning_effort: ultra`; not for `command`
  - agents.<name>.provider: optional string — `t3` runner only: the T3 Code provider instance (`claudeAgent`, `codex`, …) the model runs on; set it together with `model`. See [T3 Code threads](./agent.md#t3-code-threads)
  - agents.<name>.thread_name: optional string — `t3` runner only: the T3 Code thread's title, with the ticket's `${{...}}` templates filled in when the job starts (for example `"${{ticket.id}} fix: ${{ticket.title}}"`); default `<ID> <title>`. A continued thread is renamed to it before its turn. A rule's `agent.thread_name` wins. See [T3 Code threads](./agent.md#t3-code-threads)
  - agents.<name>.session: `new` | `continue` | `fresh` (default `new`) — `continue` runs the job as the next turn of the ticket's session of this profile; `new` and `fresh` start a new one. See [Sessions](./agent.md#sessions)
  - agents.<name>.context_entries: integer (default 2) — how many of the ticket's newest stored prompts and replies the job's prompt carries, each cut at 1,500 characters; `0` carries none. See [Context between phases and agents](./agent.md#context-between-phases-and-agents)
  - agents.<name>.full_history: boolean (default false) — the job's prompt carries every stored prompt and reply uncut and no comment excerpts, as before DEV-620; wins over `context_entries`
  - agents.<name>.continue_within_minutes: integer (default 55) — a `continue` job resumes the session only when its last turn ended less than this many minutes ago; otherwise it starts a new session with a handoff in its prompt. `0` never resumes. See [Sessions](./agent.md#sessions)
  - agents.<name>.tools.allowed / tools.disallowed: string[] — tools allowed or refused without `on_permission`, by the agent's name for them (trailing `*` matches a prefix); agents.<name>.mcp.allow: string[] — MCP servers whose tools are allowed. See [Agent Jobs](./agent.md#permissions)
  - agents.<name>.command: string — the program to run. For the other runners it replaces the agent's command from harnessmux's `agents.toml` (and its pinned adapter) for this profile's jobs
  - agents.<name>.args: string[] — the program's arguments. For the other runners without `command` they go after the agent's command
  - agents.<name>.env: map — extra environment variables for the agent
  - `agents` can be set as a whole with `lotar config set agents '<json or yaml map>'` (project or `--global`); names may use letters, digits, `-`, `_` and `.`, and each runner must be one of the runners in [Agent Jobs](./agent.md#runners).
  - agents.<name>.mcp.lotar: boolean (default false) — give the job LoTaR's own MCP server, limited to ticket tools (not for `amp`, `t3` or `command`)
  - agents.<name>.max_budget_usd: number — the job's turn is cancelled and the job fails once the cost the agent reports passes it. See [Usage and cost](./agent.md#usage-and-cost)
  - agents.<name>.stall_timeout_seconds: integer — a job whose agent sends nothing for this many seconds fails as stalled; unset or `0`: no limit. See [Stalls and tickets that move on](./agent.md#stalls-and-tickets-that-move-on)
  - agents.<name>.instructions: string | { file: string } — added after `agent.instructions` (or the built-in instructions) for this profile
- scan.signal_words: string[] (default: [TODO, FIXME, HACK, BUG, NOTE])
- scan.ticket_patterns: string[] (regex patterns to detect ticket keys)
- scan.enable_ticket_words: boolean (built-in default: true) — issue-type words act as signal words in addition to TODO/FIXME/etc. Bare ticket keys alone never create tasks.
- scan.enable_mentions: boolean (default: true) — when true, add code anchors under `references` for existing ticket keys found in source
- scan.strip_attributes: boolean (default: true) — when true, remove inline [key=value] attribute blocks from source after inserting the ticket key
- auto.identity: boolean (default true)
- auto.identity_git: boolean (default true)
- auto.set_reporter: boolean (default true)
- auto.reciprocal_relationships: boolean (default true). If true, adding a relationship also writes the matching side on the other task (Depends on ↔ Blocks, Parent ↔ Children, Related ↔ Related). When false, only the task you edit changes and `relationships check` does not report the missing side; removing a link still removes both sides.
- auto.link_mentions: boolean (default true). If true, ticket IDs newly written in a description or comment add a `related` link (on both tasks, following `auto.reciprocal_relationships`). Tasks already linked in any way, unknown IDs, the task itself and IDs inside code are skipped; links are only added, never removed when the text changes.
- auto.assign_on_status: boolean (default true)
- auto.codeowners_assign: boolean (default true)
- auto.tags_from_path: boolean (default true)
- auto.branch_infer_type: boolean (default true)
- auto.branch_infer_status: boolean (default true)
- auto.branch_infer_priority: boolean (default true)
- auto.populate_members: boolean (default true)
- branch.type_aliases / branch.status_aliases / branch.priority_aliases: maps of branch token to configured label (default empty)
- sprints.defaults.length: optional positive duration in days or weeks (`14d`, `2w`, `3 weeks`); sprints.defaults.overdue_after: optional grace period of zero or more hours, days or weeks (`12h`, `1d`). Both are capped at ten years; see the recipes below
- sprints.defaults.capacity_points / capacity_hours: optional nonnegative numbers
- sprints.notifications.enabled: boolean (default true; project-overridable)
- web_ui_path: optional global-only path (top-level spelling), for an external web bundle
- repos: optional global-only map of repo name to path, naming other repositories' LoTaR for relationships such as `depends_on: [lotar:DEV-374]` (see [tasks in another repo](./task.md#tasks-in-another-repo)). A path is the repository (its `.tasks` folder is used) or the tasks folder itself; `~/` is the home folder and a relative path starts at the repository holding this tasks folder. Names use letters, digits, `.`, `_` and `-`. Paths are usually machine-specific, so `~/.lotar` is a good place; the repository's own entries win for the same name
- ui.theme: optional global-only default web UI theme, written `skin` or `skin:palette`. Skins and palettes (first = default): `classic` (lotar, ocean, forest, ember, violet, rose), `contrast`, `terminal` (amber, green, red, cyan, white), `matrix` (green, red, blue), `synthwave` (sunset, outrun, ice), `blueprint` (blue, green, sepia), `paper` (blue, black, red, green), `retro` (teal, plum, slate, desert), `candy` (bubblegum, mint, grape), and `custom` for the repository skin below. The ids `lotar`, `ocean`, `forest`, `ember`, `violet` and `rose` from before palettes still work (`classic:<id>`). Set it in the repository's config for everyone, or in your own `~/.lotar` (which wins) for yourself; people can still pick a theme per browser in Preferences
- ui.theme_file: optional global-only repository skin: a `.css` file (path relative to the repository) that sets theme variables, e.g. `:root { --color-accent: #c0392b; --font-heading: Georgia, serif; }` with dark-mode values under any selector or `@media (prefers-color-scheme: dark)` block mentioning `dark`. Only custom properties are used (no `url()`, `@import` or other rules); they apply when `ui.theme` (or a person's choice) is `custom`. The variable names are the ones in `view/styles.css` and `view/themes.css`
- ui.name: optional global-only name shown next to the logo and as the browser tab title (at most 40 characters). Without it the tab reads `<repository folder> · LoTaR`
- ui.logo / ui.logo_dark / ui.icon: optional global-only images for the header logo, its dark-mode version, and the browser tab icon. Paths are relative to the repository (the folder holding the tasks directory), e.g. `assets/app-icon.svg`; files outside it, symlinks out of it, and files that are not SVG/PNG/JPEG/WebP/GIF/ICO/AVIF images are never served. Images uploaded in Config → Appearance are stored in `.tasks/@branding/`. Without `ui.icon` the tab shows the repository's own icon when it has one: the first image named `favicon.svg`, `favicon.ico`, `favicon.png`, `icon.svg` or `icon.png` at the repository root or in `public/`, `static/`, `assets/`, `app/`, `src/`, `src/app/` or `web/`
- remotes: map (named remotes)
- remotes.<name>.provider: enum (jira|github)
- remotes.<name>.project: string (Jira project key)
- remotes.<name>.repo: string (GitHub owner/repo)
- remotes.<name>.filter: string (JQL or GitHub qualifiers)
- remotes.<name>.auth_profile: string (home profile name)
- remotes.<name>.mapping: map (local field name -> mapping)
- remotes.<name>.mapping.<field>.field: string (remote field name)
- remotes.<name>.mapping.<field>.values: map (local value -> remote value)
- remotes.<name>.mapping.<field>.set: string (force remote value)
- remotes.<name>.mapping.<field>.default: string (apply when local value is empty)
- remotes.<name>.mapping.<field>.add: string[] (append for list fields like tags/labels)
- remotes.<name>.mapping.<field>.when_empty: enum (skip|clear)
- Note: mapped local values for `status`, `priority`, and `type` are validated against the target project's `issue.states`/`issue.priorities`/`issue.types`. A remote value that maps to a local value outside those sets fails for that issue (recorded as a sync warning/failed entry listing the valid values) instead of being persisted silently — configure the project enums or extend the mapping values accordingly.
- auth_profiles: map (home config only; ignored in project configs)
- auth_profiles.<name>.provider: enum (jira|github)
- auth_profiles.<name>.method: string (basic|token|bearer|pat; default basic for Jira, token for GitHub)
- auth_profiles.<name>.token_env: string (env var name or literal token)
- auth_profiles.<name>.email_env: string (env var name or literal email)
- auth_profiles.<name>.base_url: string (Jira site URL)
- auth_profiles.<name>.api_url: string (API address override, such as GitHub Enterprise; GitHub defaults to https://api.github.com)

## Home and Project keys
Home/global/project schemas are not identical. Project values override inherited fields for that project; `project.name` is an optional label, not the folder identifier. `server.port`, `server.public_url`, `default.project`, sprint defaults, `web_ui_path`, `repos` and `ui.*` (except `ui.custom_fields_from`) are global-only; sprint notifications and the listed project automation flags can be overridden. See [config scope](./config.md) before copying fields between files.

Auth profiles are home-only: `auth_profiles` is ignored in project configs (secrets never live in the repo). Remotes can be configured globally or per-project; project remotes override/extend global remotes.

Project configs can also override any automation toggle inherited from global defaults. Supported keys: `auto.populate_members`, `auto.set_reporter`, `auto.reciprocal_relationships`, `auto.link_mentions`, `auto.assign_on_status`, `auto.codeowners_assign`, `auto.tags_from_path`, `auto.branch_infer_type`, `auto.branch_infer_status`, `auto.branch_infer_priority`, `auto.identity`, and `auto.identity_git`.

> Legacy: older configs may include `issue.categories`. The key is normalized for compatibility but is not consumed by the runtime. Prefer modeling these labels with `custom_fields` (the dotted alias remains accepted for compatibility).

## Examples

These are three separate files, not one YAML document with duplicate keys. Spaces, not tabs, are required for indentation.

```yaml
# ~/.lotar (home config)
default:
  project: DEMO
```

```yaml
# .tasks/config.yml (global)
issue:
  states: [Todo, InProgress, NeedsReview, Done]
  priorities: [Low, Medium, High]
scan:
  signal_words: [TODO, FIXME]
  ticket_patterns: ["[A-Z]{2,}-\\d+"]
  enable_ticket_words: true
  enable_mentions: true

attachments:
  # Relative paths resolve under the tasks directory
  dir: "@attachments"
  # 0 disables uploads, -1 is unlimited, positive values are MiB
  max_upload_mb: 10

# Web UI look for everyone using this repository
ui:
  theme: terminal:green
  name: Acme Shop
  # Paths relative to the repository root
  logo: assets/logo.svg
  logo_dark: assets/logo-dark.svg
  icon: assets/app-icon.svg
  # Your own skin (theme variables only); pick it with theme: custom
  # theme_file: assets/lotar-theme.css

sync:
  reports_dir: "@reports"
  write_reports: true

agent:
  context_enabled: false
  instructions: "Follow the project's checked-in development guidance."
  worktree:
    enabled: true
    branch_prefix: "agent/"

# Automation rules live in a separate file:
# - Global: .tasks/automation.yml
# - Project: .tasks/<PROJECT>/automation.yml
# - Home: ~/.lotar.automation.yml

agents:
  myclaude: claude
  copilot-default:
    runner: copilot
  codex-default:
    runner: codex
```

```yaml
# .tasks/DEMO/config.yml (project)
project:
  name: Demo Service
issue:
  types: [Feature, Bug, Chore]
attachments:
  max_upload_mb: 25
scan:
  enable_mentions: false
auto:
  identity: true
  identity_git: true
  assign_on_status: true
default:
  tags: [oncall, sev]

remotes:
  jira-home:
    provider: jira
    project: DEMO
    auth_profile: jira.default
    mapping:
      title: summary
      status:
        field: status
        values:
          Todo: to-do
          Done: done
```

The remote example is configuration only. Before a real sync, configure its home auth profile as documented in [Sync](./sync.md); validation of YAML is not a credential or connectivity check.

## Duration and JSON Recipes

```bash
# Preview before persisting global sprint defaults
lotar config set sprints_defaults_length 2w --global --dry-run
lotar config set sprints_defaults_overdue_after 12h --global --dry-run
lotar config show --full --format=json | jq '.config.sprint_defaults'

# Read configured labels, including built-in values omitted from a partial show
lotar config show --project=DEMO --full --format=json | jq -r '.config.issue_priorities[]'
```

Nested YAML paths are not all exposed as dotted CLI mutation aliases: the sprint recipes deliberately use the shipped flat setter names. `config set`, its dry run, REST and MCP reject a sprint default duration outside that grammar (for example `not-a-duration`, `12h` as a length, `0w`, or more than ten years) without writing, and `--force` does not bypass it; an empty value clears the default. A malformed value already saved in a config file is still loaded so unrelated commands keep working: `lotar config validate` reports it as a warning, and sprint status treats an unusable length as unknown. Environment-variable values are applied as given. Explicit sprint `--length`/`--overdue-after` values are not validated by this check. Use [sprint](./sprints.md) for length/grace inputs and [automation](./automation.md) for its separately validated cooldown grammar.

## Validation
- Values are checked against allowed enums.
- Unknown keys are ignored or flagged during validation (see `lotar config validate`).
- If `scan.ticket_patterns` include invalid regex or overlapping patterns, validation will report errors/warnings.
