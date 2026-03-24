<template>
  <section class="col" style="gap: 16px;">
    <header class="row" style="align-items: flex-start; justify-content: space-between;">
      <div class="col" style="gap: 4px;">
        <h1 style="margin: 0;">Automations</h1>
        <p class="muted" style="margin: 0;">Configure rules that trigger actions automatically.</p>
      </div>
      <div class="row" style="gap: 8px;">
        <UiSelect v-model="project" aria-label="Project scope" style="min-width: 200px;">
          <option value="">Global rules</option>
          <option v-for="p in projects" :key="p.prefix" :value="p.prefix">
            {{ formatProjectLabel(p) }}
          </option>
        </UiSelect>
        <UiButton variant="ghost" type="button" :disabled="loading" @click="openConfig">
          Open config
        </UiButton>
        <ReloadButton
          :disabled="loading"
          :loading="loading"
          label="Refresh automations"
          title="Refresh automations"
          variant="ghost"
          @click="refresh"
        />
      </div>
    </header>

    <p v-if="projectsError" class="muted" style="margin: 0; font-size: 13px;">
      Project list unavailable: {{ projectsError }}
    </p>

    <!-- Tabs for Rules vs Simulator -->
    <div class="row" style="gap: 8px; border-bottom: 1px solid var(--border); padding-bottom: 8px;">
      <button
        class="filter-tab"
        :class="{ active: activeTab === 'rules' }"
        type="button"
        @click="activeTab = 'rules'"
      >
        Rules
      </button>
      <button
        class="filter-tab"
        :class="{ active: activeTab === 'simulator' }"
        type="button"
        @click="activeTab = 'simulator'"
      >
        Simulator
      </button>
    </div>

    <div v-if="loading" class="muted">Loading configuration…</div>
    <div v-else-if="error" class="error">{{ error }}</div>

    <!-- Rules Tab -->
    <template v-else-if="activeTab === 'rules'">
      <div class="col" style="gap: 12px;">
        <div class="row automation-rules__toolbar" style="justify-content: space-between; align-items: center; gap: 12px; flex-wrap: wrap;">
          <p class="muted" style="margin: 0; max-width: 48rem;">
            Create rules visually here, or fine-tune the generated YAML for advanced cases.
          </p>
          <div class="row" style="gap: 8px; flex-wrap: wrap;">
            <UiButton variant="ghost" type="button" :disabled="loading || savingRules || !rulesDirty" @click="resetRules">
              Reset
            </UiButton>
            <UiButton variant="primary" type="button" :disabled="loading || savingRules || !rulesDirty" @click="saveRules">
              {{ savingRules ? 'Saving…' : 'Save rules' }}
            </UiButton>
          </div>
        </div>

        <AutomationRulesEditor
          v-model="scopeYaml"
          :effective-yaml="effectiveYaml"
          :loading="loading || savingRules"
          :error="rulesSaveError"
          :source-label="automationSourceLabel"
          :scope-hint="project ? `Edit .tasks/${project}/automation.yml. Leave it empty to inherit higher scopes.` : 'Edit .tasks/automation.yml. Leave it empty to clear the global scoped rules.'"
          :available-statuses="availableStatuses"
          :available-priorities="availablePriorities"
          :available-types="availableTypes"
          :available-tags="availableTags"
        />
      </div>

      <div class="help-text">
        <p class="muted" style="font-size: 13px;">
          Rules live in <code>.tasks/automation.yml</code> (global) or <code>.tasks/&lt;PROJECT&gt;/automation.yml</code> (project).
          You can edit them here or from the Configuration page, and use the simulator tab to test job lifecycle events.
          <span v-if="automationSourceLabel">Effective source: {{ automationSourceLabel }}.</span>
        </p>
      </div>
    </template>

    <!-- Simulator Tab -->
    <template v-else-if="activeTab === 'simulator'">
      <UiCard class="simulator-card">
        <h3 style="margin: 0 0 12px 0;">Test automation rules</h3>
        <p class="muted" style="margin: 0 0 16px 0; font-size: 13px;">
          Select a job lifecycle event and a ticket to see which rules would trigger.
        </p>

        <div class="col" style="gap: 12px;">
          <div class="row" style="gap: 12px;">
            <div class="col" style="gap: 4px; flex: 1;">
              <label class="muted" style="font-size: 12px;">Event type</label>
              <UiSelect v-model="simEvent" aria-label="Event type">
                <option value="">Select event…</option>
                <option value="job_start">job_start</option>
                <option value="complete">complete</option>
                <option value="error">error</option>
                <option value="cancel">cancel</option>
              </UiSelect>
            </div>
            <div class="col" style="gap: 4px; flex: 1;">
              <label class="muted" style="font-size: 12px;">Ticket ID</label>
              <UiInput v-model="simTicket" placeholder="e.g., PROJ-123" />
            </div>
          </div>

          <div class="row" style="gap: 8px; justify-content: flex-end;">
            <UiButton
              variant="primary"
              type="button"
              :disabled="!simEvent || !simTicket || simulating"
              @click="runSimulation"
            >
              {{ simulating ? 'Simulating…' : 'Simulate' }}
            </UiButton>
          </div>
        </div>
      </UiCard>

      <div v-if="simError" class="error">{{ simError }}</div>

      <template v-if="simResult">
        <UiCard class="result-card">
          <h4 style="margin: 0 0 8px 0;">Simulation result</h4>

          <div v-if="!simResult.actions.length" class="muted">
            No rules matched for this event.
          </div>

          <div v-else class="col" style="gap: 12px;">
            <div
              v-for="(action, idx) in simResult.actions"
              :key="idx"
              class="simulated-action"
            >
              <div class="row" style="gap: 8px; align-items: center;">
                <span v-if="simResult.rule_name" class="rule-ref">{{ simResult.rule_name }}</span>
                <span v-else class="rule-ref muted">Unnamed rule</span>
                <span class="action-chip">{{ action.action }}</span>
              </div>
              <p v-if="action.description" class="action-description">
                {{ action.description }}
              </p>
            </div>
          </div>
        </UiCard>

        <UiCard v-if="simResult.task_after" class="result-card">
          <h4 style="margin: 0 0 8px 0;">Task state after actions</h4>
          <div class="task-preview">
            <div class="row" style="gap: 8px; flex-wrap: wrap;">
              <span class="chip">{{ simResult.task_after.id }}</span>
              <span class="chip">{{ simResult.task_after.status }}</span>
              <span v-if="simResult.task_after.priority" class="chip">{{ simResult.task_after.priority }}</span>
              <span v-if="simResult.task_after.assignee" class="chip">{{ simResult.task_after.assignee }}</span>
            </div>
            <p v-if="simResult.task_after.title" style="margin: 8px 0 0 0;">
              {{ simResult.task_after.title }}
            </p>
            <div v-if="simResult.task_after.tags?.length" class="row" style="gap: 4px; margin-top: 8px; flex-wrap: wrap;">
              <span v-for="tag in simResult.task_after.tags" :key="tag" class="tag-chip">{{ tag }}</span>
            </div>
          </div>
        </UiCard>
      </template>
    </template>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { api } from '../api/client'
