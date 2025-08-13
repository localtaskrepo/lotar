# Configuration Reference

All configuration keys by scope with types and notes.

## Precedence
CLI > env > home > project > global > defaults. See [Resolution & Precedence](./precedence.md).

## Global keys
- server_port: number (default 8080)
- default_project: string
- issue_states: string[] (e.g., [TODO, IN_PROGRESS, VERIFY, BLOCKED, DONE])
- issue_types: string[] (feature, bug, epic, spike, chore)
- issue_priorities: string[] (LOW, MEDIUM, HIGH, CRITICAL)
- categories: string[]
- tags: string[]
- default_assignee: string
- default_reporter: string
- default_priority: enum Priority
- default_status: enum TaskStatus
- custom_fields: object
- auto_set_reporter: boolean (default true)
- auto_assign_on_status: boolean (default true)

## Home and Project keys
Same shape as global; project values override global for that project.

## Examples
```yaml
# ~/.lotar (home config)
default_project: DEMO

# .tasks/config.yml (global)
issue_states: [TODO, IN_PROGRESS, DONE]
issue_priorities: [LOW, MEDIUM, HIGH]

# .tasks/DEMO/config.yml (project)
issue_types: [feature, bug, chore]
auto_assign_on_status: true
```

## Validation
- Values are checked against allowed enums.
- Unknown keys are ignored or flagged during validation (see `lotar config validate`).
