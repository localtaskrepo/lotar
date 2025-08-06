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
lotar config show [--project=PROJECT]
```

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
- `default_priority` - Default task priority
- `default_status` - Default task status
- `custom_fields` - Custom field definitions

### Global
- `server_port` - Web server port
- `default_project` - Default project prefix
- `issue_states` - Default task statuses for all projects
- `issue_types` - Default task types for all projects
- `issue_priorities` - Default priorities for all projects
- `categories` - Default categories for all projects
- `tags` - Default tags for all projects
- `default_assignee` - Default task assignee for all projects
- `default_priority` - Default task priority for all projects
- `default_status` - Default task status for all projects
- `custom_fields` - Default custom fields for all projects

## Templates

- `default` - Basic task management
- `agile` - Agile/Scrum workflow
- `kanban` - Kanban board style
- `simple` - Minimal configuration

## Notes

- Global settings apply to all projects
- Project settings override global defaults
- Templates provide pre-configured workflows
- Configuration is stored in YAML format
