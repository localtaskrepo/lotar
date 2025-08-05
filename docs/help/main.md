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
```

## Global Options

- `--format <FORMAT>` - Output format: text, table, json, markdown (default: text)
- `--verbose` - Enable verbose output
- `--project <PROJECT>` - Specify project context

## Commands

- **add** - Create new tasks with validation
- **list** - Display tasks with filtering
- **status** - Change task status with validation  
- **set** - Update task properties
- **task** - Full task management (legacy interface)
- **config** - Comprehensive project configuration with templates, validation, and dry-run
- **scan** - Find TODO comments in code
- **serve** - Start web interface
- **index** - Manage search indexes

Use `lotar help <command>` for detailed command information.

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

## Getting Help

- `lotar help` - Show this overview
- `lotar help <command>` - Detailed command help
- `lotar <command> --help` - Quick command options
