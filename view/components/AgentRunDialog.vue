<template>
  <UiModal
    :open="open"
    :aria-labelledby="headingId"
    size="md"
    :dismissible="!submitting"
    initial-focus="#agent-run-ticket"
    @close="close"
  >
    <form class="agent-run" data-testid="agent-run-dialog" @submit.prevent="submit">
      <h2 :id="headingId" class="agent-run__title">Run an agent</h2>
      <p class="agent-run__intro">
        Starts a job now, without changing the ticket's assignee. To run a workflow, assign the ticket to an agent
        instead.
      </p>

      <div v-if="!profiles.length" class="agent-run__empty">
        No agent profiles are configured. Add one under <code>agents:</code> in the configuration.
      </div>

      <template v-else>
        <div class="field-grid">
          <label class="field">
            <span class="field-label">Ticket</span>
            <input
              id="agent-run-ticket"
              v-model.trim="ticket"
              class="input"
              list="agent-run-tickets"
              placeholder="PROJ-123"
              autocomplete="off"
              required
            />
            <datalist id="agent-run-tickets">
              <option v-for="id in ticketSuggestions" :key="id" :value="id" />
            </datalist>
          </label>
          <label class="field">
            <span class="field-label">Agent</span>
            <select id="agent-run-agent" v-model="agent" class="input">
              <option v-for="profile in choices" :key="profile.name" :value="profile.name">
                @{{ profile.name }} · {{ profile.runner }}
              </option>
            </select>
          </label>
          <label class="field">
            <span class="field-label">Model</span>
            <input
              id="agent-run-model"
              v-model.trim="model"
              class="input"
              :placeholder="selected?.model || 'CLI default'"
              autocomplete="off"
            />
          </label>
          <label class="field">
            <span class="field-label">Effort</span>
            <input
              id="agent-run-effort"
              v-model.trim="effort"
              class="input"
              list="agent-run-efforts"
              :placeholder="selected?.effort || 'CLI default'"
              autocomplete="off"
            />
            <datalist id="agent-run-efforts">
              <option v-for="level in efforts" :key="level" :value="level" />
            </datalist>
          </label>
        </div>
        <p v-if="selected" class="field-hint">
          Runs {{ selected.runner }} with {{ selected.permissions }} permissions. Leave model and effort empty to use
          the profile's settings.
        </p>

        <label class="field">
          <span class="field-label">Request</span>
          <textarea id="agent-run-prompt" v-model="prompt" class="input agent-run__prompt" rows="4" required></textarea>
        </label>
      </template>

      <p v-if="error" class="error agent-run__error" role="alert">{{ error }}</p>

      <div class="agent-run__actions">
        <UiButton variant="ghost" type="button" :disabled="submitting" @click="close">Cancel</UiButton>
        <UiButton variant="primary" type="submit" :disabled="submitting || !canSubmit">
          {{ submitting ? 'Starting…' : 'Start job' }}
        </UiButton>
      </div>
    </form>
  </UiModal>
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { api } from '../api/client'
import type { AgentJob, AgentProfileInfo } from '../api/types'
import UiButton from './UiButton.vue'
import UiModal from './UiModal.vue'
import { DEFAULT_RUN_PROMPT } from '../utils/agentJobs'

const props = defineProps<{
  open: boolean
  profiles: AgentProfileInfo[]
  ticketSuggestions: string[]
  initialTicket?: string
  initialAgent?: string | null
}>()

const emit = defineEmits<{ close: []; started: [job: AgentJob] }>()

const headingId = useId()
const efforts = ['low', 'medium', 'high', 'xhigh', 'max']
const ticket = ref('')
const agent = ref('')
const model = ref('')
const effort = ref('')
const prompt = ref(DEFAULT_RUN_PROMPT)
const submitting = ref(false)
const error = ref('')

// Profiles can differ per project; once the ticket names one, offer its profiles.
const projectProfiles = ref<AgentProfileInfo[] | null>(null)
const choices = computed(() => projectProfiles.value ?? props.profiles)
const selected = computed(() => choices.value.find((profile) => profile.name === agent.value))

let profileRequest = 0
watch(
  () => ticket.value.match(/^(.+)-\d+$/)?.[1]?.toUpperCase() ?? '',
  async (project) => {
    const request = ++profileRequest
    if (!project) {
      projectProfiles.value = null
      return
    }
    try {
      const response = await api.listAgentProfiles(project)
      if (request !== profileRequest) return
      projectProfiles.value = response?.profiles?.length ? response.profiles : null
    } catch {
      if (request === profileRequest) projectProfiles.value = null
    }
    if (!choices.value.some((profile) => profile.name === agent.value)) agent.value = choices.value[0]?.name ?? ''
  },
)
const canSubmit = computed(() => !!ticket.value && !!selected.value && !!prompt.value.trim())

watch(
  () => props.open,
  (open) => {
    if (!open) return
    ticket.value = props.initialTicket ?? ''
    const preferred = props.profiles.find((profile) => profile.name === props.initialAgent)
    agent.value = (preferred ?? props.profiles[0])?.name ?? ''
    model.value = ''
    effort.value = ''
    prompt.value = DEFAULT_RUN_PROMPT
    error.value = ''
  },
  { immediate: true },
)

function close() {
  if (!submitting.value) emit('close')
}

async function submit() {
  if (!canSubmit.value || submitting.value) return
  submitting.value = true
  error.value = ''
  try {
    const response = await api.runAgentJob({
      ticket_id: ticket.value,
      agent: agent.value,
      prompt: prompt.value.trim(),
      ...(model.value ? { model: model.value } : {}),
      ...(effort.value ? { effort: effort.value } : {}),
    })
    emit('started', response.job)
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    submitting.value = false
  }
}
</script>

<style scoped>
.agent-run {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.agent-run__title {
  margin: 0;
  font-size: 18px;
}

.agent-run__intro,
.agent-run__empty {
  margin: 0;
  color: var(--color-muted);
  font-size: var(--text-sm, 14px);
  line-height: 1.5;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.field-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px 16px;
}

.field-label {
  font-size: var(--text-sm, 14px);
  font-weight: 600;
  color: var(--color-fg);
}

.field-hint {
  margin: -6px 0 0;
  font-size: 12px;
  color: var(--color-muted);
}

.field .input {
  box-sizing: border-box;
  width: 100%;
}

.agent-run__prompt {
  resize: vertical;
  min-height: 88px;
  font: inherit;
  line-height: 1.45;
}

.agent-run__error {
  margin: 0;
}

.agent-run__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

@media (max-width: 640px) {
  .field-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
