# Task File Format Specification

## Current Implementation

The current task structure in `src/store.rs` includes:

```rust
pub struct Task {
    pub id: u64,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub priority: u8,
    pub project: String,
    pub category: Option<String>,
    pub created: String,
    pub due_date: Option<String>,
    pub tags: Vec<String>,
}
```

## Target YAML Format (from README)

```yaml
Title: My Task
Subtitle: (optional) Subtitle
Description: (optional) Description
ID: 1234
Status: TODO
Priority: 1
Due: 2019-01-01
Created: 2019-01-01
Modified: 2019-01-01
Tags: [tag1, tag2]
Custom-X: value
```

## Missing Fields

The current implementation is missing:
- `Status` field for workflow management
- `Modified` timestamp for tracking changes
- Support for custom fields (`Custom-X: value`)

## File Structure

Based on README specifications:

```
.tasks/
├── metadata.yaml          # Project metadata
├── index.json            # Database indices
├── transitions.yaml      # Status transition rules
└── projects/
    └── project1/
        ├── metadata.yaml  # Project-specific metadata
        └── groups/
            └── group1/
                ├── task1.yaml
                ├── task2.yaml
                └── metadata.yaml
```

## Status Transitions

```yaml
# transitions.yaml
"TODO": ["IN_PROGRESS", "VERIFY"]
"IN_PROGRESS": ["VERIFY", "BLOCKED"]
"VERIFY": ["DONE", "BLOCKED"]
"BLOCKED": ["TODO", "IN_PROGRESS"]
"DONE": []
```

## Index Files

```json
{
  "id2file": {
    "1234": "projects/project1/groups/group1/task1.yaml"
  },
  "tag2id": {
    "tag1": ["1234", "5678"],
    "tag2": ["1234"]
  },
  "file2file": {
    "projects/project1/groups/group1/task1.yaml": "projects/project1/groups/group1/task1.yaml"
  }
}
```

## Implementation Requirements

1. **Add Status Field**: Extend Task struct with status enum
2. **Add Modified Field**: Track last modification timestamp
3. **Custom Fields**: Support arbitrary key-value pairs
4. **Index Management**: Create and maintain lookup tables
5. **Validation**: Ensure status transitions follow rules
