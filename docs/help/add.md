# lotar add

Create new tasks with smart defaults and validation.

## Usage

```bash
lotar add "Task title" [OPTIONS]
```

## Quick Examples

```bash
# Basic task
lotar add "Implement user authentication"

# Feature with details
lotar add "Add OAuth support" --type=feature -P high --reporter=lead@example.com --assignee=john.doe

# Bug with due date
lotar add "Fix login crash" --type=bug -P high --due=tomorrow

# Start in a different status
lotar add "Review the login copy" --status=NeedsReview

# Feature with custom fields
lotar add "User Management System" --type=feature --field=story_points=13 --field=sprint=2

# Custom tasks directory
lotar add "External task" --tasks-dir=/external/projects/tasks --project=external

# Environment variable usage
export LOTAR_TASKS_DIR=/project/tasks
lotar add "Environment task" --project=myapp  # Uses environment directory
```

## Options

### Core Properties
- `--type <TYPE>`, `-t <TYPE>` - Task type, one of the project's configured types (default config: Feature, Bug, Chore)
- `--priority <PRIORITY>`, `-P <PRIORITY>` - Priority, one of the configured priorities (default config: Low, Medium, High)
- `--status <STATUS>` - Initial status, one of the configured states (default config: Todo, InProgress, NeedsReview, Done). Without it the task starts in the default status.
- `--reporter <REPORTER>`, `-R <REPORTER>` - Reporter identity (email or @username). Supports `@me` to resolve to your identity.
- `--assignee <ASSIGNEE>`, `-a <ASSIGNEE>` - Task assignee (email or @username). Supports `@me` to resolve to your identity.

### Scheduling
- `--due <DATE>` - Due date (YYYY-MM-DD or relative like 'tomorrow')
- `--effort <ESTIMATE>`, `-E <ESTIMATE>` - Effort estimate (e.g., 2d, 5h, 1w)

### Organization
- `--tag <TAG>`, `-i <TAG>` - Tags (can be used multiple times)
- `--description <TEXT>`, `-D <TEXT>` - Detailed description

Types and priorities are whatever your configuration defines, so there are no shortcuts such as `--bug` or `--high`: use `--type bug` or `--priority high` (matched case-insensitively against the configured values).

### Planning and diagnostics
- `--dry-run` - Preview the task that would be created without writing
- `--explain` - Show where the project came from and how defaults (status/priority/reporter) were chosen
	- Includes project detection source, whether a path-derived tag was added, and any branch-based inference (type/status/priority) with alias hits

