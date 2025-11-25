# lotar config

Manage project and global configuration settings with comprehensive features.

## Usage

```bash
lotar config <ACTION> [OPTIONS]
```

## Actions

### show
Display the effective configuration for the global scope (default) or a specific project.

```bash
lotar config show [--project=PREFIX_OR_NAME] [--explain] [--full]
```
Options:
- `--project` — Accepts a project prefix, project directory, or friendly name. If omitted, the command shows the merged global configuration and honors the CLI-wide `--project` flag when present.
- `--explain` — For text output, annotate each key with inline comments showing whether the value came from env, home, global, or project configuration. JSON output always includes a `sources` map regardless of this flag.
- `--full` — Emit the entire effective configuration (canonical YAML by default, JSON payload when `--format=json`). Without `--full`, only non-default values from the allowed sources are shown.

Notes:
- The handler always prints the resolved tasks directory path so you can confirm discovery.

### init
Initialize project configuration from template with advanced options.

> Tip: `lotar init` is equivalent to `lotar config init`.

```bash
lotar config init [--project=PROJECT] [--template=TEMPLATE] [--prefix=PREFIX] 
                  [--copy-from=SOURCE_PROJECT] [--global] [--dry-run] [--force]
```

Options:
- `--template` — Defaults to `default`. Valid templates are `default`, `agile`, and `kanban` (see `lotar config templates`).
- `--project` — Human-readable project name stored at `project.name`. When omitted, templates keep their placeholder and the folder prefix becomes the identifier.
- `--prefix` — Explicit project prefix (e.g., `WEB`). The handler validates collisions before writing. Without it, a unique prefix is auto-generated from the project name.
- `--copy-from` — Merge settings from an existing project before canonicalizing the template. Pass the project prefix/directory name (the handler reads `.tasks/<PREFIX>/config.yml`).
- `--global` — Initialize `.tasks/config.yml` instead of a project directory.
- `--dry-run` — Preview the prefix, target path, and any conflicts without writing files.
- `--force` — Overwrite existing config files (required if the target already exists).

### set
Set configuration values with validation and conflict detection.

```bash
lotar config set <KEY> <VALUE> [--global] [--dry-run] [--force]
```

Details:
- Use canonical keys such as `server.port`, `issue.tags`, or `custom_fields`. Arrays/maps accept JSON strings (e.g., `"[\"frontend\",\"backend\"]"`) or comma-separated values (e.g., `frontend,backend`).
- Project scope is chosen via the CLI-wide `--project` flag or the configured `default.project`. Pass `--global` to edit `.tasks/config.yml` instead.
- `--dry-run` shows the pending change, runs conflict detection, and exits without editing files.
- `--force` applies the change even when conflict detection finds issues. Validation errors (wrong field name/value) will still abort.
- Fields that only make sense globally (`server.port`, `default_project`) are automatically treated as global even if `--global` is not passed.

### templates
List available configuration templates.

```bash
lotar config templates
```

Prints the three built-in templates (`default`, `agile`, `kanban`) with short summaries so you can choose a name for `lotar config init --template=<name>`.

### validate
Validate configuration files for errors and warnings.

```bash
lotar config validate [--project=PREFIX] [--global] [--fix] [--errors-only]
```

Behavior:
- With no flags, only the global configuration is validated. Pass `--project=PREFIX` to check a specific project directory, or combine `--global` and `--project` to validate both scopes in one run.
- `--project` expects the canonical project prefix/folder name.
- `--errors-only` limits the report to validation errors (warnings stay hidden unless a JSON format consumer inspects them separately).
- `--fix` is reserved for future automated remediation. Today it only prints guidance when errors are present—you must still edit files manually.

### normalize
Rewrite config files into the canonical nested YAML form (supports dotted keys on input).

```bash
lotar config normalize [--global] [--project=PREFIX] [--write]
```
Options:
- Without `--write`, prints the canonical YAML for each target and leaves files untouched.
- With `--write`, rewrites every targeted config and reports how many files changed.
- `--project` expects the project prefix/directory name. When omitted (and `--global` is also omitted) the command normalizes the global config plus every project directory that already contains a config file.
- Passing `--global` alone only touches `.tasks/config.yml` if it exists.

## Examples