import type {
    AutomationSimulateResponse,
    ConfigInspectResult,
    ProjectDTO,
} from '../api/types'
import AutomationRulesEditor from '../components/AutomationRulesEditor.vue'
import ReloadButton from '../components/ReloadButton.vue'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiInput from '../components/UiInput.vue'
import UiSelect from '../components/UiSelect.vue'

const activeTab = ref<'rules' | 'simulator'>('rules')
const loading = ref(false)
const error = ref('')
const automationSource = ref('')
const scopeYaml = ref('')
const effectiveYaml = ref('')
const baselineYaml = ref('')
const savingRules = ref(false)
const rulesSaveError = ref('')
const project = ref('')
const projects = ref<ProjectDTO[]>([])
const projectsError = ref('')
const configData = ref<ConfigInspectResult | null>(null)
const router = useRouter()
const rulesDirty = computed(() => scopeYaml.value !== baselineYaml.value)

const availableStatuses = computed(() => configData.value?.effective.issue_states ?? [])
const availablePriorities = computed(() => configData.value?.effective.issue_priorities ?? [])
const availableTypes = computed(() => configData.value?.effective.issue_types ?? [])
const availableTags = computed(() => configData.value?.effective.tags ?? [])

const automationSourceLabel = computed(() => {
  switch (automationSource.value) {
    case 'project':
      return 'project automation.yml'
    case 'home':
      return 'home automation.yml'
    case 'global':
      return 'global automation.yml'
    case 'built_in':
      return 'built-in defaults'
    default:
      return ''
  }
})

