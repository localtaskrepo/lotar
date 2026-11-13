<template>
  <article
    class="card task"
    :class="[priorityClass(task.priority), { 'task--selected': selected, 'task--compact': compact }]"
    :draggable="!compact"
    tabindex="0"
    data-board-card
    @dragstart="emit('drag-start', task)"
    @dblclick="emit('open', task.id)"
    @click.exact="compact ? emit('open', task.id) : emit('select', task.id)"
    @keydown.enter.self.prevent="emit('open', task.id)"
  >
    <div v-if="cover" class="task-cover" aria-hidden="true">
      <AttachmentTile :attachment="cover" cover :cover-height="120" />
    </div>
    <header v-if="hasHeader || compact" class="row task-header">
      <div v-if="hasIdentity" class="row task-header__left">
        <span v-if="fields?.isVisible('id') && (task.id || '').trim()" class="muted id">{{ task.id }}</span>
        <AgentActivityIndicator :task-id="task.id" />
        <strong v-if="fields?.isVisible('title') && (task.title || '').trim()" class="title">{{ task.title }}</strong>
      </div>
      <span
        v-if="fields?.isVisible('priority') && (task.priority || '').trim()"
        class="priority"
        :class="priorityClass(task.priority)"
        >{{ task.priority }}</span
      >
      <UiButton
        v-if="compact"
        icon-only
        variant="ghost"
        type="button"
        class="task__actions"
        :aria-label="`Actions for ${task.id}`"
        :title="`Actions for ${task.id}`"
        data-testid="board-card-actions"
        @click.stop="emit('actions', task)"
      >
        <IconGlyph name="dots-horizontal" />
      </UiButton>
    </header>
    <BoardCardMeta
      :task="task"
      :has-header="hasHeader || compact"
      :due-info="dueInfo"
      :modified-info="modifiedInfo"
      :sprint="sprint"
    />
    <OpenQuestion :task="task" class="task-question" />
  </article>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { TaskDTO } from '../api/types'
import { injectColumnStore } from '../composables/useColumns'
import type { TaskAttachmentView } from '../utils/taskAttachments'
import AgentActivityIndicator from './AgentActivityIndicator.vue'
import AttachmentTile from './AttachmentTile.vue'
import BoardCardMeta from './BoardCardMeta.vue'
import IconGlyph from './IconGlyph.vue'
import OpenQuestion from './OpenQuestion.vue'
import UiButton from './UiButton.vue'

/** One board card; the board provides the card-field store (DEV-419: shared by the classic and compact boards). */
const props = defineProps<{
  task: TaskDTO
  selected: boolean
  cover: TaskAttachmentView | null
  dueInfo: { label: string; overdue: boolean }
  modifiedInfo: string
  sprint: {
    label: (id: number) => string
    stateClass: (id: number) => string
    tooltip: (id: number) => string
  }
  /** Phone layout: a tap opens the task, ⋯ opens its actions, and the card is not draggable. */
  compact?: boolean
}>()

const emit = defineEmits<{
  (event: 'open', id: string): void
  (event: 'select', id: string): void
  (event: 'drag-start', task: TaskDTO): void
  (event: 'actions', task: TaskDTO): void
}>()

const fields = injectColumnStore()

const hasIdentity = computed(() =>
  Boolean(
    (fields?.isVisible('id') && (props.task.id || '').trim()) ||
    (fields?.isVisible('title') && (props.task.title || '').trim()),
  ),
)

const hasHeader = computed(() =>
  Boolean(hasIdentity.value || (fields?.isVisible('priority') && (props.task.priority || '').trim())),
)

function priorityClass(value: string | undefined): string {
  const v = (value || '').trim().toLowerCase()
  if (v === 'critical') return 'priority--critical'
  if (v === 'high') return 'priority--high'
  if (v === 'low') return 'priority--low'
  return ''
}
</script>

