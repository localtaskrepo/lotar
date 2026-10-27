<template>
  <section class="col agents-page">
    <PageHeader reload-label="Refresh jobs" :reloading="loading" @reload="refresh()">
      <template #title>Agents</template>
      <template #subtitle>Run agents on tickets and follow their jobs.</template>
      <template #toolbar>
        <UiButton
          variant="primary"
          type="button"
          data-testid="agent-run-open"
          :disabled="!profiles.length"
          :title="profiles.length ? undefined : 'Set up an agent first'"
          @click="openRunDialog()"
        >
          Run agent
        </UiButton>
        <UiButton variant="danger" type="button" :disabled="loading || !hasCancelableJobs" @click="cancelAll">
          Stop all
        </UiButton>
      </template>
    </PageHeader>

    <div class="agents-overview">
      <div v-if="queueStats" class="queue-stats-card" aria-label="Queue">
        <span class="stat-item"
          ><span class="stat-value running">{{ queueStats.running }}</span>
          <span class="stat-label">Running</span></span
        >
        <span class="stat-item"
          ><span class="stat-value queued">{{ queueStats.queued }}</span> <span class="stat-label">Queued</span></span
        >
        <span class="stat-item"
          ><span class="stat-value">{{ queueStats.max_parallel || '∞' }}</span>
          <span class="stat-label">Max parallel</span></span
        >
      </div>
      <div v-if="profiles.length" class="profile-list" aria-label="Agent profiles">
        <button
          v-for="profile in profiles"
          :key="profile.name"
          type="button"
          class="profile-chip"
          :title="`Run @${profile.name} on a ticket`"
          @click="openRunDialog({ agent: profile.name })"
        >
          <span class="profile-chip__name">@{{ profile.name }}</span>
          <span class="profile-chip__meta">{{ profileSummary(profile) }}</span>
        </button>
      </div>
    </div>

    <AgentSetupPanel
      v-if="profilesLoaded && !profiles.length"
      :installed="installedRunners"
      :project="profilesProject"
      @added="loadProfiles"
    />

    <template v-if="profiles.length || jobs.length || !profilesLoaded">
      <div class="filter-tabs" role="tablist" aria-label="Filter jobs">
        <button
          v-for="tab in tabs"
          :key="tab.value"
          class="filter-tab"
          :class="{ active: activeTab === tab.value }"
          type="button"
          role="tab"
          :aria-selected="activeTab === tab.value"
          @click="activeTab = tab.value"
        >
          {{ tab.label }}
          <span v-if="tab.count !== undefined" class="tab-count">{{ tab.count }}</span>
        </button>
      </div>

      <div v-if="loading && !jobs.length" class="muted">Loading jobs…</div>
      <div v-else-if="error" class="error">{{ error }}</div>

      <UiEmptyState
        v-else-if="!filteredJobs.length"
        :title="activeTab === 'all' ? 'No agent jobs yet' : `No ${activeTab} jobs`"
        :description="
          activeTab === 'all'
            ? 'Assign a ticket to an agent profile (for example @implement), or use Run agent to start a job.'
            : 'Jobs with this status will show up here.'
        "
      />

      <div v-else class="col" style="gap: 12px">
        <UiCard v-for="group in groupedJobs" :key="group.ticketId" class="ticket-group-card">
          <div class="ticket-group-card__header">
            <div class="ticket-group-card__identity">
              <div class="ticket-group-card__title-row">
                <button class="chip" type="button" @click="openTask(group.ticketId)">{{ group.ticketId }}</button>
                <span v-if="ticketInfo[group.ticketId]?.title" class="ticket-title">{{
                  ticketInfo[group.ticketId]!.title
                }}</span>
              </div>
              <div class="ticket-meta">
                <span v-if="ticketInfo[group.ticketId]" class="ticket-status">{{
                  ticketInfo[group.ticketId]!.status
                }}</span>
                <span v-if="ticketInfo[group.ticketId]?.assignee">{{ ticketInfo[group.ticketId]!.assignee }}</span>
                <span>{{ group.jobs.length }} {{ group.jobs.length === 1 ? 'job' : 'jobs' }}</span>
                <span v-if="group.cost !== null" class="ticket-cost" title="Total cost of these jobs">{{
                  formatCost(group.cost)
                }}</span>
              </div>
              <ol v-if="group.jobs.length > 1" class="phase-trail" aria-label="Job sequence">
                <li v-for="job in group.jobs" :key="job.id" :class="`phase-step phase-step--${job.status}`">
                  {{ job.agent ? `@${job.agent}` : job.runner }}
                </li>
              </ol>
            </div>
            <UiButton
              variant="ghost"
              type="button"
              class="ticket-group-card__run"
              :disabled="!profiles.length"
              @click="openRunDialog({ ticket: group.ticketId, agent: group.latest?.agent })"
            >
              Run again
            </UiButton>
          </div>

          <div class="ticket-group-jobs">
            <article v-for="job in group.jobs" :key="job.id" class="job-card" :class="`job-${job.status}`">
              <div class="job-card__top">
                <div class="job-card__identity">
                  <span class="status-chip" :class="`status-${job.status}`">{{ job.status }}</span>
                  <span v-if="job.agent" class="phase-chip">@{{ job.agent }}</span>
                  <span class="job-runner"
                    >{{ job.runner
                    }}<span v-if="job.model || job.effort" data-testid="agent-job-settings">
                      · {{ runSettings(job.model, job.effort) }}</span
                    ></span
                  >
                </div>
                <div class="job-card__actions">
                  <UiButton variant="ghost" type="button" @click="toggleLogs(job.id)">
                    {{ isLogsOpen(job.id) ? 'Hide logs' : 'Show logs' }}
                  </UiButton>
                  <UiButton v-if="isActiveJob(job)" variant="danger" type="button" @click="cancel(job.id)">
                    {{ job.status === 'queued' ? 'Remove' : 'Stop' }}
                  </UiButton>
                </div>
              </div>

              <div class="job-card__meta">
                <span :title="formatDate(job.created_at)">{{ jobTiming(job) }}</span>
                <span
                  v-if="job.exit_code !== null && job.exit_code !== undefined && job.exit_code !== 0"
                  class="job-exit"
                  >exit {{ job.exit_code }}</span
                >
                <span v-if="describeUsage(job.usage)" class="job-usage" :title="usageDetail(job.usage) || undefined">{{
                  describeUsage(job.usage)
                }}</span>
                <span v-if="job.worktree_branch" class="branch-chip" :title="job.worktree_path || undefined">{{
                  job.worktree_branch
                }}</span>
              </div>

              <div v-if="job.last_message" class="job-summary">
                <p v-if="isToolLine(job.last_message)" class="job-summary__text job-summary__text--tool">
                  {{ job.last_message }}
                </p>
                <MarkdownContent
                  v-else
                  class="job-summary__text"
                  :class="{
                    'job-summary__text--clamped': isLongSummary(job.last_message) && !expandedSummaries[job.id],
                  }"
                  :source="job.last_message"
                />
                <button
                  v-if="isLongSummary(job.last_message)"
                  type="button"
                  class="link-button"
                  @click="expandedSummaries[job.id] = !expandedSummaries[job.id]"
                >
                  {{ expandedSummaries[job.id] ? 'Show less' : 'Show more' }}
                </button>
              </div>

              <div v-if="isLogsOpen(job.id)" class="log-panel">
                <div v-if="logsLoading[job.id]" class="muted">Loading logs…</div>
                <div v-else-if="logsError[job.id]" class="error">{{ logsError[job.id] }}</div>
                <div v-else-if="(logsByJob[job.id] || []).length === 0" class="muted">No log entries yet.</div>
                <AgentJobLog v-else :entries="logsByJob[job.id] || []" />

                <form v-if="canMessage(job)" class="job-message" @submit.prevent="sendMessage(job.id)">
                  <UiInput
                    :model-value="messageDrafts[job.id] || ''"
                    placeholder="Send a message to the agent"
                    aria-label="Message to the agent"
                    @update:modelValue="(value: string) => setMessageDraft(job.id, value)"
                  />
                  <UiButton variant="ghost" type="submit" :disabled="!messageDrafts[job.id]"> Send </UiButton>
                </form>
              </div>
            </article>
          </div>
        </UiCard>
      </div>
    </template>
  </section>

  <AgentRunDialog
    :open="runDialog.open"
    :profiles="profiles"
    :ticket-suggestions="ticketSuggestions"
    :initial-ticket="runDialog.ticket"
    :initial-agent="runDialog.agent"
    @close="runDialog.open = false"
    @started="onRunStarted"
  />

  <UiModal
    :open="stopAllDialogOpen"
    aria-label="Stop all agent jobs"
    :aria-labelledby="stopAllHeadingId"
    :aria-describedby="stopAllBodyId"
    size="sm"
    :dismissible="!stopAllSubmitting"
    @close="closeStopAllDialog"
  >
    <div class="stop-all-dialog">
      <h2 :id="stopAllHeadingId" class="stop-all-dialog__title">Stop all agent jobs</h2>
      <p :id="stopAllBodyId" class="stop-all-dialog__message">Stop all queued and running agent jobs?</p>
      <p v-if="stopAllError" class="error stop-all-dialog__error" data-testid="stop-all-dialog-error">
        {{ stopAllError }}
      </p>
      <div class="stop-all-dialog__actions">
        <UiButton
          variant="ghost"
          type="button"
          data-autofocus
          :disabled="stopAllSubmitting"
          @click="closeStopAllDialog"
        >
          Cancel
        </UiButton>
        <UiButton variant="danger" type="button" :disabled="stopAllSubmitting" @click="confirmStopAll">
          {{ stopAllSubmitting ? 'Stopping…' : 'Stop all' }}
        </UiButton>
      </div>
    </div>
  </UiModal>
