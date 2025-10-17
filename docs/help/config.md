# lotar config

Manage project and global configuration settings with comprehensive features.

## Usage

```bash
lotar config <ACTION> [OPTIONS]
```

## Actions

### show
Display current configuration.

```bash
lotar config show [--project=PROJECT] [--explain]
```
Options:
- `--explain` — Annotate where values come from (env, home, global, project, default).
 - When `--format=json` is used, an additional structured explanation object is emitted with a `sources` map per key.

### init
Initialize project configuration from template with advanced options.

```bash
lotar config init [--project=PROJECT] [--template=TEMPLATE] [--prefix=PREFIX] 
                  [--copy-from=SOURCE_PROJECT] [--global] [--dry-run] [--force]
```

### set
Set configuration values with validation and conflict detection.

```bash
lotar config set <KEY> <VALUE> [--project=PROJECT] [--global] [--dry-run] [--force]
```

### templates
List available configuration templates.

```bash
lotar config templates
```

### validate
Validate configuration files for errors and warnings.

```bash
lotar config validate [--project=PROJECT] [--global] [--fix] [--errors-only]
```

### normalize
Rewrite config files into the canonical nested YAML form (supports dotted keys on input).

```bash
lotar config normalize [--global] [--project=PREFIX] [--write]
```
Options:
- Without --write, prints the normalized form (dry-run) and does not modify files.
- When no scope is provided, normalizes the global config and all project configs.

## Examples

```bash
# Show global configuration
lotar config show

# Show project-specific configuration
lotar config show --project=backend

# Show configuration with custom tasks directory
lotar config show --tasks-dir=/custom/path

# Initialize new project with agile template
lotar config init --project=backend --template=agile

# Preview config initialization (dry-run)
lotar config init --project=my-awesome-project --template=default --dry-run

# Initialize with custom prefix
lotar config init --project="Long Project Name" --prefix=LPN --template=kanban

# Copy configuration from another project
lotar config init --project=frontend --copy-from=backend --template=agile

# Force overwrite existing configuration
lotar config init --project=backend --template=simple --force

# Set configuration with validation preview (use dotted canonical keys)
lotar config set default.priority HIGH --project=backend --dry-run

# Set global configuration (server port)
lotar config set server.port 9000 --global

# Set global tags and custom fields for all projects (arrays)
lotar config set issue.tags '["frontend","backend","urgent"]' --global
lotar config set custom.fields '["product","sprint"]' --global

# Environment variable integration
export LOTAR_TASKS_DIR=/custom/tasks
lotar config show  # Shows environment-configured directory

# List available templates
lotar config templates

# Validate global configuration
lotar config validate --global

# Validate specific project configuration
lotar config validate --project=backend

# Show only errors, not warnings
lotar config validate --global --errors-only

# Validate and attempt auto-fixes
lotar config validate --project=my-project --fix

# Preview canonical nested config for all files (no writes)
lotar config normalize

# Normalize a single project and write back
lotar config normalize --project=DEMO --write
```

## Advanced Features

### Configuration Precedence
When resolving configuration, LoTaR uses this order (highest wins):
1. Command-line flags (per command)
2. Environment variables
3. Home config (~/.lotar)
4. Project config (.tasks/<PROJECT>/config.yml)
5. Global config (.tasks/config.yml)
6. Built-in defaults

Notes:
- Project config overrides global, but home/env can override both.
- CLI flags are applied by each command and always win for that invocation.
 - Identity resolution uses the merged configuration from this precedence chain.

### Canonical YAML shape
LoTaR accepts both dotted keys and nested sections in YAML. Internally, values are canonicalized to a nested structure with these groups: server, default, issue, custom, scan, auto. Use `lotar config normalize` to rewrite files into this canonical form.

Notes:
- Automation flags use the `auto.*` namespace (e.g., `auto.identity`, `auto.identity_git`, `auto.set_reporter`, `auto.assign_on_status`, `auto.branch_infer_type`, `auto.branch_infer_status`, `auto.branch_infer_priority`).
- Legacy `taxonomy.categories` and `taxonomy.tags` are accepted on input for backward compatibility. They are normalized, but `issue.categories` is considered legacy and is no longer consumed directly—prefer using `custom.fields` instead.
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
The system checks for conflicts when changing configurations:
- Validates existing tasks against new config rules
- Warns about potential breaking changes
- Use `--force` to override validation warnings

### Configuration Copying
Copy settings between projects while preserving unique identifiers:
```bash
lotar config init --project=new-service --copy-from=existing-service
```

```

## Configuration Keys

> Legacy note: older configurations may still carry `issue.categories`. The value is normalized for backwards compatibility, but the runtime no longer uses it—model the same information with `custom.fields` instead.

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
- `custom.fields` - Custom field definitions
    
Automation (defaults inherited from global):
- `auto.set_reporter` - If true, set reporter automatically on create/update when missing
- `auto.assign_on_status` - If true, auto-assign assignee on first meaningful status change
    - First-change is defined as: when a task moves away from the default.status (or the first state if default unset) and the task currently has no assignee.
    - The assignee chosen is, in order: CODEOWNERS default owner (when available and `auto.codeowners_assign` is true) → resolved current user (see Identity Resolution below).

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
- `custom.fields` - Default custom fields for all projects
Automation:
- `auto.set_reporter` - Enable auto reporter when missing (default: true)
- `auto.assign_on_status` - Enable first-change auto-assign (default: true)
- `auto.identity` - Enable smart identity detection beyond configured default (default: true)
- `auto.identity_git` - Enable git-based identity detection (default: true)
 - `auto.codeowners_assign` - Prefer CODEOWNERS owner on first status change when task has no assignee (default: true)
 - `auto.tags_from_path` - Derive a tag from monorepo paths like packages/<name> when no tags provided and no defaults exist (default: true)
 - `auto.branch_infer_type` - Infer task type from branch name prefixes like feat/, fix/, chore/ (default: true)
 - `auto.branch_infer_status` - Infer status from `branch.status_aliases` using the first branch token (default: false)
 - `auto.branch_infer_priority` - Infer priority from `branch.priority_aliases` using the first branch token (default: false)

Branch alias maps (global- and project-level):
- `branch.type_aliases` - Map branch tokens to task types. Example: `{ feat: Feature, fix: Bug }`
- `branch.status_aliases` - Map branch tokens to statuses. Example: `{ wip: InProgress }`
- `branch.priority_aliases` - Map branch tokens to priorities. Example: `{ hotfix: Critical }`

Notes:
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
- `simple` - Minimal configuration

## Global Options

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
