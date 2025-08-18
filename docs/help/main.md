# LoTaR - Local Task Repository

Git-integrated task management with beautiful terminal output.

## Quick Start

```bash
# Add a task
lotar add "Implement OAuth authentication" --type=feature --priority=high

# List tasks
lotar list

# Change task status  
lotar status AUTH-001 done

# List with different output formats
lotar list --format=table
lotar list --format=json

# Use custom tasks directory
lotar add "Custom task" --tasks-dir=/custom/path
lotar list --tasks-dir=/custom/path

# Environment variable support
export LOTAR_TASKS_DIR=/project/tasks
lotar add "Environment task"  # Uses environment-configured directory
```

## Global Options

**Available on ALL commands:**
- `--format <FORMAT>` - Output format: text, table, json, markdown (default: text)
- `--verbose` - Enable verbose output
- `--project <PROJECT>`, `-p <PROJECT>` - Specify project context (overrides auto-detection)
- `--tasks-dir <PATH>` - Custom tasks directory (overrides all auto-detection)

## Environment Variables

- `LOTAR_TASKS_DIR` - Override default tasks directory location
- `LOTAR_DEFAULT_ASSIGNEE` - Set default assignee for all new tasks
- `LOTAR_DEFAULT_REPORTER` - Set default reporter identity used when auto-setting

## Commands

- **add** - Create new tasks with validation
- **list** - Display tasks with filtering and multiple output formats
- **status** - Change task status with validation  
- **priority** - Change task priority
- **assignee** - Change task assignee
- **due-date** - Manage task due dates
- **task** - Full task management (legacy interface)
- **config** - Comprehensive project configuration with templates, validation, and dry-run
- **scan** - Find TODO comments in code
- **serve** - Start web interface
- **mcp** - Run JSON-RPC server (tools for tasks/projects/config)
- **whoami** - Show resolved current user identity (with --explain)

Use `lotar help <command>` for detailed command information.

See also: [Resolution & Precedence](./precedence.md) for value sources and identity resolution.

## Output Formats

### Text (Default)
Human-readable with colors, emojis, and styling:
```
📋 Implement OAuth [feature] - HIGH (TODO) - 👤 john.doe
🚧 Fix login bug [bug] - CRITICAL (IN_PROGRESS) - 📅 2025-08-15
```

### Table
Clean tabular output:
```
ID       TITLE               STATUS      PRIORITY  ASSIGNEE
AUTH-001 Implement OAuth     TODO        HIGH      john.doe  
BUG-042  Fix login bug       IN_PROGRESS CRITICAL  jane.smith
```

### JSON  
Machine-readable for scripts and integrations:
```json
{
  "tasks": [
    {
      "id": "AUTH-001",
      "title": "Implement OAuth",
      "status": "TODO",
      "priority": "HIGH",
      "assignee": "john.doe"
    }
  ]
}
```

### Markdown
Documentation-friendly format:
```markdown
## Tasks

- [x] **AUTH-001**: Implement OAuth *(HIGH)* - @john.doe
- [ ] **BUG-042**: Fix login bug *(CRITICAL)* - @jane.smith
```

## Project Management

LoTaR automatically detects project context:
- Uses `.tasks` directory in project root
- Supports multiple projects in monorepos
- Git integration for change tracking
- Template-based configuration
- Environment variable support for custom locations

**Tasks Directory Resolution Order:**
1. `--tasks-dir <PATH>` command line flag (highest priority)
2. `LOTAR_TASKS_DIR` environment variable
3. Parent directory search for existing `.tasks` folder
4. Current directory `.tasks` folder (created if needed)

Configuration precedence (for config values): CLI > env > home > project > global > defaults

## Getting Help

- `lotar help` - Show this overview
- `lotar help <command>` - Detailed command help
- `lotar <command> --help` - Quick command options