</template>

<script setup lang="ts">
import PageHeader from '../components/PageHeader.vue'
import { computed, onBeforeUnmount, onMounted, reactive, ref, useId, watch } from 'vue'
import { api } from '../api/client'
import { formatDateTime, formatRelativeTime } from '../utils/date'
import type { AgentJob, AgentJobLogEntry, AgentProfileInfo, AgentQueueStats } from '../api/types'
import AgentJobLog from '../components/AgentJobLog.vue'
import AgentRunDialog from '../components/AgentRunDialog.vue'
import AgentSetupPanel from '../components/AgentSetupPanel.vue'
import MarkdownContent from '../components/MarkdownContent.vue'
import {
  describeUsage,
  formatCost,
  isActiveJob,
  isFailedJob,
  isToolLine,
  jobDuration,
  runSettings,
  usageDetail,
} from '../utils/agentJobs'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiInput from '../components/UiInput.vue'
import UiModal from '../components/UiModal.vue'
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

const profiles = ref<AgentProfileInfo[]>([])
const profilesLoaded = ref(false)
const installedRunners = ref<string[]>([])
const profilesProject = ref<string | null>(null)
const ticketInfo = ref<Record<string, { title: string; status: string; assignee?: string | null }>>({})
const expandedSummaries = ref<Record<string, boolean>>({})
const runDialog = reactive<{ open: boolean; ticket?: string; agent?: string | null }>({ open: false })
// Ticks while a job runs so its duration stays current.
const now = ref(Date.now())
let clock: ReturnType<typeof setInterval> | null = null

