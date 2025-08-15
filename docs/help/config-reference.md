# Configuration Reference

All configuration keys by scope with types and notes. Canonical YAML uses nested groups.

## Precedence
CLI > env > home > project > global > defaults. See [Resolution & Precedence](./precedence.md).

## Canonical keys (nested)
- server.port: number (default 8080; ignored in project-level config)
- default.project: string (default project prefix; ignored in project-level config)
- default.assignee: string
- default.reporter: string
- default.priority: enum Priority
- default.status: enum TaskStatus
- default.category: string
- default.tags: string[]
- issue.states: string[] (e.g., [Todo, InProgress, Done])
- issue.types: string[] (feature, bug, epic, spike, chore)
- issue.priorities: string[] (Low, Medium, High, Critical)
- issue.categories: string[]
- issue.tags: string[]
- custom.fields: string[]
- scan.signal_words: string[] (default: [TODO, FIXME, HACK, BUG, NOTE])
- scan.ticket_patterns: string[] (regex patterns to detect ticket keys)
- auto.identity: boolean (default true)
- auto.identity_git: boolean (default true)
- auto.set_reporter: boolean (default true)
- auto.assign_on_status: boolean (default true)
- auto.codeowners_assign: boolean (default true)
- auto.tags_from_path: boolean (default true)
- auto.branch_infer_type: boolean (default true)

## Home and Project keys
Same shape as global; project values override global for that project. Use `project.id` for the project identifier.

## Examples
```yaml
# ~/.lotar (home config)
default:
	project: DEMO

# .tasks/config.yml (global)
issue:
	states: [Todo, InProgress, Done]
	priorities: [Low, Medium, High]
scan:
	signal_words: [TODO, FIXME]
	ticket_patterns: ["[A-Z]{2,}-\\d+"]

# .tasks/DEMO/config.yml (project)
project:
	id: DEMO
issue:
	types: [feature, bug, chore]
auto:
	identity: true
	identity_git: true
	assign_on_status: true
default:
	category: Engineering
	tags: [oncall, sev]
```

## Validation
- Values are checked against allowed enums.
- Unknown keys are ignored or flagged during validation (see `lotar config validate`).
- If `scan.ticket_patterns` include invalid regex or overlapping patterns, validation will report errors/warnings.