```bash
# Show global configuration
lotar config show

# Show project-specific configuration (prefix or friendly name)
lotar config show --project=backend

# Show the full effective configuration in canonical YAML
lotar config show --full

# Show configuration with custom tasks directory
lotar config show --tasks-dir=/custom/path

# Initialize new project with agile template
lotar config init --project=backend --template=agile

# Preview config initialization (dry-run)
lotar config init --project=my-awesome-project --template=default --dry-run

# Initialize with custom prefix
lotar config init --project="Long Project Name" --prefix=LPN --template=kanban

# Copy configuration from another project (use its prefix)
lotar config init --project=frontend --copy-from=BACK --template=agile

# Force overwrite existing configuration
lotar config init --project=backend --template=default --force

# Set a project-level value with validation preview (use dotted canonical keys and the global --project flag)
lotar --project=BACK config set default.priority HIGH --dry-run

# Set global configuration (server port)
lotar config set server.port 9000 --global

# Set global tags and custom fields for all projects (arrays)
lotar config set issue.tags '["frontend","backend","urgent"]' --global
lotar config set custom_fields '["product","sprint"]' --global
# Comma-separated list works too
lotar config set custom_fields product,sprint --global

# Environment variable integration
export LOTAR_TASKS_DIR=/custom/tasks
lotar config show  # Shows environment-configured directory

# List available templates
lotar config templates

# Validate global configuration
lotar config validate --global

# Validate specific project configuration (pass the prefix)
lotar config validate --project=BACK

# Show only errors, not warnings
lotar config validate --global --errors-only

# Request fix suggestions (currently prints guidance only)
lotar config validate --project=DEMO --fix

# Preview canonical nested config for all files (no writes)
lotar config normalize

# Normalize a single project and write back
lotar config normalize --project=DEMO --write
```

## Advanced Features

### Configuration Precedence
When resolving configuration, LoTaR uses this order (highest wins):
1. Command-line flags (per command)
2. Project config (.tasks/<PROJECT>/config.yml) when a project context is resolved
3. Environment variables
4. Home config (~/.lotar)
5. Global config (.tasks/config.yml)
6. Built-in defaults

Commands that do not operate on a specific project simply skip step 2, so they evaluate CLI → env → home → global → defaults.

Notes:
- Project config overrides env/home/global, so use env/home for broad defaults and commit per-project differences inside `.tasks/<PROJECT>/config.yml`.
- CLI flags are applied by each command and always win for that invocation.
 - Identity resolution uses the merged configuration from this precedence chain.

### Canonical YAML shape
LoTaR accepts both dotted keys and nested sections in YAML. Internally, values are canonicalized to sections such as server, default, project, members, issue, custom, scan, auto, sprints, and branch. Use `lotar config normalize` to rewrite files into this canonical form.

- When a config file only uses built-in defaults the canonical writer produces a short comment instead of redundant YAML. Modify a value (for example `lotar config set server.port 9000 --global`) and rerun `lotar config normalize --write` to emit the corresponding section.
- Automation flags use the `auto.*` namespace (e.g., `auto.identity`, `auto.identity_git`, `auto.set_reporter`, `auto.assign_on_status`, `auto.branch_infer_type`, `auto.branch_infer_status`, `auto.branch_infer_priority`).
- Legacy `taxonomy.categories` and `taxonomy.tags` are accepted on input for backward compatibility. They are normalized, but `issue.categories` is considered legacy and is no longer consumed directly—prefer using `custom_fields` instead.
- Branch alias maps live under a top-level `branch` section and are merged with project-level overrides.

### Automatic Prefix Generation
Projects automatically get prefixes generated from their names:
- Short names (≤4 chars): Use as-is (`test` → `TEST`)
- Hyphenated names: First letters (`my-awesome-project` → `MAP`)
- Underscored names: First letters (`my_cool_app` → `MCA`)
- Long names: First 4 characters (`longprojectname` → `LONG`)

### Dry-Run Mode
Use `--dry-run` to preview changes without applying them:
```bash
lotar config init --project=TestProject --dry-run
# Output: Would create .tasks/TEST/config.yml
```

### Validation & Conflict Detection
`lotar config set --dry-run` runs the same validation pipeline as a real write, including conflict detection against existing tasks. If the preview reports problems you can either fix the underlying data or re-run with `--force` to apply anyway (schema/type validation still runs and may block the change).

`lotar config validate` surfaces the complete error and warning sets for global and project scopes. The `--fix` flag is a placeholder today—the handler prints guidance but no automated rewrite occurs yet.

Validation now enforces that defaults and branch alias maps reference values that exist in the corresponding `issue.*` lists. When a mismatch is detected the CLI prints a remediation hint with concrete commands so you can adjust the list or clear the offending aliases right away.

