<template>
  <div :class="rootClasses">
    <slot />
    <div :class="cardClasses" role="tooltip">
      <header class="task-hover-card__header">
        <span class="task-hover-card__id">{{ task.id }}</span>
        <span class="task-hover-card__title">{{ task.title }}</span>
      </header>
      <div v-if="summary" class="task-hover-card__summary">
        <MarkdownContent :source="summary" />
      </div>
      <dl class="task-hover-card__meta">
        <div v-if="task.status" class="task-hover-card__row">
          <dt>Status</dt>
          <dd>{{ task.status }}</dd>
        </div>
        <div v-if="task.priority" class="task-hover-card__row">
          <dt>Priority</dt>
          <dd>{{ task.priority }}</dd>
        </div>
        <div v-if="task.assignee" class="task-hover-card__row">
          <dt>Assignee</dt>
          <dd>@{{ task.assignee }}</dd>
        </div>
        <div v-if="dueInfo" class="task-hover-card__row">
          <dt>Due</dt>
          <dd :class="['task-hover-card__due', dueInfo.tone && `is-${dueInfo.tone}`]">
            <span>{{ dueInfo.label }}</span>
            <span v-if="dueInfo.context" class="task-hover-card__due-context">{{ dueInfo.context }}</span>
          </dd>
        </div>
      </dl>
      <div v-if="tags.length" class="task-hover-card__tags">
        <span v-for="tag in tags" :key="tag" class="chip small">{{ tag }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { TaskDTO } from '../api/types';
import { parseTaskDate, startOfLocalDay } from '../utils/date';
import MarkdownContent from './MarkdownContent.vue';

const props = withDefaults(defineProps<{
  task: TaskDTO
  placement?: 'left' | 'right'
  block?: boolean
}>(), {
  placement: 'left',
  block: false,
})

const rootClasses = computed(() => ({
  'task-hover': true,
  'task-hover--block': props.block,
}))

const cardClasses = computed(() => [
  'task-hover-card',
  props.placement === 'right' ? 'task-hover-card--right' : 'task-hover-card--left',
])

const tags = computed(() => {
  return (props.task.tags || []).slice(0, 12)
})

const summary = computed(() => {
  const subtitle = (props.task.subtitle || '').trim()
  if (subtitle) {
    return subtitle.length > 160 ? `${subtitle.slice(0, 157).trimEnd()}…` : subtitle
  }
  const description = (props.task.description || '').split('\n').find(line => line.trim().length > 0)?.trim() || ''
  if (!description) return ''
  return description.length > 160 ? `${description.slice(0, 157).trimEnd()}…` : description
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
  z-index: 40;
  width: max-content;
  min-width: 260px;
  max-width: clamp(260px, 40vw, 360px);
  padding: 12px;
  border-radius: 10px;
  border: 1px solid color-mix(in oklab, var(--border, #e2e8f0) 80%, transparent);
  background: var(--surface-contrast, #ffffff);
  box-shadow: var(--shadow-md, 0 10px 30px rgba(15, 23, 42, 0.16));
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
  transform: translateY(4px);
  transition: opacity 120ms ease, visibility 120ms ease, transform 120ms ease;
  color: var(--fg, #0f172a);
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

.task-hover-card__header {
  display: flex;
  gap: 8px;
  align-items: baseline;
  margin-bottom: 6px;
}

.task-hover-card__id {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  font-size: 0.72rem;
  color: var(--color-muted, #64748b);
}

.task-hover-card__title {
  font-weight: 600;
  font-size: 0.95rem;
  line-height: 1.35;
}

.task-hover-card__summary {
  margin: 0 0 8px;
  color: color-mix(in oklab, var(--color-muted, #64748b) 75%, transparent);
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
  color: color-mix(in oklab, var(--color-muted, #64748b) 80%, transparent);
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
  color: var(--color-danger, #ef4444);
}

.task-hover-card__due.is-due-today {
  color: var(--color-warning, #f59e0b);
}

.task-hover-card__due.is-soon {
  color: var(--color-accent, #0ea5e9);
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
