<template>
  <section class="config-page">
    <header class="page-header card">
      <div class="page-headings">
        <h1>Configuration</h1>
        <p class="muted">Compare global defaults with per-project overrides, edit safely, and see where each value comes from.</p>
      </div>
      <div class="page-actions">
        <UiSelect v-model="project" class="scope-select">
          <option value="">Global defaults</option>
          <option v-for="p in projects" :key="p.prefix" :value="p.prefix">{{ formatProjectLabel(p) }}</option>
        </UiSelect>
  <button class="btn" type="button" @click="handleReload" :disabled="loading">Reload</button>
        <button class="btn secondary" type="button" @click="helpOpen = true">Help</button>
      </div>
    </header>

    <div v-if="error" class="alert alert-error">{{ error }}</div>
    <div v-else-if="loading" class="alert alert-info">Loading configuration…</div>

    <div v-if="inspectData" class="config-body">
      <div class="config-main">
        <ConfigServerSection
          v-if="isGlobal"
          v-model="form.serverPort"
          :error="errors.server_port"
          :group-source="serverPortSource"
          :field-source-label="serverPortSourceLabel"
          :field-source-class="serverPortSourceClass"
          @validate="validateField('server_port')"
        />

        <ConfigGroup v-if="isGlobal" title="Project defaults" description="Applied when new tasks are created without explicit overrides." :source="sourceFor('default_prefix')">
          <div class="field-grid">
            <div class="field">
              <label class="field-label">
                <span>Default project prefix</span>
                <span v-if="sourceFor('default_prefix')" :class="['provenance', provenanceClass(sourceFor('default_prefix'))]">{{ provenanceLabel(sourceFor('default_prefix')) }}</span>
              </label>
              <UiInput v-model="form.defaultPrefix" maxlength="20" @blur="validateField('default_prefix')" placeholder="ACME" />
              <p v-if="errors.default_prefix" class="field-error">{{ errors.default_prefix }}</p>
            </div>
            <div class="field">
              <label class="field-label">
                <span>Default priority</span>
                <span v-if="sourceFor('default_priority')" :class="['provenance', provenanceClass(sourceFor('default_priority'))]">{{ provenanceLabel(sourceFor('default_priority')) }}</span>
              </label>
              <UiSelect v-model="form.defaultPriority" @change="validateField('default_priority')">
                <option v-for="option in priorityOptions" :key="option" :value="option">{{ option }}</option>
              </UiSelect>
              <p v-if="errors.default_priority" class="field-error">{{ errors.default_priority }}</p>
            </div>
            <div class="field">
              <label class="field-label">
                <span>Default status</span>
                <span v-if="sourceFor('default_status')" :class="['provenance', provenanceClass(sourceFor('default_status'))]">{{ provenanceLabel(sourceFor('default_status')) }}</span>
              </label>
              <UiSelect v-model="form.defaultStatus" @change="validateField('default_status')">
                <option value="">(inherit workflow)</option>
                <option v-for="option in statusOptions" :key="option" :value="option">{{ option }}</option>
              </UiSelect>
              <p v-if="errors.default_status" class="field-error">{{ errors.default_status }}</p>
            </div>
          </div>
        </ConfigGroup>

        <ConfigGroup v-if="!isGlobal" title="Project overview" :description="projectOverviewDescription">
          <div class="field-grid">
            <div class="field">
              <label class="field-label">Project name</label>
              <UiInput v-model="form.projectName" maxlength="100" @blur="validateField('project_name')" :placeholder="currentProject?.name || 'Project display name'" />
              <p v-if="errors.project_name" class="field-error">{{ errors.project_name }}</p>
            </div>
            <div class="field">
              <label class="field-label">
                <span>Default priority</span>
                <span v-if="sourceFor('default_priority')" :class="['provenance', provenanceClass(sourceFor('default_priority'))]">{{ provenanceLabel(sourceFor('default_priority')) }}</span>
              </label>
              <UiSelect v-model="form.defaultPriority" @change="validateField('default_priority')">
                <option value="">(inherit global)</option>
                <option v-for="option in priorityOptions" :key="option" :value="option">{{ option }}</option>
              </UiSelect>
              <p v-if="errors.default_priority" class="field-error">{{ errors.default_priority }}</p>
            </div>
            <div class="field">
              <label class="field-label">
                <span>Default status</span>
                <span v-if="sourceFor('default_status')" :class="['provenance', provenanceClass(sourceFor('default_status'))]">{{ provenanceLabel(sourceFor('default_status')) }}</span>
              </label>
              <UiSelect v-model="form.defaultStatus" @change="validateField('default_status')">
                <option value="">(inherit global)</option>
                <option v-for="option in statusOptions" :key="option" :value="option">{{ option }}</option>
              </UiSelect>
              <p v-if="errors.default_status" class="field-error">{{ errors.default_status }}</p>
            </div>
          </div>
        </ConfigGroup>

        <ConfigPeopleSection
          :description="peopleDescription"
          :is-global="isGlobal"
          v-model:default-reporter="form.defaultReporter"
          v-model:default-assignee="form.defaultAssignee"
          v-model:default-tags="form.defaultTags"
          :tag-suggestions="tagSuggestions"
          :default-reporter-error="errors.default_reporter"
          :default-assignee-error="errors.default_assignee"
          :default-tags-error="errors.default_tags"
          :provenance-label="provenanceLabel"
          :provenance-class="provenanceClass"
          :default-reporter-source="sourceFor('default_reporter')"
          :default-assignee-source="sourceFor('default_assignee')"
          :default-tags-source="sourceFor('default_tags')"
          @validate="validateField"
        />

        <ConfigWorkflowSection
          :description="workflowDescription"
          v-model:issue-states="form.issueStates"
          v-model:issue-types="form.issueTypes"
          v-model:issue-priorities="form.issuePriorities"
          :status-suggestions="statusSuggestions"
          :type-suggestions="typeSuggestions"
          :priority-suggestions="prioritySuggestions"
          :issue-states-error="errors.issue_states"
          :issue-types-error="errors.issue_types"
          :issue-priorities-error="errors.issue_priorities"
          :issue-states-source="sourceFor('issue_states')"
          :issue-types-source="sourceFor('issue_types')"
          :issue-priorities-source="sourceFor('issue_priorities')"
          :provenance-label="provenanceLabel"
          :provenance-class="provenanceClass"
          @validate="validateField"
        />

        <ConfigTaxonomySection
          :description="taxonomyDescription"
          v-model:tags="form.tags"
          v-model:custom-fields="form.customFields"
          :tag-wildcard="tagWildcard"
          :custom-field-wildcard="customFieldWildcard"
          :tags-error="errors.tags"
          :custom-fields-error="errors.custom_fields"
          :tags-source="sourceFor('tags')"
          :custom-fields-source="sourceFor('custom_fields')"
          :provenance-label="provenanceLabel"
          :provenance-class="provenanceClass"
          @validate="validateField"
        />

        <ConfigAutomationSection
          :description="automationDescription"
          :group-source="sourceFor('auto_set_reporter')"
          :is-global="isGlobal"
          v-model:auto-set-reporter="form.autoSetReporter"
          v-model:auto-assign-on-status="form.autoAssignOnStatus"
          v-model:auto-codeowners-assign="form.autoCodeownersAssign"
          v-model:auto-tags-from-path="form.autoTagsFromPath"
          v-model:auto-branch-infer-type="form.autoBranchInferType"
          v-model:auto-branch-infer-status="form.autoBranchInferStatus"
          v-model:auto-branch-infer-priority="form.autoBranchInferPriority"
          v-model:auto-identity="form.autoIdentity"
          v-model:auto-identity-git="form.autoIdentityGit"
          :toggle-select-options="toggleSelectOptions"
          :global-toggle-summary="globalToggleSummary"
          :provenance-label="provenanceLabel"
          :provenance-class="provenanceClass"
          :source-for="sourceFor"
        />

        <ConfigScanningSection
          :description="scanningDescription"
          :is-global="isGlobal"
          v-model:scan-signal-words="form.scanSignalWords"
          v-model:scan-ticket-patterns="form.scanTicketPatterns"
          v-model:scan-enable-ticket-words="form.scanEnableTicketWords"
          v-model:scan-enable-mentions="form.scanEnableMentions"
          v-model:scan-strip-attributes="form.scanStripAttributes"
          :toggle-select-options="toggleSelectOptions"
          :global-toggle-summary="globalToggleSummary"
          :provenance-label="provenanceLabel"
          :provenance-class="provenanceClass"
          :source-for="sourceFor"
          :scan-signal-words-error="errors.scan_signal_words"
          :scan-ticket-patterns-error="errors.scan_ticket_patterns"
          :signal-words-source="sourceFor('scan_signal_words')"
          :ticket-patterns-source="sourceFor('scan_ticket_patterns')"
          @validate="validateField"
        />

        <ConfigBranchAliasSection
          :description="branchAliasDescription"
          :is-global="isGlobal"
          v-model:type-entries="form.branchTypeAliases"
          v-model:status-entries="form.branchStatusAliases"
          v-model:priority-entries="form.branchPriorityAliases"
          :type-error="errors.branch_type_aliases"
          :status-error="errors.branch_status_aliases"
          :priority-error="errors.branch_priority_aliases"
          :provenance-label="provenanceLabel"
          :provenance-class="provenanceClass"
          :source-for="sourceFor"
          @add="addAliasEntry"
          @remove="removeAliasEntry"
          @clear="clearAliasField"
          @validate="validateField"
        />

        <div class="form-actions card">
          <div class="form-actions__left">
            <button class="btn" type="button" @click="save" :disabled="saveDisabled">Save changes</button>
            <button class="btn secondary" type="button" @click="resetForm" :disabled="!isDirty">Reset</button>
          </div>
          <div class="form-actions__meta">
            <span v-if="saving" class="muted">Saving…</span>
            <span v-else-if="isDirty" class="muted">You have unsaved changes.</span>
            <span v-else-if="inspectData" class="muted">Last updated {{ lastLoaded }}</span>
          </div>
        </div>
      </div>

    </div>

    <div v-if="helpOpen" class="help-backdrop" @click.self="helpOpen = false">
      <div class="help-card card" role="dialog" aria-modal="true">
        <header class="help-header">
          <div>
            <h2>Configuration help</h2>
            <p class="muted">Highlights from the CLI docs plus handy tips for the UI editor.</p>
          </div>
          <button class="btn" type="button" @click="helpOpen = false">Close</button>
        </header>
        <div class="help-content">
          <section v-for="section in helpSections" :key="section.title">
            <h3>{{ section.title }}</h3>
            <ul>
              <li v-for="item in section.items" :key="item">{{ item }}</li>
            </ul>
          </section>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { api } from '../api/client'