const tabs = computed(() => [
  { value: 'all' as const, label: 'All', count: jobs.value.length },
  { value: 'running' as const, label: 'Running', count: jobs.value.filter((j) => j.status === 'running').length },
  { value: 'queued' as const, label: 'Queued', count: jobs.value.filter((j) => j.status === 'queued').length },
  { value: 'completed' as const, label: 'Completed', count: jobs.value.filter((j) => j.status === 'completed').length },
  {
    value: 'failed' as const,
    label: 'Failed',
    count: jobs.value.filter(isFailedJob).length,
  },
])

const filteredJobs = computed(() => {
  if (activeTab.value === 'all') return jobs.value
  if (activeTab.value === 'failed') return jobs.value.filter(isFailedJob)
  return jobs.value.filter((j) => j.status === activeTab.value)
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
        cost: ticketJobs.some((job) => job.usage?.cost_usd !== undefined)
          ? ticketJobs.reduce((sum, job) => sum + (job.usage?.cost_usd ?? 0), 0)
          : null,
        counts: {
          running: ticketJobs.filter((job) => job.status === 'running').length,
          queued: ticketJobs.filter((job) => job.status === 'queued').length,
          completed: ticketJobs.filter((job) => job.status === 'completed').length,
          failed: ticketJobs.filter(isFailedJob).length,
        },
      }
    })
    .sort((a, b) => (b.latest?.created_at ?? '').localeCompare(a.latest?.created_at ?? ''))
})

const hasCancelableJobs = computed(() => jobs.value.some(isActiveJob))
const ticketSuggestions = computed(() => [...new Set(jobs.value.map((job) => job.ticket_id))])

