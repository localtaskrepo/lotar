# Configuration Reference

Canonical configuration groups by scope, with types and effective built-in defaults. Canonical YAML only emits non-default values; a comment-only `.tasks/config.yml` still uses defaults. Inspect `lotar config show --full --format=json` for resolved flat fields under `.config`; this CLI envelope is not REST's `.data`. YAML/dotted names below are input names, not JSON response paths. Typed fields are defined in `src/config/types.rs`; accepted dotted aliases, settable scopes and inspect labels are cataloged in `src/config/fields.rs`.

## Precedence
CLI > project > env > home > global > defaults (commands without a project skip the project step). See [Resolution & Precedence](./precedence.md).

## Canonical keys (nested)
- server.port: number (default 8080)
- default.project: string (default project prefix)
- default.assignee: string
- default.reporter: string
- default.priority: configured string (default Medium)
- default.status: optional configured string (unset by default; creation uses the first configured state)
- default.tags: string[]
- default.strict_members: boolean (default false)
- members: string[]
- issue.states: string[] (e.g., [Todo, InProgress, NeedsReview, Done])
- issue.done_states: optional nonempty string[] of finished task statuses, all drawn from the effective `issue.states`. An explicit list replaces inference; omitted project values inherit, and without any explicit list LoTaR infers completion after resolving the project's workflow. See [Done States](./config.md#done-states).
- issue.types: string[] (built-in [Feature, Bug, Chore])
- issue.priorities: string[] (built-in [Low, Medium, High]; Critical requires configuration)
- issue.tags: list of tag names (default wildcard ["*"]); an entry can also be an object with properties, `{ name: ui, color: "#0ea5e9" }`. See [Tag colours](./config.md#tag-colours)
- issue.tag_properties: map of tag name to properties (`color`), for a project that colours inherited tags without its own `issue.tags` list; `config set tag_properties` writes it
- custom_fields: string[] (default wildcard ["*"])
  - For both lists, `["*"]` allows any value, a named list allows only those values, and `[]` allows none (for example, it turns custom fields off). In project scope a blank `config set` value clears the override, so set an explicit empty list with `lotar config set custom_fields '[]' --project=DEMO`.
- attachments.dir: string (default "@attachments")
- attachments.max_upload_mb: number (default 10) — `0` disables uploads; `-1` is unlimited; positive values are MiB
- sync.reports_dir: string (default "@reports")
- sync.write_reports: boolean (default true)
- agent.context_enabled: boolean (default true) — when true, LoTaR stores per-ticket `.context` files to support agent resume/continue; disable for stateless runs. If you don’t want to share context files, add `.context` to `.gitignore`.
- agent.context_extension: string (default ".context")
- agent.logs_dir: optional string (default `<tasks dir>/@agent-logs`). Job events are written to `<logs_dir>/<JOB_ID>.jsonl` so `lotar agent status`/`logs`/`list-jobs` and the Agents page find finished jobs. Relative paths resolve from the workspace root; `off` turns logging off. The folder gets a `.gitignore` and keeps the newest 500 job logs; see [Agent Jobs](./agent.md).
- agent.instructions: string | { file: string } (default: built-in) — prepended to agent prompts. Inline strings are used directly; file paths can be absolute or relative to the tasks directory.
- agent.worktree.enabled: boolean (default false) — when true, agent jobs run inside a git worktree.
- agent.worktree.dir: string (default computed) — optional worktree root directory; relative paths resolve from the repo’s parent directory.
- agent.worktree.branch_prefix: string (default "agent/") — prefix used when creating agent branches.
- agent.worktree.max_parallel_jobs: optional positive integer (unset means unlimited)
- agent.worktree.cleanup_on_done / cleanup_on_failure / cleanup_on_cancel / cleanup_delete_branches: booleans (default false). Eligibility and clean/merged/active-job safeguards still apply; see [Agent Jobs](./agent.md).
- agent.worktree.cleanup_ignored_files: boolean (default true). When true, git-ignored files (build output) don't keep a worktree from being cleaned up; untracked and modified files always do. Set false to keep any worktree that holds ignored files.
- agent.automation.on_start / on_success / on_failure / on_cancel: legacy lifecycle maps that are still accepted but not applied. Job lifecycle changes (for example InProgress on start, NeedsReview on success) come from `job_started` / `job_completed` / `job_failed` / `job_cancelled` rules in the automation file; see [Automation Rules](./automation.md#agent-job-queueing).
- agents: map (named agent CLI profiles; values may be a runner string or a full object)
  - agents.<name>.runner: `claude` | `codex` | `copilot` | `gemini` | `cursor` | `amp` | `droid` | `opencode` | `kilo` | `antigravity` | `command`
  - agents.<name>.permissions: `read-only` | `workspace` | `full` (default `workspace`) — what the agent may do without asking; see [Agent Jobs](./agent.md#permissions)
  - agents.<name>.model / effort: optional strings — model and reasoning effort passed to the CLI; runs and automation actions can override them. See [Agent Jobs](./agent.md#model-and-effort)
  - agents.<name>.tools.allowed / tools.disallowed: string[] — extra tools to allow or deny (Claude, Copilot); see [Agent Jobs](./agent.md#tools-and-mcp-servers)
  - agents.<name>.mcp.allow: string[] — MCP servers whose tools need no approval (Claude, Copilot)
  - agents.<name>.command: optional string — the executable to run (default: the runner name; required for `command`)
  - agents.<name>.args: string[] — extra CLI arguments, after LoTaR's own flags
  - agents.<name>.env: map — extra environment variables
  - `agents` can be set as a whole with `lotar config set agents '<json or yaml map>'` (project or `--global`); names may use letters, digits, `-`, `_` and `.`, and each runner must be one of the runners in [Agent Jobs](./agent.md#runners).
  - agents.<name>.mcp.lotar: boolean (default false) — give the job LoTaR's own MCP server, limited to ticket tools (claude, copilot and codex)
  - agents.<name>.max_budget_usd: number — stop a Claude run once it has cost this much (`--max-budget-usd`); ignored by other runners
  - agents.<name>.instructions: string | { file: string } — added after `agent.instructions` (or the built-in instructions) for this profile
- scan.signal_words: string[] (default: [TODO, FIXME, HACK, BUG, NOTE])
- scan.ticket_patterns: string[] (regex patterns to detect ticket keys)
- scan.enable_ticket_words: boolean (built-in default: true) — issue-type words act as signal words in addition to TODO/FIXME/etc. Bare ticket keys alone never create tasks.
- scan.enable_mentions: boolean (default: true) — when true, add code anchors under `references` for existing ticket keys found in source
- scan.strip_attributes: boolean (default: true) — when true, remove inline [key=value] attribute blocks from source after inserting the ticket key
- auto.identity: boolean (default true)
- auto.identity_git: boolean (default true)
- auto.set_reporter: boolean (default true)
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
- ui.theme: optional global-only default web UI theme, written `skin` or `skin:palette`. Skins and palettes (first = default): `classic` (lotar, ocean, forest, ember, violet, rose), `contrast`, `terminal` (amber, green, red, cyan, white), `matrix` (green, red, blue), `synthwave` (sunset, outrun, ice), `blueprint` (blue, green, sepia), `paper` (blue, black, red, green), `retro` (teal, plum, slate, desert), `candy` (bubblegum, mint, grape), and `custom` for the repository skin below. The ids `lotar`, `ocean`, `forest`, `ember`, `violet` and `rose` from before palettes still work (`classic:<id>`). Set it in the repository's config for everyone, or in your own `~/.lotar` (which wins) for yourself; people can still pick a theme per browser in Preferences
- ui.theme_file: optional global-only repository skin: a `.css` file (path relative to the repository) that sets theme variables, e.g. `:root { --color-accent: #c0392b; --font-heading: Georgia, serif; }` with dark-mode values under any selector or `@media (prefers-color-scheme: dark)` block mentioning `dark`. Only custom properties are used (no `url()`, `@import` or other rules); they apply when `ui.theme` (or a person's choice) is `custom`. The variable names are the ones in `view/styles.css` and `view/themes.css`
- ui.name: optional global-only name shown next to the logo and as the browser tab title (at most 40 characters)
- ui.logo / ui.logo_dark / ui.icon: optional global-only images for the header logo, its dark-mode version, and the browser tab icon. Paths are relative to the repository (the folder holding the tasks directory), e.g. `assets/app-icon.svg`; files outside it, symlinks out of it, and files that are not SVG/PNG/JPEG/WebP/GIF/ICO/AVIF images are never served. Images uploaded in Config → Appearance are stored in `.tasks/@branding/`
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
- auth_profiles.<name>.method: string
- auth_profiles.<name>.token_env: string (env var name or literal token)
- auth_profiles.<name>.email_env: string (env var name or literal email)
- auth_profiles.<name>.base_url: string
- auth_profiles.<name>.api_url: string

## Home and Project keys
Home/global/project schemas are not identical. Project values override inherited fields for that project; `project.name` is an optional label, not the folder identifier. `server.port`, `default.project`, sprint defaults, `web_ui_path` and `ui.*` are global-only; sprint notifications and the listed project automation flags can be overridden. See [config scope](./config.md) before copying fields between files.

Auth profiles are home-only: `auth_profiles` is ignored in project configs (secrets never live in the repo). Remotes can be configured globally or per-project; project remotes override/extend global remotes.

Project configs can also override any automation toggle inherited from global defaults. Supported keys: `auto.populate_members`, `auto.set_reporter`, `auto.assign_on_status`, `auto.codeowners_assign`, `auto.tags_from_path`, `auto.branch_infer_type`, `auto.branch_infer_status`, `auto.branch_infer_priority`, `auto.identity`, and `auto.identity_git`.

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

The remote example is configuration only. Before a real sync, configure its home auth profile as documented in [Sync](../developers/sync.md); validation of YAML is not a credential or connectivity check.

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
