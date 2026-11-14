<template>
  <div class="task-panel__tab-panel">
    <div class="task-panel__tab-actions">
      <ReloadButton
        class="task-panel__tab-action"
        variant="ghost"
        :disabled="mode !== 'edit' || loading"
        :loading="loading"
        label="Reload timeline"
        title="Reload timeline"
        @click="load"
      />
    </div>
    <template v-if="mode === 'edit'">
      <TaskPanelActivityFilter
        v-if="filterGroups.length"
        label="Filter timeline"
        :groups="filterGroups"
        :hidden-text="hiddenText"
        @toggle="onToggle"
        @show-all="filter.showAll"
      />
      <div class="task-panel__history-scroll" role="region" aria-label="Task timeline">
        <UiLoader v-if="loading && !entries.length" size="sm">Loading timeline…</UiLoader>
        <p v-else-if="error" class="error">{{ error }}</p>
        <ul v-else class="task-panel__history-list timeline">
          <li
            v-for="item in visibleEntries"
            :key="item.key"
            class="task-panel__history-item timeline__item"
            :class="`timeline__item--${item.entry.kind}`"
            :data-kind="item.entry.kind"
          >
            <div class="task-panel__history-meta">
              <span class="timeline__kind">{{ KIND_LABELS[item.entry.kind] }}</span>
              <time :datetime="item.entry.at">{{ formatDate(item.entry.at) }}</time>
              <span v-if="item.author" class="task-panel__history-actor">{{ item.author }}</span>
            </div>

            <ul v-if="item.entry.kind === 'change'" class="task-panel__history-changes">
              <li v-for="(change, idx) in item.changes" :key="idx" class="task-panel__history-change">
                <strong>{{ formatFieldName(change.field) }}</strong>
                <span class="task-panel__history-change-values">
                  <span v-if="formatChangeValue(change.old)" class="task-panel__history-old">{{
                    formatChangeValue(change.old)
                  }}</span>
                  <span v-if="formatChangeValue(change.old) && formatChangeValue(change.new)" aria-hidden="true"
                    >→</span
                  >
                  <span v-if="formatChangeValue(change.new)" class="task-panel__history-new">{{
                    formatChangeValue(change.new)
                  }}</span>
                  <span v-else-if="!formatChangeValue(change.old)" class="task-panel__history-new">Set</span>
                </span>
              </li>
            </ul>

            <MarkdownContent
              v-else-if="item.entry.kind === 'comment'"
              class="timeline__text"
              :class="{ 'timeline__text--muted': isAutomation(item.entry.comment.author) }"
              :source="item.entry.comment.text"
              :attachment-task-id="taskId"
              task-references
            />

            <div v-else-if="item.entry.kind === 'commit'" class="timeline__commit">
              <span class="task-panel__history-commit">{{ formatCommit(item.entry.commit) }}</span>
              <span class="task-panel__history-message">{{ item.entry.message }}</span>
            </div>

            <div v-else-if="item.entry.kind === 'job'" class="timeline__job" data-testid="timeline-job">
              <div class="timeline__job-line">
                <span class="timeline__status" :class="`timeline__status--${item.entry.job.status}`">{{
                  item.entry.job.status
                }}</span>
                <span class="timeline__job-settings">{{ jobSettings(item.entry.job) }}</span>
                <span v-if="jobDuration(item.entry.job, now)" class="timeline__job-detail">{{
                  jobDuration(item.entry.job, now)
                }}</span>
                <span
                  v-if="item.entry.job.usage?.cost_usd != null"
                  class="timeline__job-detail"
                  :title="usageDetail(item.entry.job.usage) || undefined"
                  >{{ formatCost(item.entry.job.usage?.cost_usd ?? 0) }}</span
                >
              </div>
              <MarkdownContent
                v-if="item.entry.job.summary || item.entry.job.last_message"
                class="timeline__text"
                :class="{ 'timeline__text--clamped': !openThreads[item.entry.job.id] }"
                :source="item.entry.job.summary || item.entry.job.last_message || ''"
                task-references
              />
              <button
                type="button"
                class="timeline__toggle"
                :aria-expanded="!!openThreads[item.entry.job.id]"
                @click="toggleThread(item.entry.job)"
              >
                {{ openThreads[item.entry.job.id] ? 'Hide thread' : 'Show thread' }}
              </button>
              <div v-if="openThreads[item.entry.job.id]" class="timeline__thread">
                <UiLoader v-if="threadLoading[item.entry.job.id]" size="sm">Loading thread…</UiLoader>
                <p v-else-if="threadErrors[item.entry.job.id]" class="error">{{ threadErrors[item.entry.job.id] }}</p>
                <p v-else-if="!(threads[item.entry.job.id] || []).length" class="muted">The thread is empty.</p>
                <AgentJobLog v-else :entries="threads[item.entry.job.id] || []" />
              </div>
            </div>
          </li>
          <li v-if="!loading && !entries.length" class="muted">Nothing happened yet</li>
          <li v-else-if="entries.length && !visibleEntries.length" class="muted">Everything is hidden by the filter</li>
        </ul>
      </div>
    </template>
    <p v-else class="task-panel__empty-hint">The timeline appears after the task is created.</p>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { api } from '../../api/client'