async function loadProfiles() {
  try {
    const response = await api.listAgentProfiles()
    profiles.value = response?.profiles ?? []
    installedRunners.value = response?.installed_runners ?? []
    profilesProject.value = response?.project ?? null
  } catch {
    profiles.value = []
  } finally {
    profilesLoaded.value = true
  }
}

const ticketRequests = new Set<string>()
async function loadTicket(id: string) {
  if (ticketRequests.has(id)) return
  ticketRequests.add(id)
  try {
    const task = await api.getTask(id)
    if (task) {
      ticketInfo.value = {
        ...ticketInfo.value,
        [id]: { title: task.title, status: task.status, assignee: task.assignee },
      }
    }
  } catch {
    // The ticket may be deleted or unreadable; the group still shows its ID.
  } finally {
    ticketRequests.delete(id)
  }
}

watch(
  () => [...new Set(jobs.value.map((job) => job.ticket_id))],
  (ids) => {
    for (const id of ids) if (!ticketInfo.value[id]) void loadTicket(id)
  },
)

// Automation moves the ticket right after a job event; read it again shortly after.
function refreshTicketSoon(id: string) {
  setTimeout(() => void loadTicket(id), 400)
}

function profileSummary(profile: AgentProfileInfo) {
  return [
    profile.runner,
    runSettings(profile.model, profile.effort),
    profile.permissions !== 'workspace' ? profile.permissions : '',
  ]
    .filter(Boolean)
    .join(' · ')
}

function jobTiming(job: AgentJob) {
  if (job.status === 'queued') return `Queued ${formatRelativeTime(job.created_at)}`
  if (!job.started_at) return `Removed ${formatRelativeTime(job.finished_at || job.created_at)}`
  const duration = jobDuration(job, now.value)
  const started = `Started ${formatRelativeTime(job.started_at)}`
  if (!duration) return started
  return job.status === 'running' ? `${started} · running for ${duration}` : `${started} · ran ${duration}`
}

const isLongSummary = (text: string) => text.length > 280 || text.split('\n').length > 3

// Messages reach a running job through its stdin, which only Claude keeps open.
const canMessage = (job: AgentJob) => job.status === 'running' && job.runner === 'claude'

function openRunDialog(prefill: { ticket?: string; agent?: string | null } = {}) {
  runDialog.ticket = prefill.ticket
  runDialog.agent = prefill.agent
  runDialog.open = true
}

function onRunStarted(job: AgentJob) {
  mergeJob(job)
  runDialog.open = false
  activeTab.value = 'all'
}

let sse: ReturnType<typeof useSse> | null = null
const sseUnsubscribers: Array<() => void> = []

// Events can arrive while a list request is still in flight (and events
// delivered before the stream opens are lost entirely). Each applied event
// bumps the sequence and records its derived job so a late response can be
// detected and merged instead of overwriting newer live state.
let eventSeq = 0
const recentEventJobs = new Map<string, { job: AgentJob; seq: number }>()
// Concurrent list requests can complete out of order (separate connections);
// only the most recently issued refresh may apply its snapshot.
let refreshSeq = 0

async function refresh(options: { silent?: boolean } = {}) {
  const myRefresh = ++refreshSeq
  const reportErrors = !options.silent || loading.value
  // A silent reconcile (stream-open) never blanks the visible list or the
  // error state; it only updates data when it lands.
  if (!options.silent) {
    loading.value = true
    error.value = ''
  }
  const seqAtRequest = eventSeq
  try {
    const jobsResponse = await api.listAgentJobs()
    if (myRefresh !== refreshSeq) {
      // A newer refresh superseded this one; its snapshot is at least as
      // fresh and owns the application (and the loading flag).
      return
    }
    // The response is a snapshot from before any event that arrived during
    // the flight; those events win and are merged back over the list.
    const byId = new Map(jobsResponse.jobs.map((existing) => [existing.id, existing]))
    for (const { job: next, seq } of recentEventJobs.values()) {
      if (seq <= seqAtRequest) continue
      const existing = byId.get(next.id)
      byId.set(next.id, existing ? mergeJobFields(existing, next) : next)
    }
    jobs.value = sortJobs([...byId.values()])
    queueStats.value = jobsResponse.queue_stats || null
  } catch (err) {
    if (reportErrors && myRefresh === refreshSeq) {
      error.value = err instanceof Error ? err.message : String(err)
    }
  } finally {
    if (myRefresh === refreshSeq) {
      loading.value = false
    }
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
    model: payload.model || null,
    effort: payload.effort || null,
    usage: payload.usage || null,
  }
  mergeJob(next)
  recentEventJobs.set(next.id, { job: next, seq: ++eventSeq })
  if (kind.startsWith('agent_job_')) {
    const entry: AgentJobLogEntry = {
      kind,
      at: payload.at,
      message: payload.message ?? null,
      sequence: payload.sequence,
    }
    updateLogEntries(next.id, [entry])
  }
  // Update queue stats on job state changes
  if (['agent_job_started', 'agent_job_completed', 'agent_job_failed', 'agent_job_cancelled'].includes(kind)) {
    refreshTicketSoon(next.ticket_id)
    api
      .listAgentJobs()
      .then((r) => {
        queueStats.value = r.queue_stats || null
      })
      .catch(() => {})
  }
}

