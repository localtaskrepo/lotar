# Task File Format Specification

*Last Updated: 2025-07-28*
*Revision: Pragmatic approach with standard fields + configurable custom fields*

## Overview

The task file format balances **standardization** with **flexibility**. Core project management fields are built-in and handled specially, while custom fields get generic treatment for filtering and search.

## Core Design Principles

1. **Standard Fields**: Common project management fields are built-in (effort, due_date, etc.)
2. **Configurable Custom Fields**: Teams can add their own fields with generic UI treatment
3. **Typed Relationships**: Task relationships specify both target and relationship type
4. **Git-Based History**: Decision history comes from git commits, not embedded data
5. **Comments Section**: Simple text comments separate from git history

## Standard Task File Format

```yaml
# Core Identity (always required)
id: "AUTH-001"
title: "Implement OAuth Authentication"
status: "TODO"                     # Built-in enum: TODO, IN_PROGRESS, REVIEW, DONE, CANCELLED

# Standard Project Management Fields (built-in, handled specially)
priority: "HIGH"                   # Built-in enum: LOW, MEDIUM, HIGH, CRITICAL
type: "feature"                    # Built-in enum: feature, bug, epic, spike, chore
assignee: "john.doe@company.com"   # Built-in string with autocomplete
project: "webapp"                  # Built-in string
created: "2025-07-28T10:00:00Z"    # Built-in timestamp (auto-generated)
modified: "2025-07-28T14:30:00Z"   # Built-in timestamp (auto-updated)
due_date: "2025-08-15"             # Built-in date with date picker
effort: "5d"                       # Built-in effort field (handles points, hours, days)

# Acceptance criteria as simple list (built-in)
acceptance_criteria:
  - "User can login with Google OAuth"
  - "User can login with GitHub OAuth"
  - "Session expires after 24 hours"
  - "Failed login attempts are logged"

# Typed relationships (built-in, special UI treatment)
relationships:
  # Internal task relationships
  depends_on: ["AUTH-002", "SEC-001"]    # Task dependencies
  blocks: ["USER-005"]                   # Tasks blocked by this one
  related: ["AUTH-003"]                  # General relationships
  parent: ["EPIC-USER-AUTH"]             # Parent epic/story
  child: []                              # Subtasks
  fixes: []                              # Bug fixes
  duplicate_of: []                       # Duplicate tracking
  
  # External system relationships with prefixed references
  external_links:
    implements: ["github:myorg/frontend#456", "jira:PROJ-789"]
    depends_on: ["github:#123"]           # Current repo context
    references: ["linear:LIN-456", "azure:12345"]
    blocks: ["jira:PROJ-234"]

# Team-specific custom fields (generic UI treatment)
epic: "user-management"            # String: text search + autocomplete
customer_impact: "high"            # String: text search  
story_points: 8                    # Number: range filter
security_review_required: true     # Boolean: checkbox filter
affected_components: ["frontend", "backend", "database"]  # Array: multi-select
regulatory_requirement: "GDPR"     # String: text search + autocomplete
business_value: "revenue"          # String: dropdown if enum configured

# Comments section (not git history)
comments:
  - author: "john.doe"
    date: "2025-07-28T10:00:00Z"
    text: "Starting with Google OAuth first, then GitHub"
```

# OAuth Authentication Implementation

## Context and Background
Our users currently create separate accounts for our application, creating friction in the signup process.

## Technical Approach
We'll implement OAuth 2.0 using the `oauth2` Rust crate with custom provider configurations.

## Implementation Progress

### 2025-07-28 - Initial Planning
- Researched OAuth 2.0 best practices
- Selected `oauth2` crate for implementation
- Planned Google OAuth integration first

## Files and References
- `src/auth/oauth.rs` - Core OAuth implementation
- `src/auth/providers/` - Provider-specific code

## Built-in Standard Fields

### Core Identity
- **id**: Unique task identifier (required)
- **title**: Task title (required)  
- **status**: Task status (required, built-in enum)

### Standard Project Management
- **priority**: Task priority (built-in enum: LOW, MEDIUM, HIGH, CRITICAL)
- **type**: Task type (built-in enum: feature, bug, epic, spike, chore)
- **assignee**: Person assigned (string with autocomplete from git history)
- **project**: Project name (string with autocomplete)
- **created**: Creation timestamp (auto-generated)
- **modified**: Last modified timestamp (auto-updated)
- **due_date**: Due date (date field with date picker)
- **effort**: Effort estimate (special handling for different units)

### Special Built-in Fields
- **acceptance_criteria**: List of strings (special multi-line editor)
- **relationships**: Typed task relationships (special relationship UI)
- **comments**: Discussion thread (special commenting interface)

## Effort Field - Special Handling
Effort supports multiple units with normalization

```yaml
effort: "5d"        # 5 days
```
```yaml
effort: "40h"       # 40 hours
```
```yaml
effort: "8sp"       # 8 story points
```
```yaml
effort: "3w"        # 3 weeks
```
```yaml
effort: "2"         # Default unit (configured per project)
```

