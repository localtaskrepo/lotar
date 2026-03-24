<template>
  <section class="col" style="gap: 16px;">
    <header class="row" style="align-items: flex-start; justify-content: space-between;">
      <div class="col" style="gap: 4px;">
        <h1 style="margin: 0;">Agent jobs</h1>
        <p class="muted" style="margin: 0;">Track active and recent agent runs grouped by ticket.</p>
      </div>
      <div class="row" style="gap: 8px;">
        <UiButton
          variant="danger"
          type="button"
          :disabled="loading || !hasCancelableJobs"
          @click="cancelAll"
        >
          Stop all
        </UiButton>
        <ReloadButton
          :disabled="loading"
          :loading="loading"
          label="Refresh jobs"
          title="Refresh jobs"
          variant="ghost"
          @click="refresh"
        />
      </div>
    </header>

    <!-- Queue Stats -->
    <UiCard v-if="queueStats" class="queue-stats-card">
      <div class="row" style="gap: 24px; align-items: center; justify-content: flex-start;">
        <div class="stat-item">
          <span class="stat-value running">{{ queueStats.running }}</span>
          <span class="stat-label">Running</span>
        </div>
        <div class="stat-item">
          <span class="stat-value queued">{{ queueStats.queued }}</span>
          <span class="stat-label">Queued</span>
        </div>
        <div v-if="queueStats.max_parallel" class="stat-item">
          <span class="stat-value">{{ queueStats.max_parallel }}</span>
          <span class="stat-label">Max parallel</span>
        </div>
        <div v-else class="stat-item">
          <span class="stat-value muted">∞</span>
          <span class="stat-label">Max parallel</span>
        </div>
      </div>
    </UiCard>

    <!-- Filter tabs -->
    <div class="row" style="gap: 8px; border-bottom: 1px solid var(--border); padding-bottom: 8px;">
      <button
        v-for="tab in tabs"
        :key="tab.value"
        class="filter-tab"
        :class="{ active: activeTab === tab.value }"
        type="button"
        @click="activeTab = tab.value"
      >
        {{ tab.label }}
        <span v-if="tab.count !== undefined" class="tab-count">{{ tab.count }}</span>
      </button>
    </div>

    <div v-if="loading" class="muted">Loading jobs…</div>
    <div v-else-if="error" class="error">{{ error }}</div>

    <UiEmptyState
      v-else-if="!filteredJobs.length"
      title="No agent jobs yet"
      :description="activeTab === 'all' ? 'Jobs are usually started from task automation or the CLI and appear here automatically.' : `No ${activeTab} jobs.`"
    />

    <div v-else class="col" style="gap: 12px;">
      <UiCard v-for="group in groupedJobs" :key="group.ticketId" class="ticket-group-card">
        <div class="ticket-group-card__header row" style="justify-content: space-between; align-items: flex-start; gap: 16px;">
          <div class="col" style="gap: 6px; flex: 1 1 auto;">
            <div class="row" style="gap: 8px; align-items: center; flex-wrap: wrap;">
              <button class="chip" type="button" @click="openTask(group.ticketId)">
                {{ group.ticketId }}
              </button>
              <span class="muted">{{ group.jobs.length }} {{ group.jobs.length === 1 ? 'job' : 'jobs' }}</span>
            </div>
            <div class="muted" style="font-size: 12px;">
              <span>Latest {{ formatDate(group.latest.created_at) }}</span>
              <span v-if="group.latest.finished_at"> · {{ group.latest.status }}</span>
            </div>
          </div>
          <div class="row ticket-group-summary" style="gap: 8px; align-items: center; flex-wrap: wrap;">
            <span v-if="group.counts.running" class="summary-chip summary-chip-running">{{ group.counts.running }} running</span>
            <span v-if="group.counts.queued" class="summary-chip summary-chip-queued">{{ group.counts.queued }} queued</span>
            <span v-if="group.counts.completed" class="summary-chip summary-chip-completed">{{ group.counts.completed }} completed</span>
            <span v-if="group.counts.failed" class="summary-chip summary-chip-failed">{{ group.counts.failed }} failed</span>
          </div>
        </div>

        <div class="ticket-group-jobs">
          <div v-for="job in group.jobs" :key="job.id" class="job-card" :class="`job-${job.status}`">
            <div class="row" style="justify-content: space-between; align-items: flex-start; gap: 16px;">
              <div class="col" style="gap: 6px; flex: 1 1 auto;">
                <div class="row" style="gap: 8px; align-items: center; flex-wrap: wrap;">
                  <span class="status-chip" :class="`status-${job.status}`">{{ job.status }}</span>
                  <span v-if="job.agent" class="phase-chip">{{ job.agent }}</span>
                  <span class="muted">{{ job.runner }}</span>
                </div>
                <div class="muted" style="font-size: 12px;">
                  <span>Created {{ formatDate(job.created_at) }}</span>
                  <span v-if="job.started_at"> · Started {{ formatDate(job.started_at) }}</span>
                  <span v-if="job.finished_at"> · Finished {{ formatDate(job.finished_at) }}</span>
                </div>
                <div v-if="job.worktree_path" class="muted" style="font-size: 12px;">
                  <span>Worktree {{ job.worktree_path }}</span>
                  <span v-if="job.worktree_branch"> · {{ job.worktree_branch }}</span>
                </div>
                <p v-if="job.last_message" class="muted" style="margin: 0;">
                  {{ job.last_message }}
                </p>
              </div>
              <div class="row" style="gap: 8px; align-items: center;">
                <UiButton variant="ghost" type="button" @click="toggleLogs(job.id)">
                  {{ isLogsOpen(job.id) ? 'Hide logs' : 'Show logs' }}
                </UiButton>
                <UiButton
                  v-if="job.status === 'running' || job.status === 'queued'"
                  variant="danger"
                  type="button"
                  @click="cancel(job.id)"
                >
                  {{ job.status === 'queued' ? 'Remove' : 'Stop' }}
                </UiButton>
              </div>
            </div>

            <div v-if="isLogsOpen(job.id)" class="log-panel">
              <div v-if="logsLoading[job.id]" class="muted">Loading logs…</div>
              <div v-else-if="logsError[job.id]" class="error">{{ logsError[job.id] }}</div>
              <div v-else-if="(logsByJob[job.id] || []).length === 0" class="muted">
                No log entries yet.
              </div>
              <div v-else class="log-list">
                <div
                  v-for="entry in logsByJob[job.id] || []"
                  :key="entry.at + entry.kind + (entry.message || '')"
                  class="log-row"
                >
                  <span class="log-kind">{{ entry.kind }}</span>
                  <span class="log-time">{{ formatDate(entry.at) }}</span>
                  <span class="log-message">{{ formatLogMessage(entry) }}</span>
                </div>
              </div>

              <div class="row" style="gap: 8px; margin-top: 12px;">
                <UiInput
                  :model-value="messageDrafts[job.id] || ''"
                  placeholder="Send a message to the agent"
                  :disabled="job.status !== 'running'"
                  @update:modelValue="(value: string) => setMessageDraft(job.id, value)"
                />
                <UiButton
                  variant="ghost"
                  type="button"
                  :disabled="job.status !== 'running' || !messageDrafts[job.id]"
                  @click="sendMessage(job.id)"
                >
                  Send
                </UiButton>
              </div>
            </div>
          </div>
        </div>
      </UiCard>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { api } from '../api/client'
