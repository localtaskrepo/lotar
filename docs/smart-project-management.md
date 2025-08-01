# Smart Project Management Features

LoTaR includes intelligent project management capabilities that make working with multiple projects seamless and intuitive. These features automatically handle project naming, resolution, and configuration setup.

## 🧠 Smart Project Resolution

LoTaR automatically resolves between project names and prefixes, allowing you to use either format in any command.

### Bidirectional Name Resolution

You can use either the **full project name** or the **directory prefix** in any command:

```bash
# These commands are equivalent:
lotar task add --title="Fix bug" --project=FRONTEND    # Using full name
lotar task add --title="Fix bug" --project=FRON        # Using prefix

# Search works with both formats:
lotar task search "authentication" --project=AUTHENTICATION-SERVICE
lotar task search "authentication" --project=AUTH
```

### How It Works

When you specify a project, LoTaR intelligently determines whether you provided:

1. **Existing prefix** → Uses it directly
2. **Full project name** → Looks up the corresponding prefix from project config files
3. **New project name** → Generates a new prefix automatically

### Project Prefix Generation

For new projects, LoTaR generates smart 4-letter prefixes:

| Project Name | Generated Prefix | Logic |
|-------------|------------------|-------|
| `frontend` | `FRON` | First 4 letters, uppercased |
| `authentication-service` | `AUTH` | First 4 letters of first word |
| `api-backend` | `AB` | First letter of each hyphenated part |
| `user-management-system` | `UMS` | First letter of each word |

### Case-Insensitive Matching

Project resolution is case-insensitive for maximum flexibility:

```bash
# All of these work:
lotar task list --project=frontend
lotar task list --project=FRONTEND  
lotar task list --project=Frontend
lotar task list --project=FRON
lotar task list --project=fron
```

## ⚙️ Auto-Detection & Zero Configuration

### Global Config Auto-Detection

When LoTaR creates a global configuration file, it automatically detects your project structure and sets intelligent defaults:

```bash
# First time running in a repository with projects
lotar task add --title="First task" --project=myproject

# Output: "Created default global configuration at: .tasks/config.yml"
# Auto-detects and sets default_project based on existing or new project
```

**Detection Logic:**
1. **Existing projects found** → Sets `default_project` to the first alphabetically
2. **No existing projects** → Creates new project and sets it as default
3. **Multiple projects** → Chooses the most recently used or first alphabetically

### Project Auto-Initialization

When you create your first task in a project, LoTaR automatically:

1. **Creates project directory** with the appropriate prefix
2. **Generates project config** with sensible defaults inherited from global config
3. **Sets up proper file structure** for task storage

```bash
# This command automatically:
# - Creates FRON/ directory
# - Creates FRON/config.yml with project name "frontend"
# - Creates FRON/001.yml with your task
lotar task add --title="Setup routing" --project=frontend
```

## 🔍 Enhanced Search & Display

### Project Name Resolution in Search Results

Search results display full project names alongside task IDs for better readability:

```bash
lotar task search "authentication"

# Output:
Found 3 matching tasks:
  [AUTH-001] Implement OAuth login - AUTHENTICATION-SERVICE (Priority: HIGH, Status: TODO)  
  [FRON-005] Add auth UI components - FRONTEND (Priority: MEDIUM, Status: IN_PROGRESS)
  [BACK-012] Setup auth middleware - BACKEND (Priority: HIGH, Status: TODO)
```

### Different Output Formats

LoTaR uses different output formats optimized for each command:

**List Command** (clean, focused):
```bash
lotar task list --project=frontend

Tasks in project 'frontend':
  Setup routing (Priority: HIGH, Status: TODO)
  Add navigation (Priority: MEDIUM, Status: IN_PROGRESS)
  Fix responsive layout (Priority: LOW, Status: DONE)
```

**Search Command** (detailed, with context):
```bash
lotar task search "routing"

Found 2 matching tasks:
  [FRON-001] Setup routing - FRONTEND (Priority: HIGH, Status: TODO)
  [BACK-003] API routing setup - BACKEND (Priority: MEDIUM, Status: DONE)
```

## 🛠️ Advanced Usage Examples

### Cross-Project Workflows

```bash
# Create tasks in multiple projects using different naming styles
lotar task add --title="API endpoint" --project=backend
lotar task add --title="API client" --project=FRONTEND  # Full name
lotar task add --title="API docs" --project=DOC         # Prefix

# Search across projects
lotar task search "API"  # Finds tasks in all projects

# List tasks by project (works with any naming style)
lotar task list --project=backend
lotar task list --project=BACK      # Same as above
```

### Flexible Project References

```bash
# These all reference the same project:
lotar task status BACK-001 DONE --project=backend
lotar task status BACK-001 DONE --project=BACKEND
lotar task status BACK-001 DONE --project=BACK
lotar task status BACK-001 DONE  # Uses project from task ID
```

### Migration and Compatibility

If you have existing LoTaR repositories with manual prefixes, the smart resolution system is fully backward compatible:

```bash
# Existing projects continue to work
lotar task list --project=PROJ  # Still works if PROJ/ exists

# New projects benefit from smart naming
lotar task add --title="New feature" --project=user-authentication
# Creates USER/ directory automatically
```

## 🔧 Configuration Integration

### Global Config Inheritance

Project-specific configurations automatically inherit from global settings while allowing overrides:

```yaml
# .tasks/config.yml (global)
default_project: "frontend"
server_port: 8080
issue_states: ["TODO", "IN_PROGRESS", "DONE"]

# .tasks/FRON/config.yml (project-specific)
project_name: "FRONTEND"  # Automatically set during creation
# All other settings inherited from global config
```

### Template Application

Smart project creation works seamlessly with configuration templates:

```bash
# Create project with specific template
lotar config init --template=agile --project=user-management

# Later, smart resolution finds this project:
lotar task add --title="Epic planning" --project=user-management
lotar task add --title="Epic planning" --project=UM  # Same project
```

## 🎯 Benefits

### For Solo Developers
- **Zero cognitive overhead**: Use whatever project name feels natural
- **Consistent organization**: Automatic prefix generation keeps directories organized
- **Quick commands**: Type fewer characters with prefix shortcuts

### For Teams
- **Flexible conventions**: Team members can use full names or prefixes
- **Onboarding friendly**: New members can use intuitive full project names
- **Migration smooth**: Existing workflows continue to work unchanged

### For Large Codebases
- **Scalable naming**: Automatic prefix generation prevents naming conflicts
- **Searchable context**: Full project names in search results improve discoverability
- **Maintainable structure**: Consistent directory organization across projects

## 🔄 Migration Guide

### Upgrading from Manual Prefixes

If you have existing LoTaR repositories with manually created project directories:

1. **No changes required**: Existing prefixes continue to work
2. **Add project names**: Optionally add `project_name` to existing `config.yml` files to enable full name resolution
3. **New projects**: Benefit from smart naming automatically

### Example Migration

```bash
# Before: Manual prefix usage
lotar task add --title="Feature" --project=PROJ

# After: Use either format
lotar task add --title="Feature" --project=PROJ          # Still works
lotar task add --title="Feature" --project=project-name  # Now also works
```

This makes LoTaR's project management both powerful for advanced users and approachable for newcomers, while maintaining full backward compatibility.