### Custom Fields
- `--field <KEY>=<VALUE>` - Arbitrary properties (can be used multiple times)
- `--field=sprint=N` stores a custom field named `sprint`; it does not assign the task to a sprint. Use `lotar sprint add --sprint N TASK_ID` for actual membership (see [Sprints](./sprints.md)).
- A [version field](./config.md#version-fields) takes versions only: `--field affects=0.13.3`, or a list `--field affects=0.13.2,0.13.3`. A leading `v` is dropped; anything else that is not a version is an error.

### Report a problem once
- `--finding <KEY>` - A stable key for the problem (one line, at most 200 characters), for agents and scripts that check repositories and report what they find. See [Findings](#findings).

### Global Options
- `--format <FORMAT>` - Output format: text or json
- `--verbose` - Enable verbose output
- `--project <PROJECT>`, `-p <PROJECT>` - Target project (overrides auto-detection)
- `--tasks-dir <PATH>` - Custom tasks directory (overrides environment/config)

### Environment Variables
- `LOTAR_TASKS_DIR` - Default tasks directory location  
- `LOTAR_DEFAULT_ASSIGNEE` - Default assignee for new tasks
- `LOTAR_DEFAULT_REPORTER` - Default reporter identity when not provided

Reporter auto-set is driven by configuration: set `default_reporter` and ensure `auto.set_reporter: true` (default). The environment variable can provide a default reporter value.

Providing `--reporter` overrides all defaults (including `LOTAR_DEFAULT_REPORTER`) after validation.

Notes:
- `@me` resolution order: config.default_reporter (merged with precedence) → git user.name/email → $USER/$USERNAME.
- When `--assignee=@me` is provided, it is resolved at the CLI layer so previews and persisted values match.
 - See also: [Resolution & Precedence](./precedence.md).

## Output Formats

Control output with global `--format` option:

```bash
# Human-readable (default)
lotar add "New task" 
# 📋 Created task AUTH-001: New task

# With server.public_url set, the link follows: "  Link: https://lotar.example.com/task/AUTH-001"

# JSON for scripting
lotar add "New task" --format=json
# {"status":"success","message":"Created task: AUTH-001","task":{"id":"AUTH-001", ...}}
# (with server.public_url set, "task" also has "url")

# JSON dry-run preview
lotar add "Preview task" --dry-run --format=json
# {"status":"preview","action":"create","project":"AUTH","title":"Preview task","priority":"MEDIUM","status_value":"TODO"}
```

## Findings

A check that runs again and again (leftover worktrees, a red CI run, a stale dependency) should report each problem once. Give it a key that stays the same for the same problem:

```bash
lotar add "Leftover worktrees in lotar" --finding worktrees:lotar -D "3 worktrees have no running job" --format json
```

The newest ticket of the project with that key decides what happens. The command exits 0 in every case and names the ticket:

| Newest ticket with the key | What happens | JSON |
| --- | --- | --- |
| none | a new ticket | `{"created": true, "state": "new", "id": "DEV-12"}` |
| open (any status that isn't done) | no new ticket; the report is added to it as a comment with the reporter, date and description. A report with nothing new (no description, or the ticket's description, or the text of the last report comment) adds no comment; it sets the ticket's `finding_seen` field to today's date | `{"created": false, "state": "open", "id": "DEV-12"}` |
| won't do (`WontDo`, `Won't fix`, `Cancelled`, `Rejected` or `Declined`) | nothing; the problem was accepted | `{"created": false, "state": "suppressed", "id": "DEV-12"}` |
| done | a new ticket, related to the done one | `{"created": true, "state": "recurred", "id": "DEV-15"}` |

- The JSON also has the usual `status`, `message`, `task_id` and `task` (the new ticket, or the one that already reports the problem). Text output names the ticket in its first line.
- The key is stored in the ticket's `finding` field (also in projects that limit custom fields), so `lotar list finding=worktrees:lotar` lists the tickets that reported it.
- `finding_seen` (`YYYY-MM-DD`) is set without a history entry and leaves `modified` alone, so a daily check doesn't make the ticket look edited every day. `lotar list finding_seen=2026-11-11` lists what a check saw that day.
- Checking and creating hold the project's finding lock (`.finding.lock`), so two agents reporting the same key at the same moment create one ticket.
- Automation rules on `created` run only when a ticket is created; the comment on an open ticket runs `commented` rules.
- REST (`/api/tasks/add`) and MCP (`task_create`) take the same key as `finding`; see [API quick reference](api-quick-reference.md) and [MCP tools](mcp-tools.md#task_create).
- Done means the project's done states (`issue.done_states`, or the inferred ones: see [Done States](config.md#done-states)). A won't-do state is recognized by its name, done or not.

## Validation

All task properties are validated against project configuration:

- **Task Type**: Must be in configured `issue_types` list
- **Priority**: Must be in configured `issue_priorities` list  
- **Status**: Defaults to first state in `issue_states`
- **Tags**: Validated against `tags` config
- **Custom Fields**: Validated for format and allowed values

Task creation and any auto-populated project members are committed together through the coordinated task transaction. A failed task write does not leave a member/configuration update behind. `--dry-run` performs validation without writing either the task or project configuration.

Configure validation rules with:
```bash
lotar config set issue.types feature,bug,chore,epic --global
lotar config set issue.priorities low,medium,high,critical --global
```

`--global` changes `.tasks/config.yml` for every project; use `--project=<PREFIX>` to change one project.

## Project Resolution

Tasks are created in the appropriate project:

1. **Explicit project**: `--project=myproject` or `-p myproject`
2. **Task ID prefix**: If title starts with "PROJ-", uses PROJ project  
3. **Default project**: `default.project` in the configuration, or the `LOTAR_PROJECT` / `LOTAR_DEFAULT_PROJECT` environment variable. A manifest name never overrides it.
4. **No default project set**: the existing project (the first one alphabetically; a folder with a `config.yml` or task files), else a name detected from the checkout:
	 - Nearest project manifest walking up to the repo root:
		 - `package.json` name (scope stripped, e.g. `@scope/api` -> `api`)
		 - `Cargo.toml` `[package].name`
		 - `go.mod` module last path segment
	 - Git repo name (folder name at repo root)
	 - Current directory name

	 A project picked by detection that doesn't exist yet prints a warning with the prefix it creates (also on `--dry-run`), and the first real `add` saves that prefix as `default.project`.

`--explain` names where the project came from, for example `project G from default.project in the configuration`; JSON dry runs also return it as `project_source`.

Auto-detection walks upward but stops at the repository root (supports `.git` directory or file for worktrees/submodules).

Branch inference reads the current checkout's `HEAD`, including linked worktrees whose `.git` file points to an administration directory. Relative `gitdir` paths are resolved from the checkout, and relative `commondir` paths from that administration directory. A detached, missing, or malformed `HEAD` does not provide a branch inference value; configured defaults and explicit task options still apply. These metadata reads do not invoke Git commands or change repository files.

### Auto Tags from Monorepo Paths

When you don't pass any `--tag` and your config has no `default.tags`, LoTaR will try to derive a single tag from common monorepo structures:

- Recognized folders: `packages/<name>`, `apps/<name>`, `libs/<name>`, `services/<name>`, `examples/<name>`
- Hidden names are ignored (e.g. `.hidden`)
- The derived tag is validated against your configured `tags` list; if invalid, it's skipped

Examples:
```bash
# In repo/packages/api
lotar add "Implement endpoint"             # auto-adds --tag=api if allowed

# In repo/apps/web, with no configured 'web' tag
lotar add "Style header"                   # does NOT add tag (fails validation)
```

Configuration toggle:
- Set `auto.tags_from_path: false` in `.tasks/config.yml` (global) or in a project config to disable this behavior entirely.
	- Default is `true`.

### Branch-aware defaults

When no `--type` is provided, LoTaR will try to infer a task type from the current git branch name:

- `feat/*` or `feature/*` → type = Feature
- `fix/*`, `bugfix/*`, `hotfix/*` → type = Bug
- `chore/*`, `docs/*`, `refactor/*`, `test/*`, `perf/*` → type = Chore

Notes:
- In addition to conventional prefixes, LoTaR also matches configured type names against the branch prefix: if your config lists a type like `Research`, a branch like `research-123` or `research/123` (case-insensitive; supports `-`, `_`, `/`) will infer that type when allowed by your config.
- If the inferred type isn’t allowed by your config’s `issue.types`, it’s ignored and the default type resolved from your configuration (following precedence) is used.
- Inference only runs inside a git repository with a readable HEAD; otherwise default behavior applies.

Configuration toggle:
- Set `auto.branch_infer_type: false` to disable branch-based type inference.
	- Default is `true`.

Status and priority can also be inferred using branch alias maps when enabled. LoTaR inspects the first segment of the branch (before the first `/`) and looks it up in configured alias maps:

- `branch.status_aliases`: Maps a token to a status from `issue.states` (e.g., `wip: InProgress`)
- `branch.priority_aliases`: Maps a token to a priority from `issue.priorities` (e.g., `hotfix: Critical`)

Behavior and toggles:
- Set `auto.branch_infer_status: true` to enable status inference from `branch.status_aliases`.
- Set `auto.branch_infer_priority: true` to enable priority inference from `branch.priority_aliases`.
- Project-level alias maps override/extend global ones; keys are matched case-insensitively (normalized to lowercase).
- Inference only applies when the inferred value is valid per your config; otherwise the normal defaults apply.

See `lotar config help` for how to configure alias maps.

## Examples by Use Case

### Development Team
```bash
# Feature development
lotar add "Implement dark mode" --type=feature --assignee=alice@company.com --due=2025-08-15

# Bug reporting
lotar add "Login page crashes on mobile" --type=bug --priority=high --tag=mobile --tag=urgent

# Technical debt
lotar add "Refactor authentication module" --type=chore --effort=3d
```

### Project Management
```bash
# Larger planned work (add an Epic type first if you want one: lotar config set issue.types feature,bug,chore,epic --global)
lotar add "User Management System" --type=feature --field=story_points=21 --field=quarter=Q3

# Custom sprint label (not sprint membership)
lotar add "Design user profile page" --field=sprint=15 --field=team=frontend

# Assign the returned task ID to an actual sprint
lotar sprint add --sprint 15 TASK_ID
```

### Personal Productivity
```bash
# Simple todo
lotar add "Review pull requests"

# With deadline
lotar add "Prepare presentation slides" --due=friday --effort=2h
```