Example:

```bash
lotar config validate --project=ENG
ℹ️  Project Config (ENG) Validation Results:
❌ branch_status_aliases: Unknown status alias target 'QA'
ℹ️  Hint: realign aliases with `lotar config set branch_status_aliases 'qa:InProgress' --project=ENG` or clear them via `lotar config set branch_status_aliases '' --project=ENG`
```

Similar hints are emitted for `default.priority`/`issue.priorities` and `default.status`/`issue.states` mismatches to highlight either adjusting the allowed list (`lotar config set issue_priorities 'Low,Medium,High' --project=ENG`) or changing the default value.

### Configuration Copying
Copy settings between projects while preserving unique identifiers:
```bash
lotar config init --project=new-service --copy-from=existing-service
```

```

## Configuration Keys

> Legacy note: older configurations may still carry `issue.categories`. The value is normalized for backwards compatibility, but the runtime no longer uses it—model the same information with `custom_fields` instead (the older `custom.fields` alias remains accepted for compatibility).

### Project-Level
- `project.name` - Optional human-readable project name; folder name remains the canonical identifier
- `issue.states` - Available task statuses
- `issue.types` - Available task types  
- `issue.priorities` - Available priorities
- `issue.tags` - Available tags
- `default.assignee` - Default task assignee
- `default.reporter` - Default task reporter (also used for auto-assign resolution)
- `default.tags` - Default tags for new tasks (applied when no tags provided)
- `default.priority` - Default task priority
- `default.status` - Default task status
- `members` - Allowed members when `default.strict_members` is true
- `default.strict_members` - Enforce member list for reporter/assignee fields
- `custom_fields` - Custom field definitions
    
Automation (defaults inherited from global):
- `auto.populate_members` - Automatically add new assignees/reporters to the project member list when strict members are enabled (default: true)
- `auto.set_reporter` - If true, set reporter automatically on create/update when missing
- `auto.assign_on_status` - If true, auto-assign assignee on first meaningful status change
    - First-change is defined as: when a task moves away from the default.status (or the first state if default unset) and the task currently has no assignee.
    - The assignee chosen is, in order: CODEOWNERS default owner (when available and `auto.codeowners_assign` is true) → resolved current user (see Identity Resolution below).
- `auto.codeowners_assign` - Enable CODEOWNERS fallback when auto-assigning on first status change
- `auto.tags_from_path` - Allow monorepo path heuristics to seed tags when no defaults were provided
- `auto.branch_infer_type` - Infer task type from branch prefixes like feat/, fix/, chore/
- `auto.branch_infer_status` - Infer task status via `branch.status_aliases`
- `auto.branch_infer_priority` - Infer task priority via `branch.priority_aliases`
- `auto.identity` - Enable manifest/git/system detection for `@me`
- `auto.identity_git` - Allow git config to feed `@me` resolution

### Global
- `server.port` - Web server port
- `default.project` - Default project prefix
- `issue.states` - Default task statuses for all projects
- `issue.types` - Default task types for all projects
- `issue.priorities` - Default priorities for all projects
- `issue.tags` - Default tags for all projects
- `default.assignee` - Default task assignee for all projects
- `default.reporter` - Default task reporter for all projects (also used for auto-assign resolution)
- `default.tags` - Global default tags for new tasks
- `default.priority` - Default task priority for all projects
- `default.status` - Default task status for all projects
- `members` - Global allowed members list (project overrides replace/extend)
- `default.strict_members` - Enforce member list globally (projects can override)
- `custom_fields` - Default custom fields for all projects
- `sprints.defaults.length` - Default planned sprint length (e.g., `2w`) applied when creating sprints without an explicit length.
- `sprints.defaults.capacity_points` / `sprints.defaults.capacity_hours` - Default sprint capacity values used when not provided by the caller.
- `sprints.defaults.overdue_after` - Default grace period (e.g., `12h`) before overdue warnings trigger.
- `sprints.notifications.enabled` - Toggle lifecycle warnings for start/close operations (default: true). Accepts the same precedence chain as other config keys and can be overridden per project.
- `scan.signal_words` - Default scanner keywords (default: `["TODO","FIXME","HACK","BUG","NOTE"]`)
- `scan.ticket_patterns` - Regex patterns used globally for ticket detection
- `scan.enable_ticket_words` - Promote task type words (e.g., "Feature") to signal words (default: false)
- `scan.enable_mentions` - Emit references for existing ticket keys (default: true)
- `scan.strip_attributes` - Remove inline attribute blocks after insertion (default: true)
- Automation:
    - `auto.set_reporter` - Enable auto reporter when missing (default: true)
    - `auto.assign_on_status` - Enable first-change auto-assign (default: true)
    - `auto.identity` - Enable smart identity detection beyond configured default (default: true)
    - `auto.identity_git` - Enable git-based identity detection (default: true)
    - `auto.codeowners_assign` - Prefer CODEOWNERS owner on first status change when task has no assignee (default: true)
    - `auto.tags_from_path` - Derive a tag from monorepo paths like packages/<name> when no tags provided and no defaults exist (default: true)
    - `auto.branch_infer_type` - Infer task type from branch name prefixes like feat/, fix/, chore/ (default: true)
    - `auto.branch_infer_status` - Infer status from `branch.status_aliases` using the first branch token (default: true)
    - `auto.branch_infer_priority` - Infer priority from `branch.priority_aliases` using the first branch token (default: true)
    - `auto.populate_members` - Append new assignees/reporters to the configured members list when strict members is enabled (default: true)

Branch alias maps (global- and project-level):
- `branch.type_aliases` - Map branch tokens to task types. Example: `{ feat: Feature, fix: Bug }`
- `branch.status_aliases` - Map branch tokens to statuses. Example: `{ wip: InProgress }`
- `branch.priority_aliases` - Map branch tokens to priorities. Example: `{ hotfix: Critical }`

Notes:
- When `default.strict_members` is true, reporter and assignee must be listed in `members`. With `auto.populate_members` enabled (default), LoTaR automatically appends new identities to the project member list before validation.
- Keys are matched case-insensitively (normalized to lowercase during parse/merge).
- Project-level maps override/add to global maps. On conflict, project wins.
- Aliases must map to values present in the project's `issue.*` lists; invalid entries are ignored at use time.

Canonical YAML example:

```yaml
issue:
    states: [Todo, InProgress, Done]
    types: [Feature, Bug, Chore]
    priorities: [Low, Medium, High, Critical]