import ConfigAutomationSection from '../components/ConfigAutomationSection.vue'
import ConfigBranchAliasSection from '../components/ConfigBranchAliasSection.vue'
import ConfigGroup from '../components/ConfigGroup.vue'
import ConfigPeopleSection from '../components/ConfigPeopleSection.vue'
import ConfigScanningSection from '../components/ConfigScanningSection.vue'
import ConfigServerSection from '../components/ConfigServerSection.vue'
import ConfigTaxonomySection from '../components/ConfigTaxonomySection.vue'
import ConfigWorkflowSection from '../components/ConfigWorkflowSection.vue'
import UiInput from '../components/UiInput.vue'
import UiSelect from '../components/UiSelect.vue'
import { showToast } from '../components/toast'
import { useConfigForm } from '../composables/useConfigForm'
import { useConfigScope } from '../composables/useConfigScope'
import { formatProjectLabel } from '../utils/projectLabels'

const { projects, project, loading, error: loadError, inspectData, lastLoadedAt, reload } = useConfigScope()
const saving = ref(false)
const helpOpen = ref(false)
const saveError = ref<string | null>(null)
const error = computed(() => saveError.value ?? loadError.value)

const {
  form,
  baseline,
  errors,
  isGlobal,
  currentProject,
  tagWildcard,
  customFieldWildcard,
  tagSuggestions,
  statusOptions,
  priorityOptions,
  statusSuggestions,
  prioritySuggestions,
  typeSuggestions,
  peopleDescription,
  workflowDescription,
  taxonomyDescription,
  projectOverviewDescription,
  automationDescription,
  scanningDescription,
  branchAliasDescription,
  isDirty,
  saveDisabled,
  toggleSelectOptions,
  globalToggleSummary,
  provenanceLabel,
  provenanceClass,
  sourceFor,
  addAliasEntry,
  removeAliasEntry,
  clearAliasField,
  validateField,
  validateAll,
  snapshotForm,
  clearErrors,
  populateForm,
  resetForm,
  buildPayload,
} = useConfigForm({ project, projects, inspectData, saving })

