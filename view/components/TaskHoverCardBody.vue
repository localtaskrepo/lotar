<script setup lang="ts">
import { computed, watch } from 'vue'
import type { TaskDTO } from '../api/types'
import { customFieldEntriesFromTask } from '../composables/useColumns'
import { ensureCompletionPolicy, useCompletionPolicy } from '../composables/useCompletionPolicy'
import { formatRelativeTime, parseTaskDate, startOfLocalDay } from '../utils/date'
import { formatMember } from '../utils/member'
import { projectOf } from '../utils/text'
import { firstPreviewImage, taskAttachments } from '../utils/taskAttachments'
import AttachmentTile from './AttachmentTile.vue'

const props = defineProps<{
  task: Pick<
    TaskDTO,
    | 'id'
    | 'title'
    | 'status'
    | 'priority'
    | 'task_type'
    | 'reporter'
    | 'assignee'
    | 'effort'
    | 'tags'
    | 'sprints'
    | 'due_date'
    | 'created'
    | 'modified'
    | 'custom_fields'
    | 'task_state'
  > &
    Partial<Pick<TaskDTO, 'references'>>
  fields?: Partial<Record<string, boolean>>
}>()

const completion = useCompletionPolicy()

watch(
  () => props.task?.id,
  (id) => {
    const prefix = projectOf(id ?? '')
    if (prefix) void ensureCompletionPolicy(prefix)
  },
  { immediate: true },
)

type FieldKey =
  | 'id'
  | 'title'
  | 'status'
  | 'priority'
  | 'task_type'
  | 'reporter'
  | 'assignee'
  | 'effort'
  | 'tags'
  | 'sprints'
  | 'due_date'
  | 'modified'
  | 'attachments'

function isFieldVisible(key: FieldKey) {
  return props.fields?.[key] !== false
}

const showId = computed(() => isFieldVisible('id'))
const showTitle = computed(() => isFieldVisible('title'))
const showStatus = computed(() => isFieldVisible('status'))
const showTaskType = computed(() => isFieldVisible('task_type'))
const showPriority = computed(() => isFieldVisible('priority'))
const showEffort = computed(() => isFieldVisible('effort'))
const showAssignee = computed(() => isFieldVisible('assignee'))
const showReporter = computed(() => isFieldVisible('reporter'))
const showDueDate = computed(() => isFieldVisible('due_date'))
const showTags = computed(() => isFieldVisible('tags'))
const showSprints = computed(() => isFieldVisible('sprints'))
const showModified = computed(() => isFieldVisible('modified'))

const hasHeader = computed(() => showId.value || showTitle.value)

const attachments = computed(() => (isFieldVisible('attachments') ? taskAttachments(props.task) : []))
const preview = computed(() => (attachments.value.length ? firstPreviewImage(props.task) : null))
const otherFiles = computed(() => attachments.value.filter((attachment) => attachment.url !== preview.value?.url))

const customFieldEntries = computed(() => {
  const enabled = Object.entries(props.fields ?? {})
    .filter(([, on]) => on)
    .map(([key]) => key)
  return customFieldEntriesFromTask(props.task, enabled)
})

const tags = computed(() => (props.task.tags || []).slice(0, 12))

const relativeTime = (value: string) => formatRelativeTime(value)

const modifiedInfo = computed(() => {
  const raw = (props.task.modified || '').trim()
  if (!raw) return null
  let parsed: Date | null
  try {
    const d = new Date(raw)
    parsed = Number.isFinite(d.getTime()) ? d : null
  } catch {
    parsed = null
  }
  if (!parsed) return { label: raw, title: raw }
  return { label: relativeTime(raw), title: parsed.toLocaleString() }
})

const dueInfo = computed(() => {
  const raw = props.task.due_date
  if (!raw) return null
  const parsed = parseTaskDate(raw)
  if (!parsed) {
    return { label: raw, context: '', tone: null as 'overdue' | 'due-today' | 'soon' | null }
  }
  const startToday = startOfLocalDay(new Date())
  const startDue = startOfLocalDay(parsed)
  const diffDays = Math.round((startDue.getTime() - startToday.getTime()) / 86_400_000)
  let context: string
  let tone: 'overdue' | 'due-today' | 'soon' | null = null
  if (diffDays < 0) {
    context = `${Math.abs(diffDays)} day${Math.abs(diffDays) === 1 ? '' : 's'} ago`
    // Terminal tasks are never shown as overdue (DEV-21 shared policy).
    tone = completion.isTaskOverdue(props.task) ? 'overdue' : null
  } else if (diffDays === 0) {
    context = 'Today'
    tone = 'due-today'
  } else if (diffDays === 1) {
    context = 'Tomorrow'
    tone = 'soon'
  } else if (diffDays <= 7) {
    context = `In ${diffDays} days`
    tone = 'soon'
  } else {
    context = `In ${diffDays} days`
  }
  const today = new Date()
  const sameYear = today.getFullYear() === parsed.getFullYear()
  const label = parsed.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: sameYear ? undefined : 'numeric',
  })
  return { label, context, tone }
})
</script>