import type { AgentJob, AgentJobLogEntry, AgentQueueStats } from '../api/types'
import ReloadButton from '../components/ReloadButton.vue'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiInput from '../components/UiInput.vue'
import { useSse } from '../composables/useSse'
import { useTaskPanelController } from '../composables/useTaskPanelController'

const jobs = ref<AgentJob[]>([])
const queueStats = ref<AgentQueueStats | null>(null)
const loading = ref(false)
const error = ref('')
const activeTab = ref<'all' | 'running' | 'queued' | 'completed' | 'failed'>('all')
const { openTaskPanel } = useTaskPanelController()
const logsByJob = ref<Record<string, AgentJobLogEntry[]>>({})
const logsLoading = ref<Record<string, boolean>>({})
const logsError = ref<Record<string, string>>({})
const openLogs = ref<Record<string, boolean>>({})
const messageDrafts = ref<Record<string, string>>({})

const tabs = computed(() => [
  { value: 'all' as const, label: 'All', count: jobs.value.length },
  { value: 'running' as const, label: 'Running', count: jobs.value.filter(j => j.status === 'running').length },
  { value: 'queued' as const, label: 'Queued', count: jobs.value.filter(j => j.status === 'queued').length },
  { value: 'completed' as const, label: 'Completed', count: jobs.value.filter(j => j.status === 'completed').length },
  { value: 'failed' as const, label: 'Failed', count: jobs.value.filter(j => j.status === 'failed' || j.status === 'cancelled').length },
])