import type { AgentJob, AgentJobLogEntry, TaskHistoryChange, TaskTimelineEntry } from '../../api/types'
import { TIMELINE_FILTER_KEY, useActivityFilter } from '../../composables/task-panel/useActivityFilter'
import { useSse } from '../../composables/useSse'
import { formatCost, jobDuration, runSettings, usageDetail } from '../../utils/agentJobs'
import AgentJobLog from '../AgentJobLog.vue'
import MarkdownContent from '../MarkdownContent.vue'
import ReloadButton from '../ReloadButton.vue'
import UiLoader from '../UiLoader.vue'
import TaskPanelActivityFilter, { type ActivityFilterGroup } from './TaskPanelActivityFilter.vue'

const props = defineProps<{
  mode: 'create' | 'edit'
  taskId: string
  /** Changes with the task's history and comments: the timeline reloads then. */
  revision?: string
  formatDate: (input: string) => string
  formatFieldName: (field: string) => string
  formatChangeValue: (value?: string | null) => string
  formatCommit: (value: string) => string
}>()

type Kind = TaskTimelineEntry['kind']

const KIND_LABELS: Record<Kind, string> = {
  change: 'Change',
  comment: 'Comment',
  commit: 'Commit',
  job: 'Agent run',
}

const KIND_FILTERS: Array<{ value: Kind; label: string }> = [
  { value: 'change', label: 'Changes' },
  { value: 'comment', label: 'Comments' },
  { value: 'commit', label: 'Commits' },
  { value: 'job', label: 'Agent runs' },
]

/** Field groups changes filter by; every other field counts as "other". */
const FIELD_GROUPS = [
  { value: 'status', label: 'Status' },
  { value: 'assignee', label: 'Assignee' },
  { value: 'tags', label: 'Tags' },
  { value: 'other', label: 'Other' },
]

const fieldGroup = (change: TaskHistoryChange) =>
  FIELD_GROUPS.some((group) => group.value === change.field) ? change.field : 'other'

// Kinds share the stored "fields" set under a prefix, so one preference holds the whole filter.
const kindKey = (kind: Kind) => `kind:${kind}`

const filter = useActivityFilter(TIMELINE_FILTER_KEY)

const entries = ref<TaskTimelineEntry[]>([])
const loading = ref(false)
const error = ref('')
let generation = 0

async function load() {
  if (props.mode !== 'edit' || !props.taskId) return
  const request = ++generation
  loading.value = true
  try {
    const timeline = await api.taskTimeline(props.taskId)
    if (request !== generation) return
    entries.value = timeline.entries
    error.value = ''
  } catch (err) {
    if (request !== generation) return
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    if (request === generation) loading.value = false
  }
}

watch(
  () => [props.taskId, props.revision, props.mode],
  () => void load(),
  { immediate: true },
)

