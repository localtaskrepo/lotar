# lotar task

Full task management with legacy command structure.

**Alias:** `lotar tasks` (both singular and plural work identically)

## Usage

```bash
lotar task <ACTION> [OPTIONS]
# or
lotar tasks <ACTION> [OPTIONS]
```

## Actions

### add
Create a new task with detailed options.

```bash
lotar task add --title="Task title" [OPTIONS]
```

### list
List tasks with advanced filtering options.

```bash
lotar task list [OPTIONS]
```

### edit
Edit an existing task.

```bash
lotar task edit <TASK_ID> [OPTIONS] [--dry-run]
# JSON preview: lotar task edit PROJ-1 --priority=high --dry-run --format=json
# {"status":"preview","action":"edit","task_id":"PROJ-1","priority":"HIGH", ...}
```

### status
Change or view task status.

```bash
lotar task status <TASK_ID> [NEW_STATUS]
```

### priority
Change or view task priority.

```bash
lotar task priority <TASK_ID> [NEW_PRIORITY]
```

### assignee
**⚠️ PLACEHOLDER** - Change or view task assignee.

```bash
lotar task assignee <TASK_ID> [NEW_ASSIGNEE]
```

### due-date
**⚠️ PLACEHOLDER** - Change or view task due date.

```bash
lotar task due-date <TASK_ID> [NEW_DATE]
```

### delete
Delete a task.

```bash
lotar task delete <TASK_ID> [--dry-run] [--force]
# JSON preview: lotar task delete PROJ-1 --dry-run --format=json
# {"status":"preview","action":"delete","task_id":"PROJ-1","project":"PROJ"}
```

## Examples

```bash
# Create task with full options
lotar task add --title="Implement API" --type=feature --priority=high

# List tasks in specific project (using alias)
lotar tasks list --project=backend

# Change task status
lotar task status PROJ-123 done

# Change task priority (using alias)
lotar tasks priority PROJ-123 high

# Set task assignee
lotar task assignee PROJ-123 john.doe@example.com

# View task due date (using alias)
lotar tasks due-date PROJ-123

# Set task due date
lotar task due-date PROJ-123 2025-12-31
```

## Notes

- This is the legacy command interface
- For simpler operations, use direct commands: `lotar add`, `lotar list`, `lotar status`
- All options from the direct commands are available here
- Project context is automatically detected or can be specified with `--project`