const helpSections = [
  {
    title: 'Scope & precedence',
    items: [
      'Global defaults live in .tasks/config.yml and feed every project unless overridden.',
      'Project overrides win over global, but blank fields always inherit from the higher scope.',
      'CLI flags and environment variables still beat everything else for the running command.',
    ],
  },
  {
    title: 'Editing tips',
    items: [
      'Use the chips inputs to manage lists—press Enter to confirm each value.',
      'Clearing a list in a project scope removes the override and falls back to global.',
      'Statuses, types, and priorities here shape every workflow menu across the app.',
    ],
  },
  {
    title: 'Validation rules',
    items: [
      'Ports must be >= 1024.',
      'Prefixes accept A-Z, 0-9, - or _.',
      'Tags and custom fields are limited to 50 characters each.',
    ],
  },
  {
    title: 'Need more?',
    items: [
      'Run “lotar config help” or check docs/help/config.md for canonical YAML layouts.',
      'Use “lotar config validate” to lint files after bulk edits.',
    ],
  },
]

const serverPortSource = computed(() => sourceFor('server_port'))
const serverPortSourceLabel = computed(() => provenanceLabel(serverPortSource.value))
const serverPortSourceClass = computed(() => provenanceClass(serverPortSource.value))

async function handleReload() {
  saveError.value = null
  await reload()
}

