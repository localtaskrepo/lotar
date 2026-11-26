<template>
  <div class="transcript" data-testid="agent-job-transcript">
    <UiButton
      variant="ghost"
      class="transcript__toggle"
      :aria-expanded="open ? 'true' : 'false'"
      :aria-controls="panelId"
      @click="toggle"
    >
      {{ open ? 'Hide conversation' : 'Show conversation' }}
    </UiButton>
    <div v-if="seen" v-show="open" :id="panelId" class="transcript__panel" role="region" aria-label="Conversation">
      <UiLoader v-if="loading && !transcript" size="sm">Loading the conversation…</UiLoader>
      <div v-else-if="error && !transcript" class="transcript__error" data-testid="agent-job-transcript-error">
        <span class="error">{{ error }}</span>
        <UiButton variant="ghost" @click="load(0)">Retry</UiButton>
      </div>
      <p v-else-if="expired" class="muted" data-testid="agent-job-transcript-expired">
        The conversation is no longer on this machine. The job's summary, commits and checks stay on the ticket.
      </p>
      <div v-else-if="transcript?.state === 'unsupported'" class="transcript__unsupported">
        <p class="muted">{{ transcript.reason || 'This conversation can’t be shown here.' }}</p>
        <div v-if="attachCommand" class="transcript__command">
          <code data-testid="agent-job-transcript-attach">{{ attachCommand }}</code>
          <UiButton variant="ghost" @click="copy(attachCommand)">Copy</UiButton>
        </div>
      </div>
      <template v-else-if="transcript">
        <div class="transcript__header">
          <span class="muted">{{ turnText }}</span>
          <a
            v-if="transcript.link"
            class="transcript__open"
            :href="transcript.link"
            target="_blank"
            rel="noopener noreferrer"
            >Open in T3</a
          >
        </div>
        <p v-if="transcript.agent === 't3' && !transcript.tool_events" class="muted transcript__note">
          Tool calls are in T3.
        </p>
        <p v-if="!transcript.events.length" class="muted">The conversation is empty.</p>
        <AgentJobLog v-else :entries="transcript.events" viewer="transcript" />
        <div v-if="transcript.next_after != null || error" class="transcript__more">
          <span v-if="error" class="error">{{ error }}</span>
          <UiButton variant="ghost" :disabled="loading" @click="load(transcript.next_after ?? 0)">
            {{ loading ? 'Loading…' : error ? 'Retry' : 'Load more' }}
          </UiButton>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, useId } from 'vue'
import { api } from '../api/client'
import type { AgentJobTranscript } from '../api/types'
import AgentJobLog from './AgentJobLog.vue'
import { showToast } from './toast'
import UiButton from './UiButton.vue'
import UiLoader from './UiLoader.vue'

// A job's conversation from its harnessmux event log (DEV-909): nothing loads until it is first opened.
const props = defineProps<{ jobId: string }>()

const panelId = `transcript-${useId()}`
const open = ref(false)
const seen = ref(false)
const loading = ref(false)
const error = ref('')
const expired = ref(false)
const transcript = ref<AgentJobTranscript | null>(null)

function toggle() {
  open.value = !open.value
  if (open.value && !seen.value) {
    seen.value = true
    void load(0)
  }
}

async function load(after: number) {
  loading.value = true
  error.value = ''
  try {
    const page = (await api.getAgentJobTranscript(props.jobId, after || undefined)).transcript
    expired.value = page.state === 'expired'
    const earlier = after && transcript.value ? transcript.value.events : []
    transcript.value = { ...page, events: [...earlier, ...page.events] }
  } catch (err) {
    // The job log is gone with the job: so is its conversation.
    if ((err as { status?: number }).status === 404) {
      expired.value = true
      return
    }
    const message = err instanceof Error ? err.message : String(err)
    error.value =
      err instanceof TypeError || !navigator.onLine
        ? `Could not reach LoTaR to load the conversation (${message}).`
        : message
  } finally {
    loading.value = false
  }
}

const attachCommand = computed(() =>
  transcript.value?.session ? `harnessmux attach ${transcript.value.session} --from-start` : '',
)

const turnText = computed(() => {
  const value = transcript.value
  if (!value) return ''
  const turns = value.turns ?? []
  const which = turns.length === 1 ? `Turn ${turns[0]}` : `Turns ${turns.join(', ')}`
  const of = value.session_turns ? ` of ${value.session_turns}` : ''
  const how = value.source === 'time_window' ? ', matched by the job’s time' : ''
  return `${which}${of} in the ${value.agent || 'agent'} session ${value.session}${how}`
})

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    showToast('Copied the command')
  } catch {
    showToast('Could not copy the command')
  }
}
</script>

<style scoped>
.transcript {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.transcript__toggle {
  align-self: flex-start;
}

.transcript__panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.transcript__header,
.transcript__error,
.transcript__more,
.transcript__command {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  font-size: var(--text-sm, 0.875rem);
}

.transcript__command code {
  overflow-wrap: anywhere;
  min-width: 0;
}

.transcript__open {
  color: var(--color-accent);
}

.transcript__note,
.transcript__unsupported p {
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
}
</style>
