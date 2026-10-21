<template>
  <td :class="['task-cell', `task-cell--${col}`]">
    <template v-if="col === 'id'">
      <span class="muted">{{ projectOf(task.id) }}</span>
      <strong>{{ numericOf(task.id) }}</strong>
    </template>
    <template v-else-if="col === 'title'">
      <span class="task-title">
        {{ task.title || 'Untitled task' }}
        <span
          v-if="multiSprint"
          class="task-title__badge badge badge--info"
          :title="multiSprintTooltip(task, contextSprintId)"
          :aria-label="multiSprintTooltip(task, contextSprintId)"
        >
          Multi
        </span>
      </span>
    </template>
    <template v-else-if="col === 'status'">
      <span class="status" :data-status="task.status">{{ task.status }}</span>
    </template>
    <template v-else-if="col === 'priority'">
      <span>{{ task.priority || '—' }}</span>
    </template>
    <template v-else-if="col === 'task_type'">
      <span>{{ task.task_type || '—' }}</span>
    </template>
    <template v-else-if="col === 'reporter'">
      <span v-if="task.reporter">{{ formatMember(task.reporter) }}</span>
      <span v-else class="muted">—</span>
    </template>
    <template v-else-if="col === 'assignee'">
      <span v-if="task.assignee">{{ formatMember(task.assignee) }}</span>
      <span v-else class="muted">—</span>
    </template>
    <template v-else-if="col === 'effort'">
      <span v-if="task.effort">{{ task.effort }}</span>
      <span v-else class="muted">—</span>
    </template>
    <template v-else-if="col === 'tags'">
      <div class="row" style="gap: 6px; flex-wrap: wrap; align-items: center;">
        <span v-for="tag in task.tags" :key="task.id + '-tag-' + tag" class="tag">{{ tag }}</span>
        <span v-if="!task.tags.length" class="muted">—</span>
      </div>
    </template>
    <template v-else-if="col === 'sprints'">
      <!-- Backlog rows have no sprint membership to show. -->
      <span v-if="contextSprintId === null" class="muted">—</span>
      <div v-else class="row" style="gap: 6px; flex-wrap: wrap; align-items: center;">
        <span
          v-for="sprintId in task.sprints"
          :key="task.id + '-sprint-' + sprintId"
          class="chip small"
          :class="sprintStateClass(sprintId)"
        >
          {{ sprintLabel(sprintId) }}
        </span>
        <span v-if="!task.sprints.length" class="muted">—</span>
      </div>
    </template>
    <template v-else-if="col === 'due_date'">
      <span v-if="formatDue(task.due_date)" :class="{ 'text-overdue': overdue }">
        {{ formatDue(task.due_date) }}
      </span>
      <span v-else class="muted">—</span>
    </template>
    <template v-else-if="col === 'created'">
      <span :title="formatExact(task.created)">{{ formatRelative(task.created) }}</span>
    </template>
    <template v-else-if="col === 'modified'">
      <span :title="formatExact(task.modified)">{{ formatRelative(task.modified) }}</span>
    </template>
    <template v-else>
      <span>{{ (task as any)[col] ?? '—' }}</span>
    </template>
  </td>
</template>

<script setup lang="ts">
import type { TaskDTO } from '../api/types'
import type { ColKey } from '../composables/useColumns'
import { formatRelativeTime } from '../utils/date'
import { formatMember } from '../utils/member'
import { numericOf, projectOf } from '../utils/text'

/**
 * One task cell shared by the sprint and backlog tables on the Sprints page.
 * `contextSprintId` is the sprint whose table renders the row, or `null` for
 * the backlog: it selects the multi-sprint tooltip wording and whether the
 * sprints column shows membership chips or a placeholder.
 */
defineProps<{
  task: TaskDTO
  col: ColKey
  contextSprintId: number | null
  multiSprint: boolean
  overdue: boolean
  multiSprintTooltip: (task: TaskDTO, contextSprintId: number | null) => string
  sprintLabel: (id: number) => string
  sprintStateClass: (id: number) => string
}>()

function formatDue(value?: string | null) {
  if (!value) return ''
  try {
    const date = new Date(value)
    if (Number.isNaN(date.getTime())) return value
    return date.toLocaleDateString()
  } catch {
    return value
  }
}

const formatRelative = (value?: string | null) => formatRelativeTime(value, { empty: '—' })

function formatExact(value?: string | null) {
  if (!value) return ''
  try {
    return new Date(value).toLocaleString()
  } catch {
    return value || ''
  }
}
</script>

<style scoped>
.task-title {
  font-weight: 600;
}

.task-title__badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  margin-left: 6px;
  cursor: default;
}

.badge {
  display: inline-flex;
  align-items: center;
  padding: 0.15rem 0.45rem;
  border-radius: var(--radius-pill);
  font-size: var(--text-xs, 0.75rem);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.badge--info {
  background: color-mix(in oklab, var(--color-accent) 18%, transparent);
  color: var(--color-accent);
}

.status[data-status] {
  text-transform: capitalize;
}

.text-overdue {
  color: var(--color-danger);
  font-weight: 600;
}
</style>
