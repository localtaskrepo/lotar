<script setup lang="ts">
import { computed } from 'vue'
import type { TaskDTO } from '../api/types'
import { customFieldEntriesFromTask } from '../composables/useColumns'
import { parseTaskDate, startOfLocalDay } from '../utils/date'
import { formatMember } from '../utils/member'

const props = defineProps<{
    task: TaskDTO
    fields?: Partial<Record<string, boolean>>
}>()

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

const customFieldEntries = computed(() => {
    const enabled = Object.entries(props.fields ?? {})
        .filter(([, on]) => on)
        .map(([key]) => key)
    return customFieldEntriesFromTask(props.task, enabled)
})

const tags = computed(() => (props.task.tags || []).slice(0, 12))

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

<template>
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
    </dl>
    <div v-if="showTags && tags.length" class="task-hover-card__tags">
        <span v-for="tag in tags" :key="tag" class="tag">{{ tag }}</span>
    </div>
</template>

<style scoped>
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
