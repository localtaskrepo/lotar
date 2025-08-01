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
- **config** - Manage project configuration
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
Structured data in terminal tables:
```
| Title              | Status      | Priority | Type    |
|--------------------|-------------|----------|---------|
| Implement OAuth    | TODO        | HIGH     | feature |
| Fix login bug      | IN_PROGRESS | CRITICAL | bug     |
```

### JSON
Machine-readable for scripting and automation:
```json
[
  {
    "title": "Implement OAuth",
    "status": "TODO",
    "priority": "HIGH",
    "task_type": "feature"
  }
]
```

### Markdown
Documentation-friendly format for reports:
```markdown
| Title | Status | Priority | Type |
|-------|--------|----------|------|
| Implement OAuth | TODO | HIGH | feature |
```

## Configuration

Tasks are validated against project configuration:
- **Issue States**: Valid status transitions
- **Issue Types**: Allowed task types
- **Priorities**: Available priority levels
- **Categories**: Project-specific categories
- **Tags**: Organizational tags

Configure with: `lotar config set issue_states TODO,IN_PROGRESS,DONE`
