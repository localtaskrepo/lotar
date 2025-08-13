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

# Set configuration with validation preview
lotar config set default_priority HIGH --project=backend --dry-run

# Set global configuration
lotar config set server_port 9000 --global

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
lotar config init --project=backend --template=agile

# Set default assignee for project
lotar config set default_assignee john.doe@company.com --project=backend

# Set global server port
lotar config set server_port 8080

# List available templates
lotar config templates
```

## Configuration Keys

### Project-Level
- `project_name` - Project name/identifier
- `issue_states` - Available task statuses
- `issue_types` - Available task types  
- `issue_priorities` - Available priorities
- `categories` - Available categories
- `tags` - Available tags
- `default_assignee` - Default task assignee
- `default_reporter` - Default task reporter (also used for auto-assign resolution)
- `default_priority` - Default task priority
- `default_status` - Default task status
- `custom_fields` - Custom field definitions
- `auto_set_reporter` - If true, set reporter automatically on create/update when missing
- `auto_assign_on_status` - If true, auto-assign assignee on first meaningful status change
    - First-change is defined as: when a task moves away from the default_status (or the first state if default unset) and the task currently has no assignee.
    - The assignee chosen is the resolved current user (see Identity Resolution below).

### Global
- `server_port` - Web server port
- `default_project` - Default project prefix
- `issue_states` - Default task statuses for all projects
- `issue_types` - Default task types for all projects
- `issue_priorities` - Default priorities for all projects
- `categories` - Default categories for all projects
- `tags` - Default tags for all projects
- `default_assignee` - Default task assignee for all projects
- `default_reporter` - Default task reporter for all projects (also used for auto-assign resolution)
- `default_priority` - Default task priority for all projects
- `default_status` - Default task status for all projects
- `custom_fields` - Default custom fields for all projects
- `auto_set_reporter` - If true, set reporter automatically on create/update when missing
- `auto_assign_on_status` - If true, auto-assign assignee on first meaningful status change

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
- `LOTAR_PROJECT` - Default project name; mapped to a prefix and applied as `default_project`
- `LOTAR_DEFAULT_ASSIGNEE` - Default assignee for all new tasks
- `LOTAR_DEFAULT_REPORTER` - Default reporter identity used for auto reporter/assign

Notes:
- Auto reporter and auto-assign behavior is controlled by configuration: set `default_reporter`, `auto_set_reporter`, and `auto_assign_on_status` in config files. The above environment variables can provide defaults, but do not toggle automation flags.
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