auto:
    branch_infer_type: true
    branch_infer_status: true
    branch_infer_priority: true
branch:
    type_aliases:
        feat: Feature
        fix: Bug
    status_aliases:
        wip: InProgress
    priority_aliases:
        hotfix: Critical
```

## Templates

- `default` - Basic task management
- `agile` - Agile/Scrum workflow
- `kanban` - Kanban board style

## Global Options

- `--project <PREFIX_OR_NAME>` - Override auto-detected project for any config subcommand (same resolution rules as `lotar config show --project`).
- `-C, --config KEY=VALUE` - Inline configuration override for the current invocation. Accepts the same field names as `lotar config set` (e.g., `-C default_status=Done`). Multiple flags can be supplied; later ones win. Existing per-command options remain as shorthands.
- `--format <FORMAT>` - Output format: text, table, json, markdown
- `--verbose` - Enable verbose output
- `--tasks-dir <PATH>` - Custom tasks directory (overrides environment/config)

## Environment Variables

- `LOTAR_TASKS_DIR` - Default tasks directory location (overrides discovery)
- `LOTAR_PORT` - Web server port override
- `LOTAR_PROJECT` - Default project name; mapped to a prefix and applied as `default.project`
- `LOTAR_DEFAULT_ASSIGNEE` - Default assignee for all new tasks
- `LOTAR_DEFAULT_REPORTER` - Default reporter identity used for auto reporter/assign

Notes:
- Auto reporter and auto-assign behavior are controlled by configuration: set `default_reporter` and (optionally) override `auto.set_reporter` or `auto.assign_on_status` in YAML. The above environment variables can provide defaults, but do not toggle automation flags.
- Diagnostic/testing variables (not for general use): `LOTAR_TEST_SILENT=1` silences warnings in tests; `LOTAR_VERBOSE=1` enables extra setup logs.

## Identity Resolution and @me

- Anywhere a person field is accepted (reporter, assignee, default_reporter), the special value `@me` is allowed.
- `@me` resolves to the current user using this order:
    1) Merged config `default_reporter` (following the precedence above)
    2) git config at repo root: user.name or user.email
    3) System environment: $USER or $USERNAME
- This resolution is used consistently by CLI, REST, and MCP.

## Notes

- Global settings apply to all projects
- Project settings override global defaults
- Templates provide pre-configured workflows
- Configuration is stored in YAML format
- Environment variables are respected in configuration display and operations
 - See also: [Resolution & Precedence](./precedence.md) for source order and identity rules.
