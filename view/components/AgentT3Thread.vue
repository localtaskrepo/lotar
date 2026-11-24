<template>
  <div class="t3-thread" role="group" aria-label="T3 Code thread" data-testid="agent-t3-thread">
    <span class="t3-thread__label">T3 Code thread</span>
    <span v-if="!thread && loading" class="muted">Asking T3 Code…</span>
    <template v-else-if="thread">
      <span class="t3-chip" :class="`t3-chip--${thread.state}`" data-testid="agent-t3-thread-state">{{
        stateLabel(thread)
      }}</span>
      <a v-if="thread.link" class="t3-thread__open" :href="thread.link" target="_blank" rel="noopener noreferrer"
        >Open in T3 Code</a
      >
      <span class="muted t3-thread__checked" :title="detail(thread)">checked {{ checkedAgo }}</span>
    </template>
    <UiButton variant="ghost" class="t3-thread__refresh" :disabled="loading" @click="load">Refresh</UiButton>
    <span v-if="note" class="t3-thread__reason">{{ note }}</span>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { api } from '../api/client'
import type { AgentJobT3Thread } from '../api/types'
import { formatDateTime, formatRelativeTime } from '../utils/date'
import UiButton from './UiButton.vue'

const props = defineProps<{ jobId: string }>()

// The server asks T3 Code at most every 10 seconds per thread.
const POLL_MS = 15000

const thread = ref<AgentJobT3Thread | null>(null)
const loading = ref(false)
const error = ref('')
const now = ref(Date.now())
let poll: ReturnType<typeof setInterval> | null = null

const LABELS: Record<AgentJobT3Thread['state'], string> = {
  running: 'Running',
  waiting: 'Waits for an answer',
  finished: 'Finished',
  failed: 'Failed',
  stopped: 'Stopped',
  unknown: 'Unknown',
  unavailable: 'Unavailable',
}

function stateLabel(value: AgentJobT3Thread): string {
  if (value.state === 'unknown' && value.t3_status) return `T3: ${value.t3_status}`
  return LABELS[value.state] ?? value.state
}

function detail(value: AgentJobT3Thread): string {
  const parts = [`Asked ${formatDateTime(value.checked_at)}`]
  if (value.t3_status) parts.push(`T3 status: ${value.t3_status}`)
  if (value.updated_at) parts.push(`thread changed ${formatDateTime(value.updated_at)}`)
  if (value.thread_id) parts.push(value.thread_id)
  return parts.join(' · ')
}

// Why there is no state: the first ask failed, or T3 Code can't say.
const note = computed(() => {
  if (!thread.value) return error.value
  return thread.value.state === 'unavailable' ? (thread.value.reason ?? '') : ''
})

const checkedAgo = computed(() => {
  void now.value
  return thread.value ? formatRelativeTime(thread.value.checked_at) : ''
})

async function load() {
  if (loading.value) return
  loading.value = true
  try {
    thread.value = (await api.getAgentJobT3Thread(props.jobId)).thread
    error.value = ''
  } catch (err) {
    // Keep the last answer; say why this one failed only when there is none.
    error.value = err instanceof Error ? err.message : 'Could not ask lotar about the T3 Code thread'
  } finally {
    loading.value = false
    now.value = Date.now()
  }
}

function tick() {
  now.value = Date.now()
  if (document.visibilityState === 'visible') void load()
}

function onVisible() {
  if (document.visibilityState === 'visible') void load()
}

onMounted(() => {
  void load()
  poll = setInterval(tick, POLL_MS)
  document.addEventListener('visibilitychange', onVisible)
})

onBeforeUnmount(() => {
  if (poll) clearInterval(poll)
  document.removeEventListener('visibilitychange', onVisible)
})
</script>

<style scoped>
.t3-thread {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 10px;
  padding: 6px 0;
  font-size: 13px;
}

.t3-thread__label {
  font-weight: 600;
}

.t3-thread__open {
  color: var(--color-accent-strong);
}

.t3-thread__reason {
  color: var(--color-muted);
  flex-basis: 100%;
  overflow-wrap: anywhere;
}

.t3-thread__refresh {
  margin-left: auto;
}

.t3-chip {
  display: inline-flex;
  align-items: center;
  border-radius: var(--radius-pill);
  padding: 1px 8px;
  font-size: 12px;
  font-weight: 500;
  border: 1px solid currentColor;
  color: var(--color-muted);
}

.t3-chip--running {
  color: var(--color-accent-strong);
}

.t3-chip--waiting {
  color: var(--color-warning);
}

.t3-chip--finished {
  color: var(--color-success);
}

.t3-chip--failed {
  color: var(--color-danger);
}
</style>