function mergeJob(next: AgentJob) {
  const idx = jobs.value.findIndex((job) => job.id === next.id)
  if (idx === -1) {
    jobs.value = sortJobs([...jobs.value, next])
    return
  }
  const existing = jobs.value[idx]
  if (!existing) return
  const updated = [...jobs.value]
  updated[idx] = mergeJobFields(existing, next)
  jobs.value = sortJobs(updated)
}

/** Field-level merge of an event-derived job over known state; event wins. */
function mergeJobFields(existing: AgentJob, next: AgentJob): AgentJob {
  return {
    ...existing,
    ...next,
    agent: next.agent ?? existing.agent ?? null,
    last_message: next.last_message || existing.last_message,
    summary: next.summary || existing.summary,
    session_id: next.session_id || existing.session_id,
    usage: next.usage ?? existing.usage ?? null,
  }
}

function sortJobs(list: AgentJob[]): AgentJob[] {
  return [...list].sort((a, b) => b.created_at.localeCompare(a.created_at))
}

async function toggleLogs(id: string) {
  openLogs.value[id] = !openLogs.value[id]
  // A cache entry only proves live events arrived, not that the persisted
  // history was loaded: progress emitted before the stream went live is
  // missing from it. Fetch on every newly-open so the server history is
  // visible while the job is still running; loadLogs dedupes concurrent
  // requests per job.
  if (openLogs.value[id]) {
    await loadLogs(id)
  }
}

function isLogsOpen(id: string) {
  return !!openLogs.value[id]
}

const logEvents = new Map<string, Map<number, AgentJobLogEntry>>()
const logEvictionFloors = new Map<string, number>()
const LOG_BUFFER_LIMIT = 1000

async function loadLogs(id: string) {
  if (logsLoading.value[id]) return
  logsLoading.value[id] = true
  logsError.value[id] = ''
  try {
    const response = await api.getAgentJobLogs(id)
    updateLogEntries(id, response.events)
  } catch (err) {
    logsError.value[id] = err instanceof Error ? err.message : String(err)
  } finally {
    logsLoading.value[id] = false
  }
}

