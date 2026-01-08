<template>
  <div
    ref="triggerRef"
    :class="rootClasses"
    @mouseenter="handleTriggerEnter"
    @mouseleave="handleTriggerLeave"
    @focusin="handleTriggerEnter"
    @focusout="handleTriggerLeave"
  >
    <slot />

    <Teleport v-if="teleportToBody" to="body">
      <div
        ref="cardRef"
        :class="cardClasses"
        role="tooltip"
        :style="teleportStyle"
        :aria-hidden="open ? 'false' : 'true'"
        @mouseenter="handleCardEnter"
        @mouseleave="handleCardLeave"
        @focusin="handleCardEnter"
        @focusout="handleCardLeave"
      >
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
            <dd>@{{ task.assignee }}</dd>
          </div>
          <div v-if="showReporter && task.reporter" class="task-hover-card__row">
            <dt>Reporter</dt>
            <dd>@{{ task.reporter }}</dd>
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
        </dl>
        <div v-if="showTags && tags.length" class="task-hover-card__tags">
          <span v-for="tag in tags" :key="tag" class="tag">{{ tag }}</span>
        </div>
      </div>
    </Teleport>

    <div v-else :class="cardClasses" role="tooltip">
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
          <dd>@{{ task.assignee }}</dd>
        </div>
        <div v-if="showReporter && task.reporter" class="task-hover-card__row">
          <dt>Reporter</dt>
          <dd>@{{ task.reporter }}</dd>
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
      </dl>
      <div v-if="showTags && tags.length" class="task-hover-card__tags">
        <span v-for="tag in tags" :key="tag" class="tag">{{ tag }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue';
import type { TaskDTO } from '../api/types';
import { parseTaskDate, startOfLocalDay } from '../utils/date';

const props = withDefaults(defineProps<{
  task: TaskDTO
  placement?: 'left' | 'right'
  block?: boolean
  teleportToBody?: boolean
  fields?: Partial<Record<'id' | 'title' | 'status' | 'priority' | 'task_type' | 'reporter' | 'assignee' | 'effort' | 'tags' | 'sprints' | 'due_date' | 'modified', boolean>>
}>(), {
  placement: 'left',
  block: false,
  teleportToBody: false,
})

const triggerRef = ref<HTMLElement | null>(null)
const cardRef = ref<HTMLElement | null>(null)
const open = ref(false)
const hoveringTrigger = ref(false)
const hoveringCard = ref(false)
let closeTimer: number | null = null

const teleportToBody = computed(() => Boolean(props.teleportToBody))

const teleportPos = ref<{ top: number; left: number }>({ top: 0, left: 0 })

const teleportStyle = computed(() => {
  if (!teleportToBody.value) return undefined
  return {
    top: `${teleportPos.value.top}px`,
    left: `${teleportPos.value.left}px`,
  } as Record<string, string>
})

function clearCloseTimer() {
  if (closeTimer !== null) {
    window.clearTimeout(closeTimer)
    closeTimer = null
  }
}

function scheduleClose() {
  if (!teleportToBody.value) return
  clearCloseTimer()
  closeTimer = window.setTimeout(() => {
    if (!hoveringTrigger.value && !hoveringCard.value) {
      open.value = false
    }
  }, 80)
}

function updateTeleportPosition() {
  if (!teleportToBody.value) return
  const trigger = triggerRef.value
  const card = cardRef.value
  if (!trigger || !card) return

  const margin = 8
  const triggerRect = trigger.getBoundingClientRect()
  const cardRect = card.getBoundingClientRect()

  // Prefer placing beside the trigger so it doesn't cover list items above/below.
  // `placement` here is interpreted as which side has room:
  // - 'left' means "open to the right" (common when trigger is on the left half)
  // - 'right' means "open to the left" (common when trigger is on the right half)
  const preferRight = props.placement === 'left'
  const rightX = triggerRect.right + margin
  const leftX = triggerRect.left - margin - cardRect.width

  let left = preferRight ? rightX : leftX
  if (preferRight && left + cardRect.width > window.innerWidth - margin) {
    left = leftX
  } else if (!preferRight && left < margin) {
    left = rightX
  }
  left = Math.min(Math.max(margin, left), window.innerWidth - margin - cardRect.width)

  let top = triggerRect.top
  top = Math.min(Math.max(margin, top), window.innerHeight - margin - cardRect.height)

  teleportPos.value = { top, left }
}

async function handleTriggerEnter() {
  if (!teleportToBody.value) return
  hoveringTrigger.value = true
  clearCloseTimer()
  open.value = true
  await nextTick()
  updateTeleportPosition()
}

function handleTriggerLeave() {
  if (!teleportToBody.value) return
  hoveringTrigger.value = false
  scheduleClose()
}

function handleCardEnter() {
  if (!teleportToBody.value) return
  hoveringCard.value = true
  clearCloseTimer()
}

function handleCardLeave() {
  if (!teleportToBody.value) return
  hoveringCard.value = false
  scheduleClose()
}

function handleWindowChange() {
  if (!teleportToBody.value) return
  if (!open.value) return
  updateTeleportPosition()
}

onMounted(() => {
  window.addEventListener('resize', handleWindowChange)
  window.addEventListener('scroll', handleWindowChange, true)
})

onUnmounted(() => {
  window.removeEventListener('resize', handleWindowChange)
  window.removeEventListener('scroll', handleWindowChange, true)
  clearCloseTimer()
})

function isFieldVisible(key: 'id' | 'title' | 'status' | 'priority' | 'task_type' | 'reporter' | 'assignee' | 'effort' | 'tags' | 'sprints' | 'due_date' | 'modified') {
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

const rootClasses = computed(() => ({
  'task-hover': true,
  'task-hover--block': props.block,
}))

const cardClasses = computed(() => [
  'task-hover-card',
  teleportToBody.value
    ? 'task-hover-card--teleport'
    : (props.placement === 'right' ? 'task-hover-card--right' : 'task-hover-card--left'),
  teleportToBody.value && open.value ? 'is-open' : null,
])

const tags = computed(() => {
  return (props.task.tags || []).slice(0, 12)
})

const relativeTimeFormatter =
  typeof Intl !== 'undefined' && (Intl as any).RelativeTimeFormat
    ? new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
    : null

const relativeUnits: Array<{ unit: Intl.RelativeTimeFormatUnit; ms: number }> = [
  { unit: 'year', ms: 1000 * 60 * 60 * 24 * 365 },
  { unit: 'month', ms: 1000 * 60 * 60 * 24 * 30 },
  { unit: 'week', ms: 1000 * 60 * 60 * 24 * 7 },
  { unit: 'day', ms: 1000 * 60 * 60 * 24 },
  { unit: 'hour', ms: 1000 * 60 * 60 },
  { unit: 'minute', ms: 1000 * 60 },
  { unit: 'second', ms: 1000 },
]

function relativeTime(value: string) {
  if (!value) return ''
  const target = new Date(value)
  const timestamp = target.getTime()
  if (!isFinite(timestamp)) return value
  const diff = timestamp - Date.now()
  if (!relativeTimeFormatter) return target.toLocaleString()
  for (const { unit, ms } of relativeUnits) {
    if (Math.abs(diff) >= ms || unit === 'second') {
      const amount = Math.round(diff / ms)
      return relativeTimeFormatter.format(amount, unit)
    }
  }
  return target.toLocaleString()
}

const modifiedInfo = computed(() => {
  const raw = (props.task.modified || '').trim()
  if (!raw) return null
  let parsed: Date | null = null
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
  let context = ''
  let tone: 'overdue' | 'due-today' | 'soon' | null = null
  if (diffDays < 0) {
    context = `${Math.abs(diffDays)} day${Math.abs(diffDays) === 1 ? '' : 's'} ago`
    tone = 'overdue'
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
  const label = parsed.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' })
  return { label, context, tone }
})

</script>

<style scoped>
.task-hover {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  min-width: 0;
  min-height: 22px;
}

.task-hover--block {
  display: block;
}

.task-hover-card {
  position: absolute;
  top: calc(100% + 8px);
  z-index: var(--z-tooltip);
  width: max-content;
  min-width: 260px;
  max-width: clamp(260px, 40vw, 360px);
  padding: 12px;
  border-radius: var(--radius-popover);
  border: 1px solid color-mix(in oklab, var(--color-border) 80%, transparent);
  background: var(--color-bg);
  box-shadow: var(--shadow-popover);
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
  transform: translateY(4px);
  transition: opacity var(--duration-fast) var(--ease-standard), visibility var(--duration-fast) var(--ease-standard), transform var(--duration-fast) var(--ease-standard);
  color: var(--color-fg);
}

.task-hover-card--left {
  left: 0;
}

.task-hover-card--right {
  right: 0;
}

.task-hover:hover .task-hover-card,
.task-hover:focus-within .task-hover-card {
  opacity: 1;
  visibility: visible;
  pointer-events: auto;
  transform: translateY(0);
}

.task-hover-card.is-open {
  opacity: 1;
  visibility: visible;
  pointer-events: auto;
  transform: translateY(0);
}

.task-hover-card--teleport {
  position: fixed;
  left: 0;
  right: auto;
  z-index: var(--z-modal-high);
  pointer-events: none;
}

.task-hover-card--teleport.is-open {
  pointer-events: none;
}

.task-hover-card__header {
  display: flex;
  gap: 8px;
  align-items: baseline;
  margin-bottom: 6px;
}

.task-hover-card__id {
  flex: 0 0 auto;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
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

.task-hover-card__subtitle {
  margin: 0 0 8px;
  color: color-mix(in oklab, var(--color-muted) 75%, transparent);
  font-size: 0.78rem;
  line-height: 1.4;
}

.task-hover-card__summary {
  margin: 0 0 8px;
  color: color-mix(in oklab, var(--color-muted) 75%, transparent);
  font-size: 0.78rem;
  line-height: 1.4;
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

@media (max-width: 720px) {
  .task-hover-card {
    max-width: min(90vw, 320px);
  }
}
</style>
