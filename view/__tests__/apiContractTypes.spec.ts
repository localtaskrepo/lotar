/**
 * DEV-82 static contract regression: pins the wire-accurate optionality and
 * nullability of the repaired TS interfaces. The compile-time assertions
 * (`@ts-expect-error`) are enforced by `vue-tsc` via `npm run
 * lint:frontend`; the runtime tests only keep this file in the suite.
 */
import { describe, expect, it } from 'vitest'
import type {
  AutomationSimulateResponse,
  SprintCreateRequest,
  SprintCreateResponse,
  SprintDeleteResponse,
  SprintListItem,
  SprintUpdateRequest,
  SprintUpdateResponse,
  TaskDTO,
  TaskListResponse,
} from '../api/types'

const task: TaskDTO = {
  id: 'QA-1',
  title: 'Contract shape',
  status: 'Todo',
  priority: 'Medium',
  task_type: 'Feature',
  created: '2026-09-15T10:00:00Z',
  modified: '2026-09-15T11:30:00Z',
  tags: [],
  relationships: {},
  comments: [],
  references: [],
  sprints: [],
  history: [],
  custom_fields: {},
}

describe('api contract type shapes', () => {
  it('task list omits tasks on empty pages', () => {
    const empty: TaskListResponse = { total: 0, limit: 50, offset: 0 }
    expect(empty.total).toBe(0)
  })

  it('sprint responses omit empty vectors', () => {
    const created: SprintCreateResponse = { status: 'ok', sprint: sprintItem() }
    const updated: SprintUpdateResponse = { status: 'ok', sprint: sprintItem() }
    expect(created.sprint.id).toBe(1)
    expect(updated.sprint.id).toBe(1)
  })

  it('cleanup delete integrity omits empty vectors', () => {
    // Wire shape after cleanup_missing=true with nothing missing (matches the
    // Rust/spec example already validated by the dev82 contract tests):
    // missing_sprints/removed_by_sprint/remaining_missing are omitted.
    const deleted: SprintDeleteResponse = {
      status: 'ok',
      deleted: true,
      sprint_id: 1,
      sprint_label: 'W42',
      removed_references: 3,
      updated_tasks: 2,
      integrity: {
        auto_cleanup: { removed_references: 3, updated_tasks: 2 },
      },
    }
    expect(deleted.integrity?.auto_cleanup?.removed_references).toBe(3)
  })

  it('simulate always carries all five keys', () => {
    const unmatched: AutomationSimulateResponse = {
      matched: false,
      rule_name: null,
      actions: [],
      task_before: task,
      task_after: null,
    }
    expect(unmatched.task_after).toBeNull()
  })
})

function sprintItem(): SprintListItem {
  // Minimal wire shape: optional members omitted, never null.
  return { id: 1, display_name: 'W42', state: 'pending' }
}

// Requests accept null-as-omission on Option fields (skip_defaults stays bool).
const createRequest: SprintCreateRequest = {
  label: 'W42',
  goal: null,
  plan_length: null,
  ends_at: null,
  starts_at: null,
  capacity_points: null,
  capacity_hours: null,
  overdue_after: null,
  notes: null,
  skip_defaults: true,
}

// Single-value options treat null as omitted; double-options clear.
const updateRequest: SprintUpdateRequest = {
  sprint: 1,
  label: null,
  goal: null,
  overdue_after: null,
  notes: null,
  capacity_points: null,
  actual_started_at: null,
}

// @ts-expect-error rule_name is always present (null when unmatched)
const missingRuleName: AutomationSimulateResponse = {
  matched: false,
  actions: [],
  task_before: task,
  task_after: null,
}

// @ts-expect-error task_after is required, null when unmatched
const missingTaskAfter: AutomationSimulateResponse = {
  matched: true,
  rule_name: 'r',
  actions: [],
  task_before: task,
}

// @ts-expect-error task_after must be a task or null, never a string
const stringTaskAfter: AutomationSimulateResponse = { matched: true, rule_name: 'r', actions: [], task_before: task, task_after: 'nope' }

// @ts-expect-error task_before is required and always a task object
const missingTaskBefore: AutomationSimulateResponse = {
  matched: true,
  rule_name: 'r',
  actions: [],
  task_after: null,
}

// @ts-expect-error sprint list items never carry null on the wire
const nullLabel: SprintListItem = { id: 1, display_name: 'W42', state: 'pending', label: null }

// @ts-expect-error empty pages omit tasks; total is always present
const missingTotal: TaskListResponse = { limit: 50, offset: 0 }

// @ts-expect-error skip_defaults is a bool and cannot be null
const nullSkipDefaults: SprintCreateRequest = { skip_defaults: null }

void [
  createRequest,
  updateRequest,
  missingRuleName,
  missingTaskAfter,
  stringTaskAfter,
  missingTaskBefore,
  nullLabel,
  missingTotal,
  nullSkipDefaults,
]
