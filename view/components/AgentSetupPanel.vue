<template>
  <UiCard class="agent-setup" data-testid="agent-setup">
    <div class="agent-setup__intro">
      <h2 class="agent-setup__title">Set up an agent</h2>
      <p>
        An agent profile names a coding CLI that LoTaR runs on a ticket in this repository, such as Claude Code, Codex
        or GitHub Copilot. Once a profile exists, assign a ticket to <code>@{{ name || 'name' }}</code> or use
        <strong>Run agent</strong>, and the job shows up on this page.
      </p>
    </div>

    <p v-if="!installed.length" class="agent-setup__warning" role="note">
      None of these CLIs was found on the PATH of the machine running LoTaR. Install one and sign in to it first, for
      example Claude Code.
    </p>

    <form class="agent-setup__form" @submit.prevent="submit">
      <label class="field">
        <span class="field-label">Name</span>
        <input id="agent-setup-name" v-model.trim="name" class="input" autocomplete="off" required />
      </label>
      <label class="field">
        <span class="field-label">CLI</span>
        <select id="agent-setup-runner" v-model="runner" class="input">
          <option v-for="option in runnerOptions" :key="option.value" :value="option.value">
            {{ option.label }}
          </option>
        </select>
      </label>
      <label class="field">
        <span class="field-label">Add to</span>
        <select id="agent-setup-scope" v-model="scope" class="input">
          <option v-if="project" value="project">Project {{ project }}</option>
          <option value="global">All projects</option>
        </select>
      </label>
      <div class="agent-setup__submit">
        <UiButton variant="primary" type="submit" :disabled="submitting || !name">
          {{ submitting ? 'Adding…' : 'Add agent' }}
        </UiButton>
      </div>
    </form>
    <p v-if="error" class="error agent-setup__error" role="alert">{{ error }}</p>

    <details class="agent-setup__more">
      <summary>Add it to the configuration yourself</summary>
      <p>
        Put this in <code>{{ configPath }}</code
        >, or run <code>lotar config set agents '{"{{ name || 'implement' }}": "{{ runner }}"}'{{ scopeFlag }}</code
        >:
      </p>
      <pre class="agent-setup__yaml">{{ yaml }}</pre>
      <p>
        Profiles can also set <code>model</code>, <code>effort</code>, <code>permissions</code> and
        <code>instructions</code>. For a full implement → test → merge workflow, start a project from the
        <code>agent-pipeline</code> template: <code>lotar init --template agent-pipeline</code>.
      </p>
    </details>
  </UiCard>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { api } from '../api/client'
import UiButton from './UiButton.vue'
import UiCard from './UiCard.vue'

const props = defineProps<{ installed: string[]; project?: string | null }>()
const emit = defineEmits<{ added: [name: string] }>()

const RUNNERS = [
  { value: 'claude', label: 'Claude Code' },
  { value: 'codex', label: 'Codex' },
  { value: 'copilot', label: 'GitHub Copilot' },
  { value: 'gemini', label: 'Gemini' },
  { value: 'cursor', label: 'Cursor' },
  { value: 'amp', label: 'Amp' },
  { value: 'droid', label: 'Factory Droid' },
  { value: 'opencode', label: 'OpenCode' },
  { value: 'kilo', label: 'Kilo Code' },
  { value: 'antigravity', label: 'Antigravity' },
]

const name = ref('implement')
const runner = ref('claude')
const scope = ref<'project' | 'global'>('project')
const submitting = ref(false)
const error = ref('')

const runnerOptions = computed(() =>
  RUNNERS.map((option) => ({
    value: option.value,
    label: props.installed.length
      ? `${option.label}${props.installed.includes(option.value) ? '' : ' (not found)'}`
      : option.label,
  })),
)

watch(
  () => [props.installed, props.project] as const,
  ([installed, project]) => {
    runner.value = installed.find((cli) => RUNNERS.some((option) => option.value === cli)) ?? 'claude'
    scope.value = project ? 'project' : 'global'
  },
  { immediate: true },
)

const configPath = computed(() =>
  scope.value === 'project' && props.project ? `.tasks/${props.project}/config.yml` : '.tasks/config.yml',
)
const scopeFlag = computed(() =>
  scope.value === 'project' && props.project ? ` --project ${props.project}` : ' --global',
)
const yaml = computed(() => `agents:\n  ${name.value || 'implement'}:\n    runner: ${runner.value}\n`)

async function submit() {
  if (!name.value || submitting.value) return
  submitting.value = true
  error.value = ''
  try {
    const response = await api.addAgentProfile({
      name: name.value,
      runner: runner.value,
      ...(scope.value === 'global' ? { global: true } : { project: props.project ?? undefined }),
    })
    if (response?.errors?.length) {
      error.value = response.errors.join('\n')
      return
    }
    emit('added', name.value)
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    submitting.value = false
  }
}
</script>

<style scoped>
.agent-setup {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 20px;
}

.agent-setup__title {
  margin: 0 0 6px;
  font-size: 18px;
}

.agent-setup__intro p,
.agent-setup__more p {
  margin: 0;
  color: var(--color-muted);
  line-height: 1.5;
  font-size: var(--text-sm, 14px);
}

.agent-setup__warning {
  margin: 0;
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 13px;
  color: var(--color-fg);
  background: color-mix(in oklab, var(--color-warning) 14%, transparent);
}

.agent-setup__form {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1fr) auto;
  gap: 12px;
  align-items: end;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.field-label {
  font-size: var(--text-sm, 14px);
  font-weight: 600;
  color: var(--color-fg);
}

.field .input {
  box-sizing: border-box;
  width: 100%;
}

.agent-setup__error {
  margin: 0;
  white-space: pre-wrap;
}

.agent-setup__more summary {
  cursor: pointer;
  font-size: 13px;
  color: var(--color-accent);
}

.agent-setup__more[open] {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.agent-setup__yaml {
  margin: 0;
  padding: 10px 12px;
  border-radius: 8px;
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  font-family: var(--font-mono, monospace);
  font-size: 12px;
  overflow-x: auto;
}

@media (max-width: 720px) {
  .agent-setup__form {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