<template>
  <div v-if="preview" class="task-hover-card__preview">
    <AttachmentTile :attachment="preview" cover :cover-height="140" />
  </div>
  <header v-if="hasHeader" class="task-hover-card__header">
    <span v-if="showId" class="task-hover-card__id">{{ task.id }}</span>
    <span v-if="showTitle" class="task-hover-card__title">{{ task.title }}</span>
  </header>
  <dl class="task-hover-card__meta">
    <div v-if="showStatus && task.status" class="task-hover-card__row">
      <dt>Status</dt>
      <dd>{{ task.status }}</dd>
    </div>
    <div v-if="showTaskType && task.task_type" class="task-hover-card__row">
      <dt>Type</dt>
      <dd>{{ task.task_type }}</dd>
    </div>
    <div v-if="showPriority && task.priority" class="task-hover-card__row">
      <dt>Priority</dt>
      <dd>{{ task.priority }}</dd>
    </div>
    <div v-if="showEffort && task.effort" class="task-hover-card__row">
      <dt>Effort</dt>
      <dd>{{ task.effort }}</dd>
    </div>
    <div v-if="showAssignee && task.assignee" class="task-hover-card__row">
      <dt>Assignee</dt>
      <dd>{{ formatMember(task.assignee) }}</dd>
    </div>
    <div v-if="showReporter && task.reporter" class="task-hover-card__row">
      <dt>Reporter</dt>
      <dd>{{ formatMember(task.reporter) }}</dd>
    </div>
    <div v-if="showDueDate && dueInfo" class="task-hover-card__row">
      <dt>Due</dt>
      <dd :class="['task-hover-card__due', dueInfo.tone && `is-${dueInfo.tone}`]">
        <span>{{ dueInfo.label }}</span>
        <span v-if="dueInfo.context" class="task-hover-card__due-context">{{ dueInfo.context }}</span>
      </dd>
    </div>
    <div v-if="showSprints && task.sprints?.length" class="task-hover-card__row">
      <dt>Sprints</dt>
      <dd>{{ task.sprints.map((id) => `#${id}`).join(', ') }}</dd>
    </div>
    <div v-if="showModified && modifiedInfo" class="task-hover-card__row">
      <dt>Updated</dt>
      <dd :title="modifiedInfo.title">{{ modifiedInfo.label }}</dd>
    </div>
    <div v-for="entry in customFieldEntries" :key="`custom-${entry.key}`" class="task-hover-card__row">
      <dt>{{ entry.label }}</dt>
      <dd>{{ entry.value }}</dd>
    </div>
    <div v-if="otherFiles.length" class="task-hover-card__row">
      <dt>Files</dt>
      <dd class="task-hover-card__files">
        <span v-for="file in otherFiles" :key="file.path" class="task-hover-card__file">
          <AttachmentTile :attachment="file" :size="18" />{{ file.name }}
        </span>
      </dd>
    </div>
  </dl>
  <div v-if="showTags && tags.length" class="task-hover-card__tags">
    <span v-for="tag in tags" :key="tag" class="tag">{{ tag }}</span>
  </div>
</template>

<style scoped>
.task-hover-card__preview {
  margin: -12px -12px 10px;
  overflow: hidden;
  border-radius: inherit;
  border-bottom-left-radius: 0;
  border-bottom-right-radius: 0;
  background: var(--color-surface);
}

.task-hover-card__files {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.task-hover-card__file {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  overflow-wrap: anywhere;
}

.task-hover-card__header {
  display: flex;
  gap: 8px;
  align-items: baseline;
  margin-bottom: 6px;
}

.task-hover-card__id {
  flex: 0 0 auto;
  font-family: var(--font-mono);
  font-size: 0.72rem;
  color: var(--color-muted);
  white-space: nowrap;
  overflow-wrap: normal;
  word-break: keep-all;
}

.task-hover-card__title {
  min-width: 0;
  font-weight: 600;
  font-size: 0.95rem;
  line-height: 1.35;
}

.task-hover-card__meta {
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 4px 12px;
}

.task-hover-card__row {
  display: contents;
}

.task-hover-card__row dt {
  margin: 0;
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: color-mix(in oklab, var(--color-muted) 80%, transparent);
}

.task-hover-card__row dd {
  margin: 0;
  font-size: 0.85rem;
  font-weight: 500;
}

.task-hover-card__due {
  display: inline-flex;
  gap: 6px;
  align-items: baseline;
}

.task-hover-card__due.is-overdue {
  color: var(--color-danger);
}

.task-hover-card__due.is-due-today {
  color: var(--color-warning);
}

.task-hover-card__due.is-soon {
  color: var(--color-accent);
}

.task-hover-card__due-context {
  font-size: 0.72rem;
  font-weight: 400;
  color: color-mix(in oklab, currentColor 65%, transparent);
}

.task-hover-card__tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 10px;
}
</style>
