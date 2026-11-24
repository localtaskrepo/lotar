<template>
  <div class="run-inputs">
    <UiButton
      variant="ghost"
      class="run-inputs__toggle"
      :aria-expanded="open ? 'true' : 'false'"
      :aria-controls="panelId"
      @click="toggle"
    >
      {{ open ? 'Hide instructions' : 'Instructions' }}
    </UiButton>

    <div v-if="open" :id="panelId" class="run-inputs__panel" role="region" aria-label="Prompt and settings of this run">
      <div v-if="loading" class="muted">Loading instructions…</div>
      <div v-else-if="error" class="run-inputs__error">
        <span class="error">{{ error }}</span>
        <UiButton variant="ghost" @click="load">Retry</UiButton>
      </div>
      <div v-else-if="!inputs" class="muted">Not recorded for this job.</div>
      <template v-else>
        <div class="run-inputs__actions">
          <span class="muted">Recorded {{ formatDateTime(inputs.at) }}</span>
          <UiButton variant="ghost" @click="copy(inputs.prompt, 'prompt')">Copy prompt</UiButton>
          <UiButton variant="ghost" @click="copy(JSON.stringify(inputs, null, 2), 'instructions')">Copy all</UiButton>
        </div>

        <table class="run-inputs__settings" aria-label="Settings">
          <tbody>
            <tr v-for="row in settingRows" :key="row.name">
              <th scope="row">{{ row.name }}</th>
              <td>{{ row.value }}</td>
            </tr>
          </tbody>
        </table>

        <div class="run-inputs__sections">
          <details v-for="(section, index) in sections" :key="index" class="run-inputs__section">
            <summary>
              {{ section.heading }} <span class="muted">({{ section.length }} characters)</span>
            </summary>
            <pre>{{ section.text }}</pre>
          </details>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, useId } from 'vue'
import { api } from '../api/client'
import type { AgentJobInstructionsOrigin, AgentJobRunInputs } from '../api/types'
import { formatDateTime } from '../utils/date'
import { showToast } from './toast'
import UiButton from './UiButton.vue'

const props = defineProps<{ jobId: string }>()

const panelId = `run-inputs-${useId()}`
const open = ref(false)
const loading = ref(false)
const error = ref('')
const inputs = ref<AgentJobRunInputs | null>(null)

async function load() {
  loading.value = true
  error.value = ''
  try {
    inputs.value = (await api.getAgentJobInputs(props.jobId)).inputs
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Could not load the instructions'
  } finally {
    loading.value = false
  }
}

// No record yet (a queued job, or one still in its worktree setup) loads again on the next open: the first turn
// writes it. A record stays cached.
function toggle() {
  open.value = !open.value
  if (open.value && !inputs.value && !loading.value) void load()
}

async function copy(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text)
    showToast(`Copied ${what}`)
  } catch {
    showToast(`Could not copy ${what}`)
  }
}

function origin(value: AgentJobInstructionsOrigin | undefined): string {
  if (!value) return ''
  return value.path ? `${value.kind}: ${value.path}` : value.kind
}

const list = (values: string[] | undefined) => (values ?? []).join(', ')

const settingRows = computed(() => {
  const value = inputs.value
  if (!value) return []
  const s = value.settings
  const rows: [string, string | undefined][] = [
    ['Runner', s.runner],
    ['Agent profile', s.agent_profile],
    ['Harness agent', s.harness_agent],
    ['Model', s.model],
    ['Effort', s.effort],
    ['Provider', s.provider],
    ['Permissions', s.permissions],
    ['On permission', s.on_permission],
    ['Denied commands', list(s.rules.deny_commands)],
    ['Allowed tools', list(s.rules.allow_tools)],
    ['Denied tools', list(s.rules.deny_tools)],
    ['Writes confined', s.rules.confine_writes ? 'yes' : undefined],
    ...Object.entries(s.options ?? {}).map(([key, option]): [string, string] => [`Option ${key}`, option]),
    ['Extra directories', list(s.add_dirs)],
    ...(s.mcp_servers ?? []).map((server): [string, string] => [
      `MCP ${server.name}`,
      [
        server.url ?? [server.command, ...(server.args ?? [])].filter(Boolean).join(' '),
        server.tools?.length ? `tools: ${server.tools.join(', ')}` : '',
        server.env?.length ? `env: ${server.env.join(', ')}` : '',
        server.headers?.length ? `headers: ${server.headers.join(', ')}` : '',
      ]
        .filter(Boolean)
        .join('; '),
    ]),
    ['Working directory', s.cwd],
    ['Title', s.title],
    ['Command', [s.command, ...(s.args ?? [])].filter(Boolean).join(' ')],
    [
      'Session',
      s.session
        ? `${s.session.name} (${s.session.mode}${s.session.configured ? `, profile: ${s.session.configured}` : ''})`
        : undefined,
    ],
    ['Instructions', origin(value.instructions_source.base)],
    ['Profile instructions', origin(value.instructions_source.profile)],
    ['Environment (names only)', list(value.env)],
  ]
  return rows.filter((row): row is [string, string] => !!row[1]).map(([name, text]) => ({ name, value: text }))
})

// Offsets are code points, so slice the prompt as an array of them.
const sections = computed(() => {
  const value = inputs.value
  if (!value) return []
  const points = Array.from(value.prompt)
  return value.sections.map((section) => ({
    heading: section.heading,
    length: section.length,
    text: points.slice(section.start, section.start + section.length).join(''),
  }))
})
</script>

<style scoped>
.run-inputs {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.run-inputs__toggle {
  align-self: flex-start;
}

.run-inputs__panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  font-size: 13px;
}

.run-inputs__error,
.run-inputs__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.run-inputs__settings {
  border-collapse: collapse;
  width: 100%;
}

.run-inputs__settings th,
.run-inputs__settings td {
  text-align: left;
  vertical-align: top;
  padding: 3px 8px 3px 0;
  border-bottom: 1px solid var(--color-border);
  overflow-wrap: anywhere;
}

.run-inputs__settings th {
  width: 11rem;
  font-weight: 600;
  color: var(--color-muted);
}

.run-inputs__settings td {
  font-family: var(--font-mono);
  font-size: 12px;
}

.run-inputs__sections {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.run-inputs__section summary {
  cursor: pointer;
  padding: 3px 0;
  font-weight: 600;
}

.run-inputs__section pre {
  margin: 4px 0 8px;
  padding: 8px;
  max-height: 360px;
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-family: var(--font-mono);
  font-size: 12px;
  background: color-mix(in oklab, var(--color-fg) 5%, transparent);
  border-radius: var(--radius-md);
}

/* Narrow screens: the name above its value, so values get the full width. */
@media (max-width: 640px) {
  .run-inputs__settings tr {
    display: block;
    padding: 4px 0;
    border-bottom: 1px solid var(--color-border);
  }

  .run-inputs__settings th,
  .run-inputs__settings td {
    display: block;
    width: auto;
    padding: 1px 0;
    border-bottom: none;
  }
}
</style>