async function save() {
  if (!validateAll()) {
    showToast('Fix validation errors before saving.')
    return
  }
  const payload = buildPayload()
  if (!Object.keys(payload).length) {
    showToast('No changes to save.')
    return
  }
  saving.value = true
  try {
    await api.setConfig({ values: payload, project: isGlobal.value ? undefined : project.value, global: isGlobal.value })
    showToast('Configuration saved')
    await reload()
    saveError.value = null
  } catch (err: any) {
    saveError.value = err?.message ?? String(err)
  } finally {
    saving.value = false
  }
}

const lastLoaded = computed(() => {
  if (!lastLoadedAt.value) return 'just now'
  return lastLoadedAt.value.toLocaleTimeString()
})

watch(
  inspectData,
  (data) => {
    if (!data) return
    populateForm(data)
    baseline.value = snapshotForm()
    clearErrors()
    saveError.value = null
  },
  { immediate: true },
)
</script>


<style scoped>
.config-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding-bottom: 32px;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
  padding: 20px;
}

.page-headings h1 {
  margin: 0;
  font-size: 26px;
}

.page-actions {
  display: flex;
  gap: 8px;
  align-items: center;
}

.scope-select {
  min-width: 200px;
}

.alert {
  padding: 12px 16px;
  border-radius: 8px;
}

.alert-error {
  background: rgba(255, 77, 109, 0.12);
  border: 1px solid rgba(255, 77, 109, 0.45);
}

.alert-info {
  background: rgba(0, 162, 255, 0.12);
  border: 1px solid rgba(0, 162, 255, 0.35);
}

.config-body {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.config-main {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field :deep(.token-input) {
  width: 100%;
}


.field :deep(.input) {
  height: 32px;
  padding: calc(var(--space-2) - 4px) var(--space-3);
  box-sizing: border-box;
}

.field :deep(.ui-select) {
  height: 32px;
  padding: calc(var(--space-2) - 4px) calc(var(--space-3) + 16px) calc(var(--space-2) - 4px) var(--space-3);
  box-sizing: border-box;
}

.field :deep(.token-input .tokens) {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  min-height: 32px;
  padding: calc(var(--space-2) - 4px) var(--space-3);
  background: color-mix(in oklab, var(--color-surface) 96%, transparent);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  color: var(--color-fg);
  transition: border 120ms ease, box-shadow 120ms ease, background 120ms ease;
}

.toggle-grid {
  display: grid;
  gap: 12px;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
}

.toggle-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.toggle-control {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--color-fg);
}

.toggle-control input {
  width: 14px;
  height: 14px;
}

.field :deep(.token-input:focus-within .tokens) {
  border-color: var(--color-accent);
  box-shadow: var(--focus-ring);
}

.field :deep(.token-input .tokens input) {
  color: var(--color-fg);
}

.field :deep(.token-input .tokens input::placeholder) {
  color: var(--color-muted);
  opacity: 1;
}

.field-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 12px;
}

.field-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
}

.field-error {
  color: #ff8091;
  font-size: 12px;
}

.field-hint {
  color: rgba(255, 255, 255, 0.6);
  font-size: 12px;
}

.provenance {
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.source-project {
  background: rgba(0, 180, 120, 0.25);
  color: #9ef0d0;
}

.source-global {
  background: rgba(0, 120, 255, 0.2);
  color: #8bc0ff;
}

.source-built_in {
  background: rgba(255, 255, 255, 0.15);
  color: rgba(255, 255, 255, 0.85);
}

.form-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  padding: 16px 20px;
}

.form-actions__left {
  display: flex;
  gap: 8px;
}

.form-actions__meta {
  font-size: 12px;
  color: rgba(255, 255, 255, 0.65);
}

.help-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 48px 16px;
  z-index: 1200;
}

.help-card {
  max-width: 720px;
  width: 100%;
  padding: 20px;
  max-height: 80vh;
  overflow: auto;
}

.help-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  margin-bottom: 16px;
}

.help-content section + section {
  margin-top: 16px;
}

.help-content h3 {
  margin-bottom: 6px;
}

.help-content ul {
  margin: 0;
  padding-left: 18px;
  color: rgba(255, 255, 255, 0.75);
}
</style>