function authorOf(entry: TaskTimelineEntry): string {
  switch (entry.kind) {
    case 'change':
      return entry.actor || ''
    case 'comment':
      return entry.comment.author || ''
    case 'commit':
      return entry.author
    case 'job':
      return entry.job.agent ? `@${entry.job.agent}` : entry.job.runner
  }
}

const isAutomation = (author?: string | null) => author === 'automation'

/** Newest first, each change entry with only its shown fields; entries left empty drop out. */
const visibleEntries = computed(() =>
  entries.value
    .map((entry, index) => ({
      key: `${entry.kind}-${entry.at}-${index}`,
      entry,
      author: authorOf(entry),
      changes:
        entry.kind === 'change'
          ? entry.changes.filter((change) => !filter.hiddenFields.value.has(fieldGroup(change)))
          : [],
    }))
    .filter((item) => !filter.hiddenFields.value.has(kindKey(item.entry.kind)))
    .filter((item) => !filter.hiddenAuthors.value.has(item.author))
    .filter((item) => item.entry.kind !== 'change' || item.changes.length)
    .reverse(),
)

const filterGroups = computed<ActivityFilterGroup[]>(() => {
  const groups: ActivityFilterGroup[] = []
  const kinds = new Set(entries.value.map((entry) => entry.kind))
  const kindItems = KIND_FILTERS.filter((kind) => kinds.has(kind.value)).map((kind) => ({
    value: kindKey(kind.value),
    label: kind.label,
    hidden: filter.hiddenFields.value.has(kindKey(kind.value)),
  }))
  if (kindItems.length) groups.push({ id: 'kind', name: 'Show', items: kindItems })
  const fields = new Set(
    entries.value.flatMap((entry) => (entry.kind === 'change' ? entry.changes.map(fieldGroup) : [])),
  )
  const fieldItems = FIELD_GROUPS.filter((group) => fields.has(group.value)).map((group) => ({
    ...group,
    hidden: filter.hiddenFields.value.has(group.value),
  }))
  if (fieldItems.length) groups.push({ id: 'field', name: 'Field', items: fieldItems })
  const authors = [...new Set(entries.value.map(authorOf))]
  const authorItems = authors.map((author) => ({
    value: author,
    label: author || 'Unknown',
    hidden: filter.hiddenAuthors.value.has(author),
  }))
  if (authorItems.length) groups.push({ id: 'author', name: 'Author', items: authorItems })
  return groups
})

const hiddenText = computed(() => {
  const hidden = entries.value.length - visibleEntries.value.length
  if (!hidden) return ''
  const total = entries.value.length
  return `${hidden} of ${total} ${total === 1 ? 'entry' : 'entries'} hidden.`
})

function onToggle(group: string, value: string) {
  if (group === 'kind' || group === 'field') filter.toggleField(value)
  else filter.toggleAuthor(value)
}

const jobSettings = (job: AgentJob) => [job.runner, runSettings(job.model, job.effort)].filter(Boolean).join(' · ')

// Threads load when opened and follow a running job's events.
const openThreads = ref<Record<string, boolean>>({})
const threads = ref<Record<string, AgentJobLogEntry[]>>({})
const threadLoading = ref<Record<string, boolean>>({})
const threadErrors = ref<Record<string, string>>({})

async function toggleThread(job: AgentJob) {
  openThreads.value = { ...openThreads.value, [job.id]: !openThreads.value[job.id] }
  if (!openThreads.value[job.id]) return
  threadLoading.value = { ...threadLoading.value, [job.id]: true }
  try {
    const response = await api.getAgentJobLogs(job.id)
    mergeThread(job.id, response.events)
    threadErrors.value = { ...threadErrors.value, [job.id]: '' }
  } catch (err) {
    threadErrors.value = { ...threadErrors.value, [job.id]: err instanceof Error ? err.message : String(err) }
  } finally {
    threadLoading.value = { ...threadLoading.value, [job.id]: false }
  }
}

function mergeThread(id: string, incoming: AgentJobLogEntry[]) {
  const bySequence = new Map((threads.value[id] || []).map((entry) => [entry.sequence, entry]))
  for (const entry of incoming) bySequence.set(entry.sequence, entry)
  threads.value = { ...threads.value, [id]: [...bySequence.values()].sort((a, b) => a.sequence - b.sequence) }
}