// Both sources share event identity, so late snapshots and queued broadcasts
// can join without overwriting live output or matching identical chunks.
function updateLogEntries(id: string, entries: AgentJobLogEntry[]) {
  const retained = logEvents.get(id) ?? new Map<number, AgentJobLogEntry>()
  const floor = logEvictionFloors.get(id) ?? 0
  for (const entry of entries) {
    if (!Number.isSafeInteger(entry.sequence) || entry.sequence <= floor || retained.has(entry.sequence)) continue
    retained.set(entry.sequence, entry)
  }
  const ordered = [...retained.values()].sort((a, b) => a.sequence - b.sequence)
  const evicted = ordered.splice(0, Math.max(0, ordered.length - LOG_BUFFER_LIMIT))
  for (const entry of evicted) retained.delete(entry.sequence)
  if (evicted.length) logEvictionFloors.set(id, evicted[evicted.length - 1]!.sequence)
  logEvents.set(id, retained)
  logsByJob.value = {
    ...logsByJob.value,
    [id]: ordered,
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

const stopAllDialogOpen = ref(false)
const stopAllSubmitting = ref(false)
const stopAllError = ref<string | null>(null)
const stopAllHeadingId = useId()
const stopAllBodyId = useId()

function cancelAll() {
  if (!hasCancelableJobs.value) return
  stopAllError.value = null
  stopAllDialogOpen.value = true
}

function closeStopAllDialog() {
  if (stopAllSubmitting.value) return
  stopAllDialogOpen.value = false
}

async function confirmStopAll() {
  if (stopAllSubmitting.value) return
  // Jobs may have finished while the dialog was open; never mutate without work to do.
  if (!hasCancelableJobs.value) {
    stopAllDialogOpen.value = false
    return
  }
  stopAllError.value = null
  stopAllSubmitting.value = true
  try {
    const response = await api.cancelAllAgentJobs()
    jobs.value = sortJobs(response.jobs)
    const statsResponse = await api.listAgentJobs()
    queueStats.value = statsResponse.queue_stats || null
    stopAllDialogOpen.value = false
  } catch (err) {
    // Keep the dialog open so the failure is visible and retry remains possible.
    stopAllError.value = err instanceof Error ? err.message : String(err)
  } finally {
    stopAllSubmitting.value = false
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
    // The input entry renders from its agent_job_input SSE echo, which
    // carries the server sequence and timestamp; a local append could not
    // participate in the sequence-keyed dedup.
    setMessageDraft(id, '')
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  }
}

function openTask(taskId: string) {
  openTaskPanel({ taskId })
}

const formatDate = (value?: string | null) => formatDateTime(value, { empty: 'unknown' })

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
      'task_updated',
    ].join(','),
  })

  const onTaskUpdated = (ev: MessageEvent) => {
    try {
      const id = JSON.parse(ev.data || '{}')?.id
      if (typeof id === 'string' && ticketInfo.value[id]) void loadTicket(id)
    } catch {
      // Ignore malformed payloads.
    }
  }
  sse?.on('task_updated', onTaskUpdated)
  sseUnsubscribers.push(() => sse?.off('task_updated', onTaskUpdated))

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

  // Reconcile whenever the stream (re)opens: events delivered before the
  // first open are lost, and events during a disconnect gap are missed, so
  // the pre-open snapshot cannot be trusted once the stream is live. The
  // reconcile is silent so a slow or dropped fetch can never blank the
  // already-visible list.
  sse?.on('open', () => {
    void refresh({ silent: true })
  })
}

onMounted(async () => {
  setupSse()
  clock = setInterval(() => {
    if (jobs.value.some((job) => job.status === 'running')) now.value = Date.now()
  }, 1000)
  void loadProfiles()
  await refresh()
})

onBeforeUnmount(() => {
  if (clock) clearInterval(clock)
  sseUnsubscribers.splice(0).forEach((fn) => fn())
  sse?.close()
})
</script>

<style scoped>
.agents-page {
  gap: 16px;
}

.agents-overview {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px 24px;
}

.queue-stats-card {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  align-items: baseline;
}

.stat-item {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
}

.stat-value {
  font-size: 18px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.stat-value.running {
  color: var(--color-accent);
}

.stat-value.queued {
  color: var(--color-warning);
}

.stat-label {
  font-size: 13px;
  color: var(--color-muted);
}

.profile-list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.profile-chip {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
  padding: 4px 10px;
  border: 1px solid var(--color-border);
  border-radius: 999px;
  background: var(--color-surface);
  color: var(--color-fg);
  font-size: 13px;
  cursor: pointer;
}

.profile-chip:hover,
.profile-chip:focus-visible {
  border-color: var(--color-accent);
}

.profile-chip__name {
  font-weight: 600;
}

.profile-chip__meta {
  color: var(--color-muted);
  font-size: 12px;
}

.filter-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  border-bottom: 1px solid var(--color-border);
  padding-bottom: 8px;
}

.filter-tab {
  background: transparent;
  border: none;
  padding: 6px 10px;
  cursor: pointer;
  color: var(--color-muted);
  font-size: 14px;
  border-radius: 6px;
  display: flex;
  align-items: center;
  gap: 6px;
}

.filter-tab:hover {
  background: var(--color-surface);
  color: var(--color-fg);
}

.filter-tab.active {
  color: var(--color-fg);
  font-weight: 600;
  background: var(--color-surface);
}

.tab-count {
  background: var(--color-border);
  padding: 1px 6px;
  border-radius: 10px;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.ticket-group-card {
  padding: 16px;
}

.ticket-group-card__header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--color-border);
}

.ticket-group-card__identity {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.ticket-group-card__title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.ticket-title {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ticket-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  font-size: 12px;
  color: var(--color-muted);
}

.ticket-status {
  font-weight: 600;
  color: var(--color-fg);
}

.ticket-group-card__run {
  flex-shrink: 0;
}

.phase-trail {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 12px;
}

.phase-step {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--color-muted);
}

