# lotar task

Full task management with legacy command structure.

## Usage

```bash
lotar task <ACTION> [OPTIONS]
```

## Actions

### add
Create a new task with detailed options.

```bash
lotar task add --title="Task title" [OPTIONS]
```

### list
List tasks with filtering options.

```bash
lotar task list [--project=PROJECT]
```

### edit
Edit an existing task.

```bash
lotar task edit <TASK_ID> [OPTIONS]
```

### status
Change task status.

```bash
lotar task status <TASK_ID> <STATUS>
```

### search
Search tasks by query.

```bash
lotar task search <QUERY> [OPTIONS]
```

### delete
Delete a task.

```bash
lotar task delete <TASK_ID>
```

## Examples

```bash
# Create task with full options
lotar task add --title="Implement API" --type=feature --priority=high

# List tasks in specific project
lotar task list --project=backend

# Change task status
lotar task status PROJ-123 done

# Search for tasks
lotar task search "authentication" --status=todo
```

## Notes

- This is the legacy command interface
- For simpler operations, use direct commands: `lotar add`, `lotar list`, `lotar status`
- All options from the direct commands are available here
- Project context is automatically detected or can be specified with `--project`