const JOB_EVENTS = [
  'agent_job_started',
  'agent_job_init',
  'agent_job_progress',
  'agent_job_message',
  'agent_job_input',
  'agent_job_question',
  'agent_job_result',
  'agent_job_completed',
  'agent_job_failed',
  'agent_job_cancelled',
]
const JOB_ENDS = new Set(['agent_job_started', 'agent_job_completed', 'agent_job_failed', 'agent_job_cancelled'])

const isOwnJob = (ticketId?: string) => !!ticketId && ticketId === props.taskId

let sse: ReturnType<typeof useSse> | null = null
const unsubscribers: Array<() => void> = []
const now = ref(Date.now())
let clock: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  sse = useSse('/api/events', { kinds: JOB_EVENTS.join(',') })
  for (const kind of JOB_EVENTS) {
    const handler = (ev: MessageEvent) => {
      let payload: { id?: string; ticket_id?: string; sequence?: number; at?: string; message?: string | null }
      try {
        payload = JSON.parse(String(ev.data || '{}'))
      } catch {
        return
      }
      if (!payload.id || !isOwnJob(payload.ticket_id)) return
      if (threads.value[payload.id] && typeof payload.sequence === 'number' && payload.at) {
        mergeThread(payload.id, [
          { kind, sequence: payload.sequence, at: payload.at, message: payload.message ?? null },
        ])
      }
      // A job starting or ending changes the run's status, summary and cost.
      if (JOB_ENDS.has(kind)) void load()
    }
    sse.on(kind, handler)
    unsubscribers.push(() => sse?.off(kind, handler))
  }
  clock = setInterval(() => {
    if (entries.value.some((entry) => entry.kind === 'job' && entry.job.status === 'running')) now.value = Date.now()
  }, 1000)
})

onBeforeUnmount(() => {
  unsubscribers.splice(0).forEach((off) => off())
  sse?.close()
  if (clock) clearInterval(clock)
})
</script>

<style scoped>
.timeline__kind {
  font-size: var(--text-xs, 0.75rem);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: var(--color-muted);
}

.timeline__item--job .timeline__kind {
  color: var(--color-accent-strong);
}

.timeline__text {
  font-size: var(--text-sm, 0.875rem);
  overflow-wrap: anywhere;
}

.timeline__text--muted {
  color: var(--color-muted);
}

.timeline__text--clamped {
  display: -webkit-box;
  -webkit-line-clamp: 4;
  line-clamp: 4;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.timeline__commit {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2, 0.5rem);
  align-items: baseline;
}

.timeline__item,
.timeline__job {
  min-width: 0;
}

.timeline__job {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: var(--space-1, 0.25rem);
}

.timeline__job-line {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2, 0.5rem);
  align-items: baseline;
  font-size: var(--text-sm, 0.875rem);
}

.timeline__job-settings,
.timeline__job-detail {
  color: var(--color-muted);
}

.timeline__status {
  font-size: var(--text-xs, 0.75rem);
  font-weight: 600;
  padding: 0 var(--space-2, 0.5rem);
  border-radius: 999px;
  border: 1px solid var(--color-border);
}

.timeline__status--running,
.timeline__status--queued {
  color: var(--color-accent-strong);
  border-color: color-mix(in oklab, var(--color-accent) 50%, var(--color-border));
}

.timeline__status--completed {
  color: var(--color-success);
  border-color: color-mix(in oklab, var(--color-success) 50%, var(--color-border));
}

.timeline__status--failed,
.timeline__status--cancelled {
  color: var(--color-danger);
  border-color: color-mix(in oklab, var(--color-danger) 50%, var(--color-border));
}

.timeline__toggle {
  justify-self: start;
  border: 0;
  padding: 0;
  background: none;
  font: inherit;
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-accent);
  cursor: pointer;
  text-decoration: underline;
}

.timeline__thread {
  border-top: 1px solid var(--color-border);
  padding-top: var(--space-2, 0.5rem);
}
</style>
