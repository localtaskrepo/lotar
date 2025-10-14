# Task Model

Canonical fields and enums for tasks.

## TaskDTO
- id: string (PROJECT-N)
- title: string
- status: enum TaskStatus
- priority: enum Priority
- task_type: enum TaskType
- reporter?: string
- assignee?: string
- created: RFC3339 timestamp
- modified: RFC3339 timestamp
- due_date?: string
- effort?: string
- subtitle?: string
- description?: string
- tags: string[]
- relationships: object
- comments: TaskComment[]
- custom_fields: object

## Enums
- TaskStatus: TODO | IN_PROGRESS | VERIFY | BLOCKED | DONE
- Priority: LOW | MEDIUM | HIGH | CRITICAL
- TaskType: feature | bug | epic | spike | chore

## ID and prefixes
- Project prefix derived from name (see `generate_project_prefix`).
- IDs format: PREFIX-<number> (e.g., AUTH-1).

## Invariants
- created <= modified
- tags is an array (may be empty)
- Explicit assignee values are preserved during status changes

See also: [OpenAPI spec](../openapi.json) and [Identity & Users](./identity.md).