<style scoped>
.task-question {
  margin-top: 6px;
}
/* The meta rows come from BoardCardMeta, so their rules reach into it with :deep. */
.task-cover {
  margin: -8px -8px 8px;
  overflow: hidden;
  border-radius: var(--radius-base) var(--radius-base) 0 0;
  background: var(--color-surface);
}
.task {
  padding: 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-base);
  cursor: grab;
  user-select: none;
}
.task:active {
  cursor: grabbing;
}
.task .id {
  font-family: var(--font-mono);
  margin-right: 6px;
  line-height: var(--line-tight);
  white-space: nowrap;
  overflow-wrap: normal;
  word-break: keep-all;
}
.task .title {
  display: block;
  min-width: 0;
  white-space: normal;
  overflow-wrap: anywhere;
  word-break: break-word;
  line-height: 1.35;
}
.priority {
  font-size: 12px;
  color: var(--muted);
  flex: 0 0 auto;
  white-space: nowrap;
}
.priority.priority--critical {
  color: var(--color-danger);
  font-weight: 600;
}
.priority.priority--high {
  color: var(--color-warning-strong);
  font-weight: 600;
}
.priority.priority--low {
  opacity: 0.6;
}
.task.priority--critical {
  border-left: 3px solid var(--color-danger);
}
.task.priority--high {
  border-left: 3px solid var(--color-warning);
}
.task-header {
  justify-content: space-between;
  gap: 6px;
  align-items: flex-start;
}

.task-header__left {
  gap: 6px;
  align-items: baseline;
  flex: 1 1 auto;
  min-width: 0;
}

.task-header__left .title {
  flex: 1 1 auto;
  min-width: 0;
}

.task-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 6px;
}

.task-meta.task-meta--no-header {
  margin-top: 0;
}

:deep(.task-meta__tags),
:deep(.task-meta__sprints),
:deep(.task-meta__custom-field) {
  gap: 6px;
  flex-wrap: wrap;
  align-items: center;
}

:deep(.task-meta__custom-field__label) {
  font-weight: 500;
}

:deep(.task-meta__custom-field__value) {
  color: var(--fg);
}

:deep(.task-meta__due) {
  font-size: 12px;
  color: var(--muted);
}

:deep(.task-meta__due.is-overdue) {
  color: var(--color-danger-strong);
  font-weight: 600;
}

:deep(.chip.sprint-chip) {
  font-size: var(--text-xs, 0.75rem);
  padding: calc(var(--space-1, 0.25rem)) var(--space-2, 0.5rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 85%, transparent);
  border-radius: var(--radius-pill);
}

:deep(.chip.sprint--active) {
  background: color-mix(in oklab, var(--color-accent) 18%, transparent);
  color: var(--color-accent-strong);
}

:deep(.chip.sprint--overdue) {
  background: color-mix(in oklab, var(--color-danger) 18%, transparent);
  color: var(--color-danger);
}

:deep(.chip.sprint--complete) {
  background: color-mix(in oklab, var(--color-success) 18%, transparent);
  color: var(--color-success-strong);
}

:deep(.chip.sprint--pending),
:deep(.chip.sprint--unknown) {
  background: color-mix(in oklab, var(--color-muted) 18%, transparent);
  color: var(--color-muted);
}

/* -- Ticket selection --------------------------------------------------- */
.task--selected {
  outline: 2px solid var(--color-accent);
  outline-offset: -1px;
  background: color-mix(in oklab, var(--color-accent) 6%, var(--bg));
}

:deep(.member-badge) {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  font-weight: 700;
  /* Member hues are fixed mid-tones (utils/member.ts), so white text stays legible in every theme. */
  color: #fff;
  flex-shrink: 0;
  line-height: 1;
}

:deep(.member-badge.small) {
  width: 18px;
  height: 18px;
  font-size: 9px;
}

:deep(.member-inline) {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: inherit;
  color: var(--muted);
}

/* -- Compact phone card (DEV-419) --------------------------------------- */
.task--compact {
  cursor: pointer;
}

.task--compact .task-header__left {
  flex-wrap: wrap;
  row-gap: 2px;
}

/* The title gets its own full-width line under the ID. */
.task--compact .task-header__left .title {
  flex-basis: 100%;
}

.task--compact .task-header {
  align-items: flex-start;
}

.task__actions {
  margin: -6px -6px 0 0;
  flex: 0 0 auto;
}
</style>