const filteredJobs = computed(() => {
  if (activeTab.value === 'all') return jobs.value
  if (activeTab.value === 'failed') return jobs.value.filter(j => j.status === 'failed' || j.status === 'cancelled')
  return jobs.value.filter(j => j.status === activeTab.value)
})

const groupedJobs = computed(() => {
  const grouped = new Map<string, AgentJob[]>()

  filteredJobs.value.forEach((job) => {
    const bucket = grouped.get(job.ticket_id) || []
    bucket.push(job)
    grouped.set(job.ticket_id, bucket)
  })

  return Array.from(grouped.entries())
    .map(([ticketId, ticketJobs]) => {
      const latest = [...ticketJobs].sort((a, b) => b.created_at.localeCompare(a.created_at))[0]
      const jobs = [...ticketJobs].sort((a, b) => a.created_at.localeCompare(b.created_at))
      return {
        ticketId,
        latest,
        jobs,
        counts: {
          running: ticketJobs.filter(job => job.status === 'running').length,
          queued: ticketJobs.filter(job => job.status === 'queued').length,
          completed: ticketJobs.filter(job => job.status === 'completed').length,
          failed: ticketJobs.filter(job => job.status === 'failed' || job.status === 'cancelled').length,
        },
      }
    })
    .sort((a, b) => b.latest.created_at.localeCompare(a.latest.created_at))
})

const hasCancelableJobs = computed(() => jobs.value.some(job => job.status === 'running' || job.status === 'queued'))

let sse: ReturnType<typeof useSse> | null = null
const sseUnsubscribers: Array<() => void> = []

async function refresh() {
  loading.value = true
  error.value = ''
  try {
    const jobsResponse = await api.listAgentJobs()
    jobs.value = sortJobs(jobsResponse.jobs)
    queueStats.value = jobsResponse.queue_stats || null
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    loading.value = false
  }
}

function updateFromEvent(payload: any, kind: string) {
  if (!payload?.id) return
  const next: AgentJob = {
    id: payload.id,
    ticket_id: payload.ticket_id || 'UNKNOWN',
    agent: payload.agent ?? null,
    runner: payload.runner || 'unknown',
    status: payload.status || 'running',
    created_at: payload.created_at || new Date().toISOString(),
    started_at: payload.started_at || null,
    finished_at: payload.finished_at || null,
    exit_code: payload.exit_code ?? null,
    last_message: payload.message || null,
    summary: payload.summary || null,
    session_id: payload.session_id || null,
    worktree_path: payload.worktree_path || null,
    worktree_branch: payload.worktree_branch || null,
  }
  mergeJob(next)
  if (kind.startsWith('agent_job_')) {
    appendLog(next.id, {
      kind,
      at: new Date().toISOString(),
      message: payload.message || null,
    })
  }
  // Update queue stats on job state changes
  if (['agent_job_started', 'agent_job_completed', 'agent_job_failed', 'agent_job_cancelled'].includes(kind)) {
    api.listAgentJobs().then(r => { queueStats.value = r.queue_stats || null }).catch(() => {})
  }
}

function mergeJob(next: AgentJob) {
  const idx = jobs.value.findIndex((job) => job.id === next.id)
  if (idx === -1) {
    jobs.value = sortJobs([...jobs.value, next])
    return
  }
  const existing = jobs.value[idx]
  const merged: AgentJob = {
    ...existing,
    ...next,
    agent: next.agent ?? existing.agent ?? null,
    last_message: next.last_message || existing.last_message,
    summary: next.summary || existing.summary,
    session_id: next.session_id || existing.session_id,
  }
  const updated = [...jobs.value]
  updated[idx] = merged
  jobs.value = sortJobs(updated)
}

function sortJobs(list: AgentJob[]): AgentJob[] {
  return [...list].sort((a, b) => b.created_at.localeCompare(a.created_at))
}

async function toggleLogs(id: string) {
  openLogs.value[id] = !openLogs.value[id]
  if (openLogs.value[id] && typeof logsByJob.value[id] === 'undefined') {
    await loadLogs(id)
  }
}

function isLogsOpen(id: string) {
  return !!openLogs.value[id]
}

async function loadLogs(id: string) {
  logsLoading.value[id] = true
  logsError.value[id] = ''
  try {
    const response = await api.getAgentJobLogs(id)
    logsByJob.value = {
      ...logsByJob.value,
      [id]: normalizeLogEntries(response.events),
    }
  } catch (err) {
    logsError.value[id] = err instanceof Error ? err.message : String(err)
  } finally {
    logsLoading.value[id] = false
  }
}

