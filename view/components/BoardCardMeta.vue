<script setup lang="ts">
import { tagAttrs } from '../composables/useTagColors'
import { computed } from 'vue'
import type { TaskDTO } from '../api/types'
import { customFieldEntriesFromTask, injectColumnStore } from '../composables/useColumns'
import { formatMember, memberColor, memberInitials } from '../utils/member'
import { taskAttachments } from '../utils/taskAttachments'
import IconGlyph from './IconGlyph.vue'

const props = defineProps<{
  task: Pick<
    TaskDTO,
    | 'id'
    | 'status'
    | 'task_type'
    | 'effort'
    | 'reporter'
    | 'assignee'
    | 'due_date'
    | 'modified'
    | 'tags'
    | 'sprints'
    | 'custom_fields'
  > &
    Partial<Pick<TaskDTO, 'references'>>
  fields?: Partial<Record<string, boolean>>
  hasHeader: boolean
  dueInfo: { label: string; overdue: boolean }
  modifiedInfo: string
  sprint: {
    label: (id: number) => string
    stateClass: (id: number) => string
    tooltip: (id: number) => string
  }
}>()

const boardFields = injectColumnStore()
const visibleFields = computed(() =>
  props.fields === undefined ? boardFields : { isVisible: (key: string) => props.fields?.[key] !== false },
)

const customEntries = computed(() => {
  const keys =
    props.fields === undefined
      ? boardFields?.columns.value
      : Object.keys(props.fields).filter((key) => props.fields?.[key] && key.startsWith('custom:'))
  return customFieldEntriesFromTask(props.task, keys ?? [])
})

const attachments = computed(() => (visibleFields.value?.isVisible('attachments') ? taskAttachments(props.task) : []))

const hasPrimaryMeta = computed(() => {
  if (!visibleFields.value) return false
  const t = props.task
  return Boolean(
    (visibleFields.value.isVisible('status') && (t.status || '').trim()) ||
    (visibleFields.value.isVisible('task_type') && (t.task_type || '').trim()) ||
    (visibleFields.value.isVisible('effort') && (t.effort || '').trim()) ||
    (visibleFields.value.isVisible('reporter') && (t.reporter || '').trim()) ||
    (visibleFields.value.isVisible('assignee') && (t.assignee || '').trim()) ||
    (visibleFields.value.isVisible('due_date') && props.dueInfo.label) ||
    (visibleFields.value.isVisible('modified') && (t.modified || '').trim()) ||
    (visibleFields.value.isVisible('tags') && (t.tags || []).length) ||
    attachments.value.length > 0 ||
    customEntries.value.length > 0,
  )
})

const showSprints = computed(() => Boolean(visibleFields.value?.isVisible('sprints') && props.task.sprints?.length))
const hasMeta = computed(() => hasPrimaryMeta.value || showSprints.value)
</script>

<template>
  <footer v-if="hasMeta" class="task-meta" :class="{ 'task-meta--no-header': !hasHeader }">
    <div v-if="hasPrimaryMeta" class="row task-meta__tags">
      <span v-if="visibleFields?.isVisible('status') && (task.status || '').trim()" class="muted">{{
        task.status
      }}</span>
      <span v-if="visibleFields?.isVisible('task_type') && (task.task_type || '').trim()" class="muted">{{
        task.task_type
      }}</span>
      <span v-if="visibleFields?.isVisible('effort') && (task.effort || '').trim()" class="muted">{{
        task.effort
      }}</span>
      <span v-if="visibleFields?.isVisible('reporter') && (task.reporter || '').trim()" class="muted"
        >by {{ formatMember(task.reporter) }}</span
      >
      <span v-if="visibleFields?.isVisible('assignee') && (task.assignee || '').trim()" class="member-inline">
        <span
          class="member-badge small"
          :style="{ background: memberColor(task.assignee) }"
          :title="task.assignee || ''"
          >{{ memberInitials(task.assignee) }}</span
        >
        {{ formatMember(task.assignee) }}
      </span>
      <span
        v-if="visibleFields?.isVisible('due_date') && dueInfo.label"
        class="task-meta__due"
        :class="{ 'is-overdue': dueInfo.overdue }"
        >{{ dueInfo.label }}</span
      >
      <span v-if="visibleFields?.isVisible('modified') && modifiedInfo" class="muted">{{ modifiedInfo }}</span>
      <span
        v-if="attachments.length"
        class="muted task-meta__attachments"
        :title="attachments.map((a) => a.name).join('\n')"
        :aria-label="`${attachments.length} ${attachments.length === 1 ? 'attachment' : 'attachments'}`"
      >
        <IconGlyph name="paperclip" />{{ attachments.length }}
      </span>
      <template v-if="visibleFields?.isVisible('tags')">
        <span v-for="tag in task.tags || []" :key="tag" class="tag" v-bind="tagAttrs(task.id, tag)">{{ tag }}</span>
      </template>
      <span v-for="cf in customEntries" :key="`cf-${task.id}-${cf.key}`" class="muted task-meta__custom-field">
        <span class="muted task-meta__custom-field__label">{{ cf.label }}:</span>
        <span class="task-meta__custom-field__value">{{ cf.value }}</span>
      </span>
    </div>
    <div v-if="showSprints" class="row task-meta__sprints">
      <span
        v-for="sprintId in task.sprints"
        :key="`${task.id}-sprint-${sprintId}`"
        class="chip small sprint-chip"
        :class="sprint.stateClass(sprintId)"
        :title="sprint.tooltip(sprintId)"
        >{{ sprint.label(sprintId) }}</span
      >
    </div>
  </footer>
</template>

<style scoped>
.task-meta__attachments {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  font-variant-numeric: tabular-nums;
}

.task-meta__attachments .icon-glyph {
  width: 13px;
  height: 13px;
}
</style>
