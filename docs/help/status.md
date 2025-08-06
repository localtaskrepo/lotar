# lotar status

Change task status with validation and different output formats.

## Usage

```bash
lotar status <TASK_ID> <NEW_STATUS>
```

## Quick Examples

```bash
# Basic status change
lotar status AUTH-001 in_progress

# With explicit project
lotar status 123 verify --project=backend

# JSON output for automation
lotar status AUTH-001 done --format=json
```

## Status Values

Available statuses depend on your project configuration. Common defaults:

- `todo` - Task is planned but not started
- `in_progress` - Task is currently being worked on  
- `verify` - Task completed, awaiting verification/review
- `blocked` - Task cannot proceed due to dependency
- `done` - Task is completed and verified

Check your project's valid statuses:
```bash
lotar config get issue_states
```

## Task ID Resolution

LoTaR intelligently resolves task IDs:

### Full Task ID
```bash
lotar status AUTH-001 done  # Complete project-prefixed ID
```

### Short Task ID
```bash
lotar status 001 done       # Auto-resolves to current project
lotar status 123 done --project=backend  # Explicit project context
```

### Project Auto-Detection
1. **Current directory**: If in a project directory
2. **Default project**: From global configuration
3. **Task ID prefix**: Extracts project from AUTH-001 format
4. **Explicit project**: Using `--project` flag

## Output Formats

### Text (Default)
Human-readable confirmation:
```
✅ Task AUTH-001 status changed from TODO to IN_PROGRESS
```

### JSON
Machine-readable for scripts:
```json
{
  "task_id": "AUTH-001",
  "old_status": "TODO", 
  "new_status": "IN_PROGRESS",
  "timestamp": "2025-08-01T10:30:00Z",
  "status": "success"
}
```

### Table  
Structured update information:
```
| Property    | Value       |
|-------------|-------------|
| Task ID     | AUTH-001    |
| Old Status  | TODO        |
| New Status  | IN_PROGRESS |
| Changed By  | john.doe    |
| Timestamp   | 10:30 AM    |
```

## Validation

Status changes are validated against project configuration:

### Status Validation
- New status must be in project's `issue_states` list
- Invalid statuses are rejected with helpful error messages

### Transition Rules
Future enhancement: Validate allowed transitions
```yaml
# transitions.yml (planned)
transitions:
  TODO: [IN_PROGRESS, BLOCKED]
  IN_PROGRESS: [VERIFY, BLOCKED, TODO]  
  VERIFY: [DONE, IN_PROGRESS]
```

## Error Handling

### Common Errors

**Task Not Found:**
```
❌ Task 'AUTH-999' not found in project 'auth'
```

**Invalid Status:**
```
❌ Status validation failed: 'invalid_status' is not in allowed values: [TODO, IN_PROGRESS, VERIFY, DONE]
```

**Project Resolution Failed:**
```
❌ Could not resolve project: No project found for task ID '123'
```

## Automation Examples

### CI/CD Integration
```bash
# Mark tasks as done when PR merges
lotar status $(git log --oneline -1 | grep -o 'AUTH-[0-9]*') done --format=json

# Bulk status updates
cat task_ids.txt | xargs -I {} lotar status {} in_progress
```

### Workflow Scripts
```bash
#!/bin/bash
# Deploy script that updates task status
TASK_ID=$1
lotar status $TASK_ID verify --format=json > /tmp/status_change.json
if [ $? -eq 0 ]; then
    echo "✅ Task $TASK_ID ready for verification"
    # Notify team, update external systems, etc.
fi
```

### Batch Operations
```bash
# Move all TODO tasks assigned to john to IN_PROGRESS
lotar list --assignee=john.doe --status=todo --format=json | \
jq -r '.[] | .id' | \
xargs -I {} lotar status {} in_progress
```

## Integration with External Tools

### Jira/GitHub Issues
```bash
# Sync status to external system
TASK_ID="AUTH-001"
NEW_STATUS="done"
lotar status $TASK_ID $NEW_STATUS --format=json > /tmp/change.json

# Extract and sync to external system
EXTERNAL_ID=$(lotar config get custom_fields.jira_id --task=$TASK_ID)
curl -X PUT "https://api.jira.com/issue/$EXTERNAL_ID" \
     -d '{"fields": {"status": "Done"}}'
```

### Slack Notifications
```bash
# Notify team when critical tasks change status
if lotar status AUTH-001 done --format=json | jq -e '.priority == "CRITICAL"'; then
    slack-cli send "#dev-team" "🎉 Critical task AUTH-001 completed!"
fi
```

## Keyboard Shortcuts & Aliases

Add to your shell configuration:
```bash
# Quick status changes
alias s='lotar status'
alias done='lotar status $1 done'
alias progress='lotar status $1 in_progress'

# Usage
done AUTH-001
progress AUTH-002
```