function normalizeLogEntries(entries: AgentJobLogEntry[]): AgentJobLogEntry[] {
  const normalized: AgentJobLogEntry[] = []

  for (const entry of entries) {
    const message = entry.message ?? ''
    if (!shouldDisplayLogEntry(entry)) continue

    const last = normalized[normalized.length - 1]
    if (entry.kind === 'agent_job_progress' && message.trim()) {
      if (last && last.kind === 'agent_job_progress') {
        normalized[normalized.length - 1] = {
          ...last,
          at: entry.at,
          message: `${last.message || ''}${message}`,
        }
      } else {
        normalized.push(entry)
      }
      continue
    }

    if (entry.kind === 'agent_job_message' && message.trim() && last && last.kind === 'agent_job_progress') {
      normalized[normalized.length - 1] = entry
      continue
    }

    normalized.push(entry)
  }

  return normalized
}

function appendLog(id: string, entry: AgentJobLogEntry) {
  const message = entry.message ?? ''
  if (!shouldDisplayLogEntry(entry)) return

  const existing = logsByJob.value[id] || []
  const last = existing[existing.length - 1]

  if (entry.kind === 'agent_job_progress' && message.trim()) {
    if (last && last.kind === 'agent_job_progress') {
      const updated = [...existing]
      updated[updated.length - 1] = {
        ...last,
        at: entry.at,
        message: `${last.message || ''}${message}`,
      }
      logsByJob.value = {
        ...logsByJob.value,
        [id]: updated,
      }
      return
    }
  }

  if (entry.kind === 'agent_job_message' && message.trim() && last && last.kind === 'agent_job_progress') {
    const updated = [...existing]
    updated[updated.length - 1] = entry
    logsByJob.value = {
      ...logsByJob.value,
      [id]: updated,
    }
    return
  }

  logsByJob.value = {
    ...logsByJob.value,
    [id]: [...existing, entry],
  }
}

async function cancel(id: string) {
  try {
    const response = await api.cancelAgentJob(id)
    if (response.job) {
      mergeJob(response.job)
    }
    // Refresh queue stats
    const statsResponse = await api.listAgentJobs()
    queueStats.value = statsResponse.queue_stats || null
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  }
}

async function cancelAll() {
  if (!hasCancelableJobs.value) return
  if (typeof window !== 'undefined') {
    const confirmed = window.confirm('Stop all queued and running agent jobs?')
    if (!confirmed) return
  }

  try {
    const response = await api.cancelAllAgentJobs()
    jobs.value = sortJobs(response.jobs)
    const statsResponse = await api.listAgentJobs()
    queueStats.value = statsResponse.queue_stats || null
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  }
}

function setMessageDraft(id: string, value: string) {
  messageDrafts.value = {
    ...messageDrafts.value,
    [id]: value,
  }
}

async function sendMessage(id: string) {
  const message = messageDrafts.value[id]?.trim()
  if (!message) return
  try {
    const response = await api.sendAgentJobMessage({ id, message })
    mergeJob(response.job)
    appendLog(id, { kind: 'agent_job_input', at: new Date().toISOString(), message })
    setMessageDraft(id, '')
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  }
}

function openTask(taskId: string) {
  openTaskPanel({ taskId })
}

function formatDate(value?: string | null) {
  if (!value) return 'unknown'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

function shouldDisplayLogEntry(entry: AgentJobLogEntry) {
  return Boolean((entry.message || '').trim()) || Boolean(formatLogMessage(entry))
}

function formatLogMessage(entry: AgentJobLogEntry) {
  const text = entry.message?.trim()
  if (text) return text

  switch (entry.kind) {
    case 'agent_job_started':
      return 'Job started'
    case 'agent_job_init':
      return 'Runner initialized'
    case 'agent_job_result':
      return 'Runner returned a result'
    case 'agent_job_completed':
      return 'Job completed'
    case 'agent_job_failed':
      return 'Job failed'
    case 'agent_job_cancelled':
      return 'Job cancelled'
    default:
      return ''
  }
}

function setupSse() {
  if (sse) sse.close()
  sseUnsubscribers.splice(0).forEach((fn) => fn())
  sse = useSse('/api/events', {
    kinds: [
      'agent_job_started',
      'agent_job_init',
      'agent_job_progress',
      'agent_job_message',
      'agent_job_input',
      'agent_job_result',
      'agent_job_completed',
      'agent_job_failed',
      'agent_job_cancelled',
    ].join(','),
  })

  const handlers = [
    'agent_job_started',
    'agent_job_init',
    'agent_job_progress',
    'agent_job_message',
    'agent_job_input',
    'agent_job_result',
    'agent_job_completed',
    'agent_job_failed',
    'agent_job_cancelled',
  ]

  handlers.forEach((kind) => {
    const wrapped = (ev: MessageEvent) => {
      if (!ev.data) return
      try {
        const payload = JSON.parse(ev.data)
        updateFromEvent(payload, kind)
      } catch (err) {
        console.warn('Failed to parse agent job SSE payload', err)
      }
    }
    sse?.on(kind, wrapped)
    sseUnsubscribers.push(() => sse?.off(kind, wrapped))
  })
}

onMounted(async () => {
  setupSse()
  await refresh()
})

onBeforeUnmount(() => {
  sseUnsubscribers.splice(0).forEach((fn) => fn())
  sse?.close()
})
</script>

<style scoped>
.queue-stats-card {
  padding: 16px;
  background: var(--surface-alt, var(--surface));
}

.ticket-group-card {
  padding: 16px;
}

.ticket-group-card__header {
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border);
}