.phase-step + .phase-step::before {
  content: '→';
  margin-right: 4px;
  color: var(--color-muted);
}

.phase-step::after {
  content: '';
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--color-border-strong, var(--color-border));
}

.phase-step--completed::after {
  background: var(--color-success);
}

.phase-step--running::after {
  background: var(--color-accent);
}

.phase-step--queued::after {
  background: var(--color-warning);
}

.phase-step--failed::after,
.phase-step--cancelled::after {
  background: var(--color-danger);
}

.ticket-group-jobs {
  display: grid;
  gap: 10px;
  margin-top: 12px;
}

.job-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  border-radius: 10px;
  background: var(--color-surface);
  border-left: 3px solid var(--color-border);
  min-width: 0;
}

.job-card.job-running {
  border-left-color: var(--color-accent);
}

.job-card.job-queued {
  border-left-color: var(--color-warning);
}

.job-card.job-completed {
  border-left-color: var(--color-success);
}

.job-card.job-failed,
.job-card.job-cancelled {
  border-left-color: var(--color-danger);
}

.job-card__top {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
}

.job-card__identity {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.job-card__actions {
  display: flex;
  gap: 6px;
  margin-left: auto;
}

.job-runner {
  font-size: 13px;
  color: var(--color-muted);
}

.job-card__meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 12px;
  font-size: 12px;
  color: var(--color-muted);
}

.job-usage {
  font-variant-numeric: tabular-nums;
}

.ticket-cost {
  font-variant-numeric: tabular-nums;
}

.job-exit {
  color: var(--color-danger);
  font-weight: 600;
}

.branch-chip {
  font-family: var(--font-mono, monospace);
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  border: 1px solid var(--color-border);
  color: var(--color-fg);
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.job-summary {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
}

.job-summary__text {
  font-size: 14px;
  line-height: 1.5;
  overflow-wrap: anywhere;
}

.job-summary__text--tool {
  margin: 0;
  font-family: var(--font-mono, monospace);
  font-size: 12px;
  color: var(--color-muted);
  white-space: pre-wrap;
}

.ticket-group-card__title-row .chip {
  flex-shrink: 0;
  white-space: nowrap;
}

.job-summary__text--clamped {
  max-height: 4.5em;
  overflow: hidden;
  mask-image: linear-gradient(to bottom, #000 60%, transparent);
}

.link-button {
  padding: 0;
  border: none;
  background: none;
  color: var(--color-accent);
  font-size: 12px;
  cursor: pointer;
}

.status-chip,
.phase-chip {
  display: inline-flex;
  align-items: center;
  border-radius: 999px;
  padding: 1px 8px;
  font-size: 12px;
  font-weight: 600;
}

.status-chip {
  text-transform: uppercase;
  letter-spacing: 0.02em;
  font-size: 11px;
  color: var(--color-muted);
  background: color-mix(in oklab, var(--color-muted) 15%, transparent);
}

.status-chip.status-running {
  color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 15%, transparent);
}

.status-chip.status-queued {
  color: var(--color-warning);
  background: color-mix(in oklab, var(--color-warning) 15%, transparent);
}

.status-chip.status-completed {
  color: var(--color-success);
  background: color-mix(in oklab, var(--color-success) 15%, transparent);
}

.status-chip.status-failed,
.status-chip.status-cancelled {
  color: var(--color-danger);
  background: color-mix(in oklab, var(--color-danger) 15%, transparent);
}

.phase-chip {
  border: 1px solid var(--color-border);
  color: var(--color-fg);
  font-weight: 500;
}

.log-panel {
  margin-top: 4px;
  padding-top: 10px;
  border-top: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.job-message {
  display: flex;
  gap: 8px;
}

.job-message :deep(input) {
  flex: 1 1 auto;
  min-width: 0;
}

.stop-all-dialog {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.stop-all-dialog__title {
  margin: 0;
  font-size: 18px;
}

.stop-all-dialog__message {
  margin: 0;
  color: var(--color-muted);
}

.stop-all-dialog__error {
  margin: 0;
}

.stop-all-dialog__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

@media (max-width: 640px) {
  .ticket-group-card {
    padding: 12px;
  }

  .ticket-group-card__header {
    flex-direction: column;
  }

  .ticket-title {
    white-space: normal;
  }
}
</style>