The effort field gets special treatment:
- **Input normalization**: Converts different units to standard format
- **Display flexibility**: Shows in team's preferred units
- **Aggregation**: Can sum efforts across tasks
- **Reporting**: Special handling in burndown charts, velocity calculations

## Custom Fields Configuration

### Project Configuration (.lotar/config.yaml)
```yaml
# Extend built-in enums
enums:
  status:
    extend: true  # Add to built-in statuses
    values: ["BLOCKED", "WAITING_FOR_STAKEHOLDER"]
  priority:
    extend: true
    values: ["URGENT"]  # Add URGENT above CRITICAL

# Custom fields with generic UI treatment
custom_fields:
  epic:
    type: "string"
    autocomplete: true      # UI: text input with autocomplete
    filterable: true
  story_points:
    type: "number"
    min: 0
    max: 100               # UI: number input with range filter
  security_review_required:
    type: "boolean"        # UI: checkbox with boolean filter
  affected_components:
    type: "array[string]"  # UI: multi-select with array filter
  business_value:
    type: "enum"
    values: ["revenue", "cost_savings", "compliance", "user_experience"]
                          # UI: dropdown with enum filter
  customer_impact:
    type: "string"
    searchable: true      # UI: text input with text search

# Default effort unit
effort:
  default_unit: "sp"      # Story points
  allowed_units: ["sp", "h", "d", "w"]
```

## Web Interface Treatment

### Built-in Fields (Special UI)
```javascript
// Built-in fields get specialized components
const TaskForm = () => (
  <form>
    <TextInput field="title" required />
    <StatusDropdown field="status" />     {/* Special status component */}
    <PriorityDropdown field="priority" /> {/* Special priority component */}
    <DatePicker field="due_date" />       {/* Special date component */}
    <EffortInput field="effort" />        {/* Special effort component */}
    <UserSelect field="assignee" />       {/* Autocomplete from git */}
    <AcceptanceCriteriaEditor />          {/* Special multi-line editor */}
    <RelationshipEditor />                {/* Special relationship UI */}
  </form>
);
```

### Custom Fields (Generic UI)
```javascript
// Custom fields get generic treatment based on type
const CustomFieldRenderer = ({ field, value, type }) => {
  switch(type) {
    case 'string':
      return <TextInput value={value} searchable autocomplete />;
    case 'number': 
      return <NumberInput value={value} />;
    case 'boolean':
      return <Checkbox checked={value} />;
    case 'enum':
      return <Select options={field.values} value={value} />;
    case 'array[string]':
      return <MultiSelect values={value} />;
    default:
      return <TextInput value={value} />;
  }
};
```

### Filtering and Search
```javascript
// Filters generated automatically based on field types
const TaskFilters = () => (
  <div className="filters">
    {/* Built-in field filters */}
    <StatusFilter />
    <PriorityFilter />
    <AssigneeFilter />
    <DateRangeFilter field="due_date" />
    <EffortRangeFilter />
    
    {/* Custom field filters - generated automatically */}
    {customFields.map(field => (
      <GenericFilter 
        key={field.name}
        field={field}
        type={field.type}
      />
    ))}
  </div>
);
```

## CLI Interface - Focused on Essential Operations

```bash
# Core task operations
lotar task create --title="OAuth Implementation" --type="feature" --assignee="john.doe"
lotar task update AUTH-001 --status="IN_PROGRESS" --due-date="2025-08-15"
lotar task list --assignee="john.doe" --status="TODO"
lotar task show AUTH-001

# Relationship management
lotar task relate AUTH-001 --depends-on="AUTH-002"
lotar task relate AUTH-001 --blocks="USER-005"

# Comments
lotar task comment AUTH-001 "Starting with Google OAuth provider first"

# Custom fields (generic handling)
lotar task update AUTH-001 --set="story_points=8" --set="epic=user-management"
```

## Benefits of This Approach

### 1. **Standardization Where It Matters**
Common project management concepts (effort, due dates, assignments) get proper treatment and can power features like:
- Burndown charts
- Velocity tracking  
- Timeline views
- Workload balancing

### 2. **Flexibility Where Teams Differ**
Custom fields allow teams to add their specific needs:
- Regulatory requirements
- Business value tracking
- Component ownership
- Customer impact

### 3. **Generic UI Treatment**
Custom fields automatically get appropriate UI based on their type:
- Strings → text search
- Numbers → range filters
- Booleans → checkboxes
- Enums → dropdowns
- Arrays → multi-select

### 4. **Primary Interface Focus**
CLI handles essential operations, but teams primarily use:
- **Web interface** for task management, planning, reporting
- **IDE plugins** for linking code to tasks, viewing context
- **CLI** for automation, scripting, git integration

This gives you the best of both worlds: standardized core functionality with team-specific flexibility, all while keeping the implementation tractable.