.ticket-group-jobs {
  display: grid;
  gap: 12px;
  margin-top: 12px;
}

.stat-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
}

.stat-value {
  font-size: 24px;
  font-weight: 600;
}

.stat-value.running {
  color: var(--accent, #3b82f6);
}

.stat-value.queued {
  color: var(--warning, #f59e0b);
}

.stat-label {
  font-size: 12px;
  color: var(--muted);
}

.filter-tab {
  background: transparent;
  border: none;
  padding: 8px 12px;
  cursor: pointer;
  color: var(--muted);
  font-size: 14px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  gap: 6px;
}

.filter-tab:hover {
  background: var(--surface-alt, rgba(0,0,0,0.05));
}

.filter-tab.active {
  color: var(--text);
  font-weight: 500;
}

.tab-count {
  background: var(--surface-alt, rgba(0,0,0,0.1));
  padding: 2px 6px;
  border-radius: 10px;
  font-size: 11px;
}

.job-card {
  padding: 16px;
  border-radius: 12px;
  background: var(--surface-alt, var(--surface));
  border-left: 3px solid transparent;
}

.summary-chip,
.phase-chip {
  display: inline-flex;
  align-items: center;
  border-radius: 999px;
  padding: 2px 8px;
  font-size: 12px;
  font-weight: 500;
}

.summary-chip-running {
  background: rgba(59, 130, 246, 0.15);
  color: var(--accent, #3b82f6);
}

.summary-chip-queued {
  background: rgba(245, 158, 11, 0.15);
  color: var(--warning, #f59e0b);
}

.summary-chip-completed {
  background: rgba(34, 197, 94, 0.15);
  color: var(--success, #22c55e);
}

.summary-chip-failed {
  background: rgba(239, 68, 68, 0.15);
  color: var(--danger, #ef4444);
}

.phase-chip {
  background: rgba(15, 23, 42, 0.08);
  color: var(--text-muted, var(--muted));
  text-transform: none;
}

.job-card.job-running {
  border-left-color: var(--accent, #3b82f6);
}

.job-card.job-queued {
  border-left-color: var(--warning, #f59e0b);
}

.job-card.job-completed {
  border-left-color: var(--success, #22c55e);
}

.job-card.job-failed,
.job-card.job-cancelled {
  border-left-color: var(--danger, #ef4444);
}

.status-chip {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 12px;
  font-weight: 500;
  text-transform: uppercase;
}

.status-chip.status-running {
  background: rgba(59, 130, 246, 0.15);
  color: var(--accent, #3b82f6);
}

.status-chip.status-queued {
  background: rgba(245, 158, 11, 0.15);
  color: var(--warning, #f59e0b);
}

.status-chip.status-completed {
  background: rgba(34, 197, 94, 0.15);
  color: var(--success, #22c55e);
}

.status-chip.status-failed,
.status-chip.status-cancelled {
  background: rgba(239, 68, 68, 0.15);
  color: var(--danger, #ef4444);
}

.log-panel {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.log-list {
  display: grid;
  gap: 6px;
  font-family: var(--font-mono, monospace);
  font-size: 12px;
  max-height: 300px;
  overflow-y: auto;
}

.log-row {
  display: grid;
  grid-template-columns: auto auto 1fr;
  gap: 8px;
  align-items: baseline;
}

.log-kind {
  font-weight: 600;
}

.log-time {
  color: var(--muted);
}

.log-message {
  color: var(--text);
  word-break: break-word;
}
</style>