// Simulator state
const simEvent = ref('')
const simTicket = ref('')
const simulating = ref(false)
const simError = ref('')
const simResult = ref<AutomationSimulateResponse | null>(null)

function openConfig() {
  const query = project.value ? { project: project.value } : undefined
  router.push({ path: '/config', query })
}

async function refresh() {
  loading.value = true
  error.value = ''
  rulesSaveError.value = ''
  try {
    const [automationResponse, configResponse] = await Promise.all([
      api.inspectAutomation(project.value || undefined),
      api.inspectConfig(project.value || undefined).catch(() => null),
    ])
    automationSource.value = automationResponse.source || ''
    scopeYaml.value = automationResponse.scope_yaml || ''
    effectiveYaml.value = automationResponse.effective_yaml || ''
    baselineYaml.value = automationResponse.scope_yaml || ''
    configData.value = configResponse
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err)
  } finally {
    loading.value = false
  }
}

async function saveRules() {
  if (!rulesDirty.value) return
  savingRules.value = true
  rulesSaveError.value = ''
  try {
    const response = await api.setAutomation({
      yaml: scopeYaml.value,
      project: project.value || undefined,
    })
    if (response.errors?.length) {
      throw new Error(response.errors.join('\n'))
    }
    await refresh()
  } catch (err) {
    rulesSaveError.value = err instanceof Error ? err.message : String(err)
  } finally {
    savingRules.value = false
  }
}

function resetRules() {
  scopeYaml.value = baselineYaml.value
  rulesSaveError.value = ''
}

async function loadProjects() {
  projectsError.value = ''
  try {
    const response = await api.listProjects({ limit: 200 })
    projects.value = response.projects || []
  } catch (err) {
    projectsError.value = err instanceof Error ? err.message : String(err)
    projects.value = []
  }
}

async function runSimulation() {
  if (!simEvent.value || !simTicket.value) return
  simulating.value = true
  simError.value = ''
  simResult.value = null
  try {
    const response = await api.simulateAutomation({
      event: simEvent.value,
      ticket_id: simTicket.value,
    })
    simResult.value = response
  } catch (err) {
    simError.value = err instanceof Error ? err.message : String(err)
  } finally {
    simulating.value = false
  }
}

function formatProjectLabel(project: ProjectDTO): string {
  const name = project.name?.trim()
  if (name && name !== project.prefix) {
    return `${name} (${project.prefix})`
  }
  return project.prefix
}

onMounted(() => {
  void Promise.allSettled([loadProjects(), refresh()])
})

watch(project, () => {
  void refresh()
})
</script>

<style scoped>
.filter-tab {
  background: transparent;
  border: none;
  padding: 8px 12px;
  cursor: pointer;
  color: var(--muted);
  font-size: 14px;
  border-radius: 4px;
}

.filter-tab:hover {
  background: var(--surface-alt, rgba(0, 0, 0, 0.05));
}

.filter-tab.active {
  color: var(--text);
  font-weight: 500;
}

.automation-rules__toolbar {
  margin-bottom: 4px;
}

.condition-chip {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 8px;
  font-size: 12px;
  background: var(--surface-alt, rgba(0, 0, 0, 0.05));
  font-family: var(--font-mono, monospace);
}

.action-chip {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 8px;
  font-size: 12px;
  background: rgba(34, 197, 94, 0.15);
  color: var(--success, #22c55e);
}

.simulator-card,
.result-card {
  padding: 16px;
}

.simulated-action {
  padding: 12px;
  border-radius: 8px;
  background: var(--surface-alt, rgba(0, 0, 0, 0.03));
}

.rule-ref {
  font-weight: 500;
}

.action-description {
  margin: 8px 0 0 0;
  font-size: 13px;
  color: var(--muted);
}

.task-preview {
  padding: 12px;
  border-radius: 8px;
  background: var(--surface-alt, rgba(0, 0, 0, 0.03));
}

.tag-chip {
  display: inline-block;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 11px;
  background: var(--surface-alt, rgba(0, 0, 0, 0.1));
}

.help-text {
  padding: 16px;
  background: var(--surface-alt, rgba(0, 0, 0, 0.02));
  border-radius: 8px;
}

.help-text code {
  padding: 2px 4px;
  border-radius: 4px;
  background: var(--surface-alt, rgba(0, 0, 0, 0.1));
  font-family: var(--font-mono, monospace);
  font-size: 12px;
}
</style>
