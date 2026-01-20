<template>
  <section class="sync-page">
    <div class="page-header">
      <div class="page-headings">
        <h1>Sync</h1>
        <p class="muted">Review sync configuration and run manual syncs with status reporting.</p>
      </div>
      <div class="page-actions">
        <div class="scope-picker">
          <label class="muted" for="sync-scope">Scope</label>
          <UiSelect id="sync-scope" v-model="project">
            <option value="">Global</option>
            <option v-for="entry in projects" :key="entry.prefix" :value="entry.prefix">
              {{ formatProjectLabel(entry) }}
            </option>
          </UiSelect>
        </div>
        <ReloadButton
          :loading="loading"
          label="Reload sync settings"
          title="Reload sync settings"
          @click="handleReload"
        />
      </div>
    </div>

    <p v-if="error" class="sync-error">{{ error }}</p>

    <div v-if="loading" class="sync-loading">
      <UiLoader>Loading sync settings…</UiLoader>
    </div>

    <div v-else class="sync-dashboard">
      <UiCard class="sync-card sync-card--remotes">
          <div class="card-header">
            <div>
              <h3>Remotes & actions</h3>
            </div>
            <div class="card-actions">
              <UiButton type="button" variant="primary" @click="openAddRemoteDialog">Add remote</UiButton>
              <label class="option-row option-row--inline">
                <input v-model="writeReport" type="checkbox" />
                <span>Write report to disk</span>
                <span class="muted option-row__hint">{{ reportsDirLabel }}</span>
              </label>
            </div>
          </div>

          <div class="card-body">
            <p v-if="!remoteEntries.length" class="muted">No remotes configured for this scope.</p>
            <div v-else class="remote-stack">
              <div v-for="entry in remoteEntries" :key="entry.name" class="remote-row">
                <div class="remote-main">
                  <div class="remote-title">
                    <strong>{{ entry.name }}</strong>
                    <span class="remote-provider">
                      <IconGlyph :name="remoteProviderIcon(entry.remote)" />
                      <span>{{ remoteProviderLabel(entry.remote) }}</span>
                    </span>
                    <span
                      v-if="runStatus(entry.name)"
                      :class="['pill', runStatusClass(entry.name), 'pill--interactive']"
                      role="button"
                      tabindex="0"
                      @click.stop="openLatestTaskByRemote(entry.name)"
                      @keydown.enter.prevent="openLatestTaskByRemote(entry.name)"
                      @keydown.space.prevent="openLatestTaskByRemote(entry.name)"
                    >
                      {{ runStatusLabel(entry.name) }}
                    </span>
                  </div>
                  <div
                    v-if="hasValue(entry.remote.auth_profile) || hasValue(entry.remote.filter)"
                    class="remote-meta"
                  >
                    <span v-if="hasValue(entry.remote.auth_profile)" class="remote-meta__item">
                      <span class="remote-meta__label muted">Auth</span>
                      <span class="remote-meta__value">{{ entry.remote.auth_profile }}</span>
                    </span>
                    <span v-if="hasValue(entry.remote.filter)" class="remote-meta__item">
                      <span class="remote-meta__label muted">Filter</span>
                      <span class="remote-meta__value">{{ entry.remote.filter }}</span>
                    </span>
                  </div>
                </div>
                <div class="remote-actions">
                  <UiButton class="remote-action" type="button" :disabled="isRemoteBusy(entry.name)" @click="runSync('pull', entry)">Pull</UiButton>
                  <UiButton class="remote-action" type="button" :disabled="isRemoteBusy(entry.name)" @click="runSync('push', entry)">Push</UiButton>
                  <UiButton class="remote-action" type="button" :disabled="isRemoteBusy(entry.name)" @click="runSync('check', entry)">Check</UiButton>
                  <UiButton class="remote-action" type="button" :disabled="isRemoteBusy(entry.name)" @click="openEditRemoteDialog(entry)">Edit</UiButton>
                </div>
              </div>
            </div>
          </div>
      </UiCard>

      <UiCard class="sync-card sync-card--reports">
          <div class="card-header">
            <div>
              <h3>Sync reports</h3>
            </div>
            <ReloadButton
              :loading="reportsLoading"
              label="Reload reports"
              title="Reload reports"
              @click="loadReports"
            />
          </div>
          <div class="card-body">
            <p v-if="reportsError" class="sync-error">{{ reportsError }}</p>
            <div class="reports-toolbar">
              <label class="reports-filter reports-filter--range">
                <span class="muted">Report range</span>
                <div class="reports-range">
                  <UiInput v-model="reportRangeStart" type="datetime-local" />
                  <span class="muted">to</span>
                  <UiInput v-model="reportRangeEnd" type="datetime-local" />
                </div>
              </label>
              <label class="reports-filter reports-filter--status">
                <span class="muted">Entry status</span>
                <UiSelect v-model="reportEntryFilter">
                  <option value="all">All</option>
                  <option value="created">Created</option>
                  <option value="updated">Updated</option>
                  <option value="skipped">Skipped</option>
                  <option value="failed">Failed</option>
                </UiSelect>
              </label>
              <label class="reports-filter reports-filter--search">
                <span class="muted">Search</span>
                <UiInput v-model="reportEntrySearch" placeholder="Task ID, reference, or message" />
              </label>
            </div>
            <p v-if="reportsLoading" class="muted">Loading reports…</p>
            <p v-else-if="!reportListItems.length" class="muted">No reports yet.</p>
            <p v-else-if="!filteredReportItems.length" class="muted">No reports match the selected range.</p>
            <div v-else class="reports-grid">
              <div class="reports-list">
                <button
                  v-for="report in filteredReportItems"
                  :key="report.id"
                  type="button"
                  class="report-item"
                  :class="{ active: selectedReport?.id === report.id }"
                  @click="openReportItem(report)"
                >
                  <div class="report-item__row report-item__row--top">
                    <div class="report-item__chips">
                      <span class="pill pill--muted report-item__action-chip">
                        {{ reportActionLabel(report) }}
                      </span>
                      <span
                        v-if="report.status"
                        :class="['pill', 'report-item__status-chip', reportStatusClass(report.status)]"
                      >
                        {{ reportStatusLabel(report.status) }}
                      </span>
                    </div>
                    <span class="muted report-item__date">{{ formatTimestamp(report.created_at) }}</span>
                  </div>
                  <div class="report-item__row report-item__row--name">
                    <strong>{{ report.remote }}</strong>
                    <span class="report-summary">{{ reportSummaryLabel(report) }}</span>
                  </div>
                </button>
              </div>
              <div class="reports-detail">
                <p v-if="!selectedReport" class="muted">Select a report to review itemized changes.</p>
                <div v-else>
                  <div class="report-header">
                    <div class="report-header__main">
                      <strong>Report details</strong>
                      <div
                        v-if="selectedReportPath"
                        class="report-path"
                        :title="`${reportsDirLabel}/${selectedReportPath}`"
                      >
                        <IconGlyph name="file" />
                        <span class="report-path__label">Path</span>
                        <span class="report-path__value">{{ reportsDirLabel }}/{{ selectedReportPath }}</span>
                      </div>
                    </div>
                    <div v-if="selectedReport.dry_run" class="report-header__status">
                      <span class="pill pill--muted">Dry run</span>
                    </div>
                  </div>
                  <div v-if="selectedReportFields.length" class="report-fields">
                    <div class="muted report-fields__label">Fields synced</div>
                    <div class="report-fields__list">
                      <span v-for="field in selectedReportFields" :key="field" class="field-chip">
                        {{ field }}
                      </span>
                    </div>
                  </div>
                  <div class="reports-entries">
                    <p v-if="!filteredReportEntries.length" class="muted">No entries match the current filter.</p>
                    <div v-else class="list-stack">
                      <div
                        v-for="entry in filteredReportEntries"
                        :key="`${entry.at}-${entry.task_id || entry.reference || entry.title}`"
                        class="list-item report-entry"
                        :class="{ interactive: !!entry.task_id }"
                        role="button"
                        :tabindex="entry.task_id ? 0 : -1"
                        @click="openTaskFromEntry(entry)"
                        @keydown.enter.prevent="openTaskFromEntry(entry)"
                        @keydown.space.prevent="openTaskFromEntry(entry)"
                      >
                        <div class="report-entry__row">
                          <strong>{{ entry.task_id || entry.reference || entry.title || 'Item' }}</strong>
                          <span class="muted report-entry__date">{{ formatTimestamp(entry.at) }}</span>
                        </div>
                        <div class="list-item__details">
                          <div class="detail"><span class="muted">Status</span>{{ entry.status }}</div>
                          <div v-if="entry.fields?.length" class="detail"><span class="muted">Fields</span>{{ entry.fields.join(', ') }}</div>
                          <div v-if="entry.message" class="detail"><span class="muted">Note</span>{{ entry.message }}</div>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
      </UiCard>
    </div>
  </section>
  <Teleport to="body">
    <div
      v-if="remoteDialogOpen"
      class="sync-remote-dialog__overlay"
      role="dialog"
      aria-modal="true"
      :aria-label="remoteDialogTitle"
      @click.self="closeRemoteDialog"
    >
      <UiCard class="sync-remote-dialog__card">
        <form class="sync-remote-dialog__form" @submit.prevent="submitRemoteDialog">
          <header class="sync-remote-dialog__header">
            <h2>{{ remoteDialogTitle }}</h2>
            <UiButton
              variant="ghost"
              icon-only
              type="button"
              :disabled="remoteDialogSubmitting"
              aria-label="Close dialog"
              title="Close dialog"
              @click="closeRemoteDialog"
            >
              <IconGlyph name="close" />
            </UiButton>
          </header>

          <div class="form-grid">
            <label class="sync-remote-dialog__field">
              <span class="muted">Name</span>
              <UiInput v-model="remoteForm.name" placeholder="jira-home" />
            </label>
            <label class="sync-remote-dialog__field">
              <span class="muted">Provider</span>
              <UiSelect v-model="remoteForm.provider">
                <option value="jira">Jira</option>
                <option value="github">GitHub</option>
              </UiSelect>
            </label>
            <label v-if="remoteForm.provider === 'jira'" class="sync-remote-dialog__field">
              <span class="muted">Project key</span>
              <UiInput v-model="remoteForm.project" placeholder="DEMO" />
            </label>
            <label v-else class="sync-remote-dialog__field">
              <span class="muted">Repository</span>
              <UiInput v-model="remoteForm.repo" placeholder="owner/repo" />
            </label>
            <label class="sync-remote-dialog__field">
              <span class="muted">Auth profile</span>
              <UiInput v-model="remoteForm.auth_profile" :placeholder="authProfilePlaceholder" list="sync-auth-profile-options" />
            </label>
            <label class="sync-remote-dialog__field">
              <span class="sync-remote-dialog__label">
                <span class="muted">Filter</span>
                <button
                  type="button"
                  class="sync-remote-dialog__help"
                  :aria-expanded="filterHelpOpen"
                  aria-label="Filter format help"
                  @click="filterHelpOpen = !filterHelpOpen"
                >
                  <IconGlyph name="help" />
                </button>
              </span>
              <UiInput v-model="remoteForm.filter" placeholder="Optional filter" />
              <p v-if="filterHelpOpen" class="muted sync-remote-dialog__hint">
                {{ filterHelpText }}
              </p>
            </label>
          </div>

          <label class="sync-remote-dialog__field">
            <span class="muted">Mapping (YAML)</span>
            <textarea
              v-model="remoteForm.mapping"
              class="input sync-textarea"
              :rows="mappingRows"
              placeholder="title: summary\nstatus:\n  field: status\n  values:\n    Todo: 'To Do'\n    InProgress: 'In Progress'"
            ></textarea>
          </label>
          <p class="muted sync-remote-dialog__hint">Saved to {{ scopeLabel }} as YAML. Mapping is required to sync fields.</p>
          <p v-if="mappingErrorPreview.length" class="error">{{ mappingErrorPreview.join(' ') }}</p>
          <p v-if="remoteFormError" class="error">{{ remoteFormError }}</p>

          <footer class="form-actions">
            <div class="form-actions__group">
              <UiButton
                variant="primary"
                type="submit"
                :disabled="remoteDialogSubmitting || remoteDialogValidating"
              >
                {{ remoteDialogSubmitting ? 'Saving…' : remoteDialogMode === 'add' ? 'Add remote' : 'Save remote' }}
              </UiButton>
              <UiButton
                variant="ghost"
                type="button"
                :disabled="remoteDialogSubmitting || remoteDialogValidating"
                @click="validateRemoteDialog"
              >
                {{ remoteDialogValidating ? 'Validating…' : 'Validate' }}
              </UiButton>
              <span v-if="remoteDialogValidationMessage" :class="['validation-status', remoteDialogValidationClass]">
                <IconGlyph name="check" />
                {{ remoteDialogValidationMessage }}
              </span>
            </div>
            <UiButton variant="ghost" type="button" :disabled="remoteDialogSubmitting" @click="closeRemoteDialog">
              Cancel
            </UiButton>
          </footer>
        </form>
      </UiCard>
      <datalist id="sync-auth-profile-options">
        <option v-for="profile in authProfileOptions" :key="profile" :value="profile" />
      </datalist>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { parse as parseYaml, stringify as stringifyYaml } from 'yaml'
import { api } from '../api/client'
import type {
  SyncFieldMapping,
  SyncProvider,
  SyncRemoteConfig,
  SyncReport,
  SyncReportEntry,
  SyncReportMeta,
  SyncReportStatus,
  SyncResponse,
} from '../api/types'
import IconGlyph from '../components/IconGlyph.vue'
import ReloadButton from '../components/ReloadButton.vue'
import { showToast } from '../components/toast'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiInput from '../components/UiInput.vue'
import UiLoader from '../components/UiLoader.vue'
import UiSelect from '../components/UiSelect.vue'
import { useConfigScope } from '../composables/useConfigScope'
import { useSse } from '../composables/useSse'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { formatProjectLabel } from '../utils/projectLabels'

const { projects, project, loading, error, inspectData, reload } = useConfigScope()
const { openTaskPanel } = useTaskPanelController()

const scope = computed(() => (project.value ? 'project' : 'global'))

const globalRemotes = computed<Record<string, SyncRemoteConfig>>(() => inspectData.value?.global_raw?.remotes ?? {})
const projectRemotes = computed<Record<string, SyncRemoteConfig>>(() => inspectData.value?.project_raw?.remotes ?? {})
const effectiveRemotes = computed<Record<string, SyncRemoteConfig>>(() => inspectData.value?.effective?.remotes ?? {})
const scopedRemotes = computed<Record<string, SyncRemoteConfig>>(() => (project.value ? projectRemotes.value : globalRemotes.value))

const remoteEntries = computed(() =>
  Object.entries(effectiveRemotes.value)
    .map(([name, remote]) => ({ name, remote }))
    .sort((a, b) => a.name.localeCompare(b.name))
)

type RemoteDialogMode = 'add' | 'edit'
type RemoteFormState = {
  name: string
  provider: SyncProvider
  project: string
  repo: string
  filter: string
  auth_profile: string
  mapping: string
}

const remoteDialogOpen = ref(false)
const remoteDialogMode = ref<RemoteDialogMode>('add')
const remoteDialogSubmitting = ref(false)
const remoteDialogValidating = ref(false)
const remoteDialogValidationStatus = ref<'idle' | 'ok' | 'warn'>('idle')
const remoteDialogValidationMessage = ref<string | null>(null)
const remoteFormError = ref<string | null>(null)
const remoteFormOriginalName = ref<string | null>(null)
const remoteForm = ref<RemoteFormState>({
  name: '',
  provider: 'jira',
  project: '',
  repo: '',
  filter: '',
  auth_profile: '',
  mapping: '',
})

const filterHelpOpen = ref(false)

const authProfileOptions = computed(() => {
  const profiles = inspectData.value?.auth_profiles ?? {}
  const provider = remoteForm.value.provider
  return Object.entries(profiles)
    .filter(([, profile]) => !profile?.provider || profile.provider === provider)
    .map(([name]) => name)
    .sort((a, b) => a.localeCompare(b))
})

const authProfilePlaceholder = computed(() =>
  remoteForm.value.provider === 'github' ? 'github.default' : 'jira.default',
)

const filterHelpText = computed(() =>
  remoteForm.value.provider === 'github'
    ? 'GitHub filter uses issues search syntax (example: is:issue label:bug state:open).'
    : 'Jira filter uses JQL (example: project = DEMO AND status != Done).',
)

const remoteDialogValidationClass = computed(() => {
  if (remoteDialogValidationStatus.value === 'ok') return 'validation-status--ok'
  if (remoteDialogValidationStatus.value === 'warn') return 'validation-status--warn'
  return 'validation-status--muted'
})

watch(
  () => remoteForm.value.provider,
  () => {
    if (
      remoteForm.value.auth_profile &&
      !authProfileOptions.value.includes(remoteForm.value.auth_profile)
    ) {
      remoteForm.value.auth_profile = ''
    }
    filterHelpOpen.value = false
  },
)

watch(
  remoteForm,
  () => {
    remoteDialogValidationStatus.value = 'idle'
    remoteDialogValidationMessage.value = null
  },
  { deep: true },
)

const remoteDialogTitle = computed(() =>
  remoteDialogMode.value === 'add' ? 'Add remote' : 'Edit remote',
)

const scopeLabel = computed(() => (project.value ? `Project ${project.value}` : 'Global'))

function hasValue(value?: string | null): boolean {
  return String(value ?? '').trim().length > 0
}

function remoteProviderIcon(remote: SyncRemoteConfig): 'jira' | 'github' | 'list' {
  if (remote.provider === 'jira') return 'jira'
  if (remote.provider === 'github') return 'github'
  return 'list'
}

function remoteProviderLabel(remote: SyncRemoteConfig): string {
  if (remote.provider === 'jira') {
    return remote.project?.trim() || 'Jira'
  }
  if (remote.provider === 'github') {
    return remote.repo?.trim() || 'GitHub'
  }
  return String(remote.provider)
}

function resetRemoteForm() {
  remoteForm.value = {
    name: '',
    provider: 'jira',
    project: '',
    repo: '',
    filter: '',
    auth_profile: '',
    mapping: '',
  }
  filterHelpOpen.value = false
  remoteDialogValidationStatus.value = 'idle'
  remoteDialogValidationMessage.value = null
}

function openAddRemoteDialog() {
  remoteDialogMode.value = 'add'
  remoteFormOriginalName.value = null
  remoteFormError.value = null
  resetRemoteForm()
  remoteDialogOpen.value = true
}

function openEditRemoteDialog(entry: { name: string; remote: SyncRemoteConfig }) {
  remoteDialogMode.value = 'edit'
  remoteFormOriginalName.value = entry.name
  remoteFormError.value = null
  filterHelpOpen.value = false
  remoteDialogValidationStatus.value = 'idle'
  remoteDialogValidationMessage.value = null
  remoteForm.value = {
    name: entry.name,
    provider: entry.remote.provider,
    project: entry.remote.project ?? '',
    repo: entry.remote.repo ?? '',
    filter: entry.remote.filter ?? '',
    auth_profile: entry.remote.auth_profile ?? '',
    mapping: formatMapping(entry.remote.mapping),
  }
  remoteDialogOpen.value = true
}

function closeRemoteDialog() {
  if (remoteDialogSubmitting.value) return
  remoteDialogOpen.value = false
}

function formatMapping(mapping?: Record<string, SyncFieldMapping>): string {
  if (!mapping || Object.keys(mapping).length === 0) return ''
  return stringifyYaml(mapping).trim()
}

function parseMappingInput(value: string): Record<string, SyncFieldMapping> | null {
  const trimmed = value.trim()
  if (!trimmed) return {}
  try {
    const parsed = parseYaml(trimmed)
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null
    return parsed as Record<string, SyncFieldMapping>
  } catch {
    return null
  }
}

function buildRemoteConfigFromForm(): { config: SyncRemoteConfig | null; errors: string[] } {
  const errors: string[] = []
  const provider = remoteForm.value.provider
  const projectValue = remoteForm.value.project.trim()
  const repoValue = remoteForm.value.repo.trim()

  if (provider === 'jira' && !projectValue) {
    errors.push('Jira project key is required.')
  }
  if (provider === 'github' && !repoValue) {
    errors.push('GitHub repository is required.')
  }

  const mapping = parseMappingInput(remoteForm.value.mapping)
  if (mapping === null) {
    errors.push('Mapping must be valid YAML.')
    return { config: null, errors }
  }

  const mappingErrors = validateMapping(mapping)
  errors.push(...mappingErrors)
  if (errors.length) return { config: null, errors }

  const config: SyncRemoteConfig = {
    provider,
    project: provider === 'jira' ? projectValue || null : null,
    repo: provider === 'github' ? repoValue || null : null,
    filter: remoteForm.value.filter.trim() || null,
    auth_profile: remoteForm.value.auth_profile.trim() || null,
    mapping: Object.keys(mapping).length ? mapping : {},
  }

  return { config, errors }
}

const mappingRows = computed(() => {
  const lines = remoteForm.value.mapping.split('\n').length
  return Math.min(Math.max(lines + 2, 8), 18)
})

const mappingErrors = computed(() => {
  const parsed = parseMappingInput(remoteForm.value.mapping)
  if (parsed === null) return ['Mapping must be valid YAML.']
  return validateMapping(parsed)
})

const mappingErrorPreview = computed(() => mappingErrors.value.slice(0, 3))

function validateMapping(mapping: Record<string, SyncFieldMapping>): string[] {
  const errors: string[] = []
  const allowedKeys = new Set(['field', 'values', 'set', 'default', 'add', 'when_empty'])
  for (const [localField, value] of Object.entries(mapping)) {
    if (!localField.trim()) {
      errors.push('Mapping keys cannot be empty.')
      continue
    }
    if (typeof value === 'string') {
      continue
    }
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      errors.push(`Mapping for ${localField} must be a string or mapping.`)
      continue
    }
    const detail = value as Record<string, unknown>
    for (const key of Object.keys(detail)) {
      if (!allowedKeys.has(key)) {
        errors.push(`Mapping for ${localField} has unsupported key '${key}'.`)
      }
    }
    if (detail.values && (typeof detail.values !== 'object' || Array.isArray(detail.values))) {
      errors.push(`Mapping for ${localField} values must be a key/value object.`)
    }
    if (detail.values && typeof detail.values === 'object' && !Array.isArray(detail.values)) {
      for (const [mapKey, mapValue] of Object.entries(detail.values)) {
        if (!String(mapKey).trim() || typeof mapValue !== 'string') {
          errors.push(`Mapping for ${localField} values must map strings to strings.`)
          break
        }
      }
    }
    if (detail.add && !Array.isArray(detail.add)) {
      errors.push(`Mapping for ${localField} add must be a list of strings.`)
    }
    if (Array.isArray(detail.add) && detail.add.some((item) => typeof item !== 'string')) {
      errors.push(`Mapping for ${localField} add must be a list of strings.`)
    }
    if (detail.when_empty && detail.when_empty !== 'skip' && detail.when_empty !== 'clear') {
      errors.push(`Mapping for ${localField} when_empty must be 'skip' or 'clear'.`)
    }
  }
  return errors
}

function formatMappingErrors(errors: string[]): string {
  const preview = errors.slice(0, 3)
  if (errors.length <= preview.length) return preview.join(' ')
  return `${preview.join(' ')} (${errors.length - preview.length} more)`
}

async function submitRemoteDialog() {
  if (remoteDialogSubmitting.value) return
  remoteFormError.value = null

  const name = remoteForm.value.name.trim()
  if (!name) {
    remoteFormError.value = 'Remote name is required.'
    return
  }

  const { config, errors } = buildRemoteConfigFromForm()
  if (!config || errors.length) {
    remoteFormError.value = formatMappingErrors(errors)
    return
  }

  const remote = config

  const updatedRemotes: Record<string, SyncRemoteConfig> = { ...scopedRemotes.value }
  const originalName = remoteFormOriginalName.value
  if (originalName && originalName !== name) {
    delete updatedRemotes[originalName]
  }
  updatedRemotes[name] = remote

  const remotesPayload = Object.keys(updatedRemotes).length
    ? stringifyYaml(updatedRemotes).trim()
    : ''

  remoteDialogSubmitting.value = true
  try {
    const payload = project.value
      ? { values: { remotes: remotesPayload }, project: project.value }
      : { values: { remotes: remotesPayload }, global: true }
    const response = await api.setConfig(payload)
    if (response.errors?.length) {
      remoteFormError.value = response.errors.join(' ')
      return
    }
    await reload()
    remoteDialogOpen.value = false
    showToast(response.warnings?.length ? 'Remote saved with warnings' : 'Remote saved')
  } catch (error: any) {
    remoteFormError.value = error?.message || 'Failed to save remote'
  } finally {
    remoteDialogSubmitting.value = false
  }
}

async function validateRemoteDialog() {
  if (remoteDialogValidating.value) return
  remoteFormError.value = null
  remoteDialogValidationStatus.value = 'idle'
  remoteDialogValidationMessage.value = null

  const { config, errors } = buildRemoteConfigFromForm()
  if (!config || errors.length) {
    remoteFormError.value = formatMappingErrors(errors)
    return
  }

  remoteDialogValidating.value = true
  try {
    const payload = {
      remote: remoteForm.value.name.trim() || undefined,
      project: project.value || undefined,
      auth_profile: remoteForm.value.auth_profile.trim() || undefined,
      remote_config: config,
    }
    const result = await api.syncValidate(payload)
    const warningCount = result.warnings?.length ?? 0
    remoteDialogValidationStatus.value = warningCount ? 'warn' : 'ok'
    remoteDialogValidationMessage.value = warningCount
      ? `Validated with ${warningCount} warning${warningCount === 1 ? '' : 's'}`
      : 'Validated'
  } catch (error: any) {
    remoteFormError.value = error?.message || 'Validation failed'
    remoteDialogValidationStatus.value = 'idle'
    remoteDialogValidationMessage.value = null
  } finally {
    remoteDialogValidating.value = false
  }
}

type SyncAction = 'pull' | 'push' | 'check'

type SyncRun = {
  id: string
  remote: string
  action: SyncAction
  actionLabel: string
  status: 'running' | 'success' | 'error'
  startedAt: string
  finishedAt?: string
  summary?: SyncResponse['summary']
  report?: SyncReportMeta | null
  reportEntries?: SyncReportEntry[]
  dry_run?: boolean
  error?: string
  warnings?: string[]
  info?: string[]
}

type SyncLiveEvent = SyncReportEntry & {
  runId: string
  remote: string
  action: SyncAction
}

type ReportListItem = SyncReportMeta & {
  runId?: string
  entries?: SyncReportEntry[]
}

const syncRuns = ref<SyncRun[]>([])
const liveEvents = ref<SyncLiveEvent[]>([])
const writeReport = ref(true)

const reportsLoading = ref(false)
const reportsError = ref<string | null>(null)
const reports = ref<SyncReportMeta[]>([])
const selectedReport = ref<SyncReport | null>(null)
const selectedReportPath = ref<string | null>(null)
const reportRangeStart = ref('')
const reportRangeEnd = ref('')
const reportEntryFilter = ref<'all' | SyncReportStatus>('all')
const reportEntrySearch = ref('')

function isRemoteBusy(name: string): boolean {
  return syncRuns.value.some((run) => run.remote === name && run.status === 'running')
}

function runStatus(name: string): SyncRun['status'] | null {
  return lastRunByRemote.value[name]?.status ?? null
}

function runStatusLabel(name: string): string {
  const status = runStatus(name)
  return status ? statusLabel(status) : ''
}

function runStatusClass(name: string): string {
  const status = runStatus(name)
  return status ? statusClass(status) : 'pill--muted'
}

function statusLabel(status: SyncRun['status']): string {
  if (status === 'running') return 'Running'
  if (status === 'error') return 'Failed'
  if (status === 'success') return 'Success'
  return ''
}

function statusClass(status: SyncRun['status']): string {
  if (status === 'success') return 'pill--success'
  if (status === 'error') return 'pill--danger'
  return 'pill--muted'
}

function reportStatusLabel(status?: string | null): string {
  if (!status) return ''
  const normalized = status.toLowerCase()
  if (normalized === 'success' || normalized === 'ok') return 'Success'
  if (normalized === 'failed') return 'Failed'
  if (normalized === 'running') return 'Running'
  return normalized.replace(/_/g, ' ').replace(/\b\w/g, (ch) => ch.toUpperCase())
}

function reportActionLabel(report: ReportListItem): string {
  return report.direction.toUpperCase()
}

function reportSummaryText(summary: SyncResponse['summary']): string {
  const parts: string[] = []
  if (summary.created) parts.push(`${summary.created} created`)
  if (summary.updated) parts.push(`${summary.updated} updated`)
  if (summary.skipped) parts.push(`${summary.skipped} skipped`)
  if (summary.failed) parts.push(`${summary.failed} failed`)
  return parts.length ? parts.join(' · ') : 'No changes'
}

function reportSummaryLabel(report: ReportListItem): string {
  const summary = reportSummaryText(report.summary)
  if (report.status?.toLowerCase() === 'running') {
    return summary === 'No changes' ? 'In progress…' : summary
  }
  return summary
}

function reportStatusClass(status?: string | null): string {
  if (!status) return 'pill--muted'
  const normalized = status.toLowerCase()
  if (normalized === 'success' || normalized === 'ok') return 'pill--success'
  if (normalized === 'failed') return 'pill--danger'
  if (normalized === 'running') return 'pill--info'
  return 'pill--muted'
}

function openTaskFromId(taskId?: string | null) {
  const trimmed = String(taskId ?? '').trim()
  if (!trimmed) return
  openTaskPanel({ taskId: trimmed })
}

function openTaskFromEntry(entry: SyncReportEntry) {
  openTaskFromId(entry.task_id)
}

function findTaskIdForRun(run: SyncRun): string | null {
  const fromEntries = run.reportEntries?.find((entry) => entry.task_id)?.task_id
  if (fromEntries) return fromEntries
  const fromLive = liveEvents.value.find((event) => event.runId === run.id && event.task_id)?.task_id
  return fromLive || null
}

function openRunTask(run: SyncRun) {
  openTaskFromId(findTaskIdForRun(run))
}

function openLatestTaskByRemote(remote: string) {
  const run = lastRunByRemote.value[remote]
  if (!run) return
  openRunTask(run)
}

const lastRunByRemote = computed<Record<string, SyncRun>>(() => {
  const map: Record<string, SyncRun> = {}
  for (const run of syncRuns.value) {
    if (!map[run.remote]) {
      map[run.remote] = run
    }
  }
  return map
})

const reportsDir = computed(() => inspectData.value?.effective?.sync_reports_dir || '@reports')
const reportsDirLabel = computed(() => {
  const dir = String(reportsDir.value || '@reports').trim()
  if (!dir) return '.tasks/@reports'
  if (dir.startsWith('/')) {
    return dir.replace(/\/+$/, '')
  }
  const cleaned = dir.replace(/^\/+/, '').replace(/\/+$/, '')
  return `.tasks/${cleaned}`
})

const emptySummary: SyncResponse['summary'] = { created: 0, updated: 0, skipped: 0, failed: 0 }

function reportStatusFromRun(run: SyncRun): string {
  if (run.status === 'running') return 'running'
  if (run.status === 'error') return 'failed'
  return run.report?.status || 'success'
}

function buildReportItemFromRun(run: SyncRun): ReportListItem {
  const report = run.report
  const provider = report?.provider || effectiveRemotes.value?.[run.remote]?.provider || 'jira'
  const direction = report?.direction || (run.action === 'check' ? 'pull' : run.action)
  const summary = run.summary || report?.summary || emptySummary
  return {
    id: report?.id || run.id,
    created_at: report?.created_at || run.startedAt,
    status: reportStatusFromRun(run),
    direction,
    provider,
    remote: report?.remote || run.remote,
    project: report?.project || project.value || null,
    dry_run: report?.dry_run ?? run.dry_run ?? run.action === 'check',
    summary,
    warnings: report?.warnings || run.warnings || [],
    info: report?.info || run.info || [],
    entries_total: report?.entries_total ?? run.reportEntries?.length ?? 0,
    stored_path: report?.stored_path || null,
    runId: run.id,
    entries: run.reportEntries,
  }
}

const reportListItems = computed<ReportListItem[]>(() => {
  const map = new Map<string, ReportListItem>()
  reports.value.forEach((report) => {
    map.set(report.id, { ...report })
  })
  syncRuns.value.forEach((run) => {
    const item = buildReportItemFromRun(run)
    const existing = map.get(item.id)
    if (!existing) {
      map.set(item.id, item)
      return
    }
    const merged: ReportListItem = {
      ...existing,
      ...item,
      stored_path: existing.stored_path || item.stored_path || null,
      entries_total: Math.max(existing.entries_total ?? 0, item.entries_total ?? 0),
      entries: item.entries || existing.entries,
    }
    if (run.status === 'running') {
      merged.status = 'running'
    }
    map.set(item.id, merged)
  })

  return Array.from(map.values()).sort((a, b) => {
    const aTime = new Date(a.created_at).getTime()
    const bTime = new Date(b.created_at).getTime()
    if (Number.isNaN(aTime) || Number.isNaN(bTime)) return 0
    return bTime - aTime
  })
})

const filteredReportItems = computed(() => {
  const start = parseReportRangeValue(reportRangeStart.value)
  const end = parseReportRangeValue(reportRangeEnd.value)
  return reportListItems.value.filter((report) => {
    const created = new Date(report.created_at)
    if (Number.isNaN(created.getTime())) return true
    if (start && created < start) return false
    if (end && created > end) return false
    return true
  })
})

const filteredReportEntries = computed(() => {
  const report = selectedReport.value
  if (!report) return []
  const statusFilter = reportEntryFilter.value
  const query = reportEntrySearch.value.trim().toLowerCase()
  return report.entries.filter((entry) => {
    if (statusFilter !== 'all' && entry.status !== statusFilter) return false
    if (!query) return true
    const haystack = [entry.task_id, entry.reference, entry.title, entry.message]
      .filter(Boolean)
      .join(' ')
      .toLowerCase()
    return haystack.includes(query)
  })
})

function parseReportRangeValue(value: string): Date | null {
  const trimmed = value.trim()
  if (!trimmed) return null
  const parsed = new Date(trimmed)
  if (Number.isNaN(parsed.getTime())) return null
  return parsed
}

const selectedReportFields = computed(() => {
  const report = selectedReport.value
  if (!report) return []
  const remote = effectiveRemotes.value?.[report.remote]
  const mapping = remote?.mapping ?? {}
  const fields = Object.entries(mapping).map(([local, detail]) => {
    if (typeof detail === 'string') {
      return local === detail ? local : `${local} → ${detail}`
    }
    const remoteField = detail?.field || local
    return remoteField === local ? local : `${local} → ${remoteField}`
  })
  return fields.sort((a, b) => a.localeCompare(b))
})

function ensureWriteReportDefault() {
  const configured = inspectData.value?.effective?.sync_write_reports
  if (typeof configured === 'boolean') {
    writeReport.value = configured
  }
}

async function loadReports() {
  reportsLoading.value = true
  reportsError.value = null
  try {
    const payload = await api.syncReportsList({ project: project.value || undefined })
    reports.value = payload.reports
  } catch (err: any) {
    reportsError.value = err?.message || 'Failed to load reports'
  } finally {
    reportsLoading.value = false
  }
}

function buildReportFromItem(item: ReportListItem, entries: SyncReportEntry[]): SyncReport {
  return {
    id: item.id,
    created_at: item.created_at,
    status: item.status,
    direction: item.direction,
    provider: item.provider,
    remote: item.remote,
    project: item.project,
    dry_run: item.dry_run,
    summary: item.summary,
    warnings: item.warnings ?? [],
    info: item.info ?? [],
    entries,
  }
}

function openReportItem(item: ReportListItem) {
  if (item.stored_path) {
    openReport(item)
    return
  }

  const run = item.runId ? syncRuns.value.find((entry) => entry.id === item.runId) : null
  const entries = run?.reportEntries || item.entries || []
  selectedReport.value = buildReportFromItem(item, entries)
  selectedReportPath.value = item.stored_path || null
  reportsError.value = null
}

async function openReport(meta: SyncReportMeta) {
  if (!meta?.stored_path) {
    selectedReport.value = null
    selectedReportPath.value = null
    reportsError.value = 'Report file not available for this run.'
    return
  }
  selectedReportPath.value = meta.stored_path
  reportsError.value = null
  try {
    const report = await api.syncReportGet(meta.stored_path)
    selectedReport.value = report
  } catch (err: any) {
    selectedReport.value = null
    reportsError.value = err?.message || 'Failed to load report'
  }
}

function upsertRun(runId: string, updater: (run: SyncRun) => SyncRun) {
  const idx = syncRuns.value.findIndex((run) => run.id === runId)
  if (idx === -1) {
    syncRuns.value = [updater({
      id: runId,
      remote: 'unknown',
      action: 'pull',
      actionLabel: 'PULL',
      status: 'running',
      startedAt: new Date().toISOString(),
    }), ...syncRuns.value].slice(0, 20)
    return
  }
  const updated = updater(syncRuns.value[idx])
  const next = syncRuns.value.slice()
  next.splice(idx, 1, updated)
  syncRuns.value = next
}

function handleSyncStarted(payload: any) {
  const runId = String(payload?.run_id || '').trim()
  const direction = String(payload?.direction || 'pull') as SyncAction
  if (!runId) return
  upsertRun(runId, (run) => ({
    ...run,
    id: runId,
    remote: String(payload?.remote || run.remote),
    action: direction,
    actionLabel: direction.toUpperCase(),
    status: 'running',
    startedAt: String(payload?.started_at || run.startedAt || new Date().toISOString()),
    dry_run: typeof payload?.dry_run === 'boolean' ? payload.dry_run : run.dry_run,
    error: undefined,
  }))
}

function handleSyncProgress(payload: any) {
  const runId = String(payload?.run_id || '').trim()
  const direction = String(payload?.direction || 'pull') as SyncAction
  const entry = payload?.entry as SyncReportEntry | undefined
  if (!runId || !entry) return
  upsertRun(runId, (run) => {
    const nextEntries = [entry, ...(run.reportEntries || [])].slice(0, 200)
    return {
      ...run,
      action: direction,
      actionLabel: direction.toUpperCase(),
      summary: payload?.summary || run.summary,
      dry_run: typeof payload?.dry_run === 'boolean' ? payload.dry_run : run.dry_run,
      reportEntries: nextEntries,
    }
  })

  liveEvents.value = [
    {
      ...entry,
      runId,
      remote: String(payload?.remote || 'unknown'),
      action: direction,
    },
    ...liveEvents.value,
  ].slice(0, 50)

  if (selectedReport.value?.id === runId && !selectedReportPath.value) {
    selectedReport.value = {
      ...selectedReport.value,
      status: 'running',
      dry_run:
        typeof payload?.dry_run === 'boolean'
          ? payload.dry_run
          : selectedReport.value.dry_run,
      summary: payload?.summary || selectedReport.value.summary,
      entries: [entry, ...selectedReport.value.entries].slice(0, 200),
    }
  }
}

function handleSyncCompleted(payload: any) {
  const runId = String(payload?.run_id || '').trim()
  const report = payload?.report as SyncReportMeta | undefined
  if (!runId) return
  upsertRun(runId, (run) => ({
    ...run,
    status: 'success',
    finishedAt: String(payload?.finished_at || new Date().toISOString()),
    summary: report?.summary || run.summary,
    warnings: report?.warnings || run.warnings,
    info: report?.info || run.info,
    report: report || run.report,
    dry_run: report?.dry_run ?? run.dry_run,
  }))
  if (selectedReport.value?.id === runId && !selectedReportPath.value) {
    if (report?.stored_path) {
      openReport(report)
    } else if (selectedReport.value) {
      selectedReport.value = {
        ...selectedReport.value,
        status: report?.status || 'success',
        summary: report?.summary || selectedReport.value.summary,
        warnings: report?.warnings || selectedReport.value.warnings,
        info: report?.info || selectedReport.value.info,
        dry_run: report?.dry_run ?? selectedReport.value.dry_run,
      }
    }
  }
  loadReports()
}

function handleSyncFailed(payload: any) {
  const runId = String(payload?.run_id || '').trim()
  const message = String(payload?.error || 'Sync failed')
  if (!runId) return
  upsertRun(runId, (run) => ({
    ...run,
    status: 'error',
    finishedAt: String(payload?.finished_at || new Date().toISOString()),
    error: message,
  }))
  if (selectedReport.value?.id === runId && !selectedReportPath.value) {
    selectedReport.value = {
      ...selectedReport.value,
      status: 'failed',
      warnings: [...(selectedReport.value.warnings || []), message],
    }
  }
}

let sse: { es: EventSource; close(): void; on(event: string, handler: (e: MessageEvent) => void): void; off(event: string, handler: (e: MessageEvent) => void): void } | null = null
const sseUnsubscribers: Array<() => void> = []

function setupSse() {
  sseUnsubscribers.splice(0).forEach((fn) => fn())
  if (sse) sse.close()

  const params: Record<string, string> = {
    kinds: 'sync_started,sync_progress,sync_completed,sync_failed',
  }
  if (project.value) {
    params.project = project.value
  }
  sse = useSse('/api/events', params)

  const bindings: Array<[string, (payload: any) => void]> = [
    ['sync_started', handleSyncStarted],
    ['sync_progress', handleSyncProgress],
    ['sync_completed', handleSyncCompleted],
    ['sync_failed', handleSyncFailed],
  ]
  bindings.forEach(([kind, handler]) => {
    const wrapped = (ev: MessageEvent) => {
      if (!ev.data) return
      try {
        const payload = JSON.parse(ev.data)
        handler(payload)
      } catch (err) {
        console.warn('Failed to parse sync SSE payload', err)
      }
    }
    sse?.on(kind, wrapped)
    sseUnsubscribers.push(() => sse?.off(kind, wrapped))
  })
}

async function runSync(action: SyncAction, entry: { name: string; remote: SyncRemoteConfig }) {
  if (!entry?.name) return
  const remoteName = entry.name
  if (isRemoteBusy(remoteName)) return
  const actionLabel = action === 'check' ? 'CHECK' : action.toUpperCase()
  const runId = `sync-${remoteName}-${Date.now()}`
  const run: SyncRun = {
    id: runId,
    remote: remoteName,
    action,
    actionLabel,
    status: 'running',
    startedAt: new Date().toISOString(),
    dry_run: action === 'check',
  }
  syncRuns.value = [run, ...syncRuns.value].slice(0, 20)

  try {
    const projectOverride = scope.value === 'project' ? project.value || undefined : undefined
    if (action !== 'push' && !projectOverride) {
      const defaultProject = String(inspectData.value?.global_effective?.default_project ?? '').trim()
      if (!defaultProject) {
        if (entry.remote.provider === 'jira') {
          const jiraProject = String(entry.remote.project ?? '').trim()
          if (jiraProject) {
            showToast(`Pull without project will use Jira project ${jiraProject} as the local prefix.`)
          } else {
            showToast('Pull requires a project scope or default_project.')
            throw new Error('Project scope required')
          }
        } else {
          showToast('Pull for GitHub requires a project scope or default_project.')
          throw new Error('Project scope required')
        }
      }
    }

    const payload: { remote: string; project?: string; dry_run?: boolean; include_report?: boolean; write_report?: boolean; client_run_id?: string } = {
      remote: remoteName,
      project: projectOverride,
    }
    if (action === 'check') {
      payload.dry_run = true
    }
    payload.include_report = true
    payload.write_report = writeReport.value
    payload.client_run_id = runId

    const result = action === 'push'
      ? await api.syncPush(payload)
      : await api.syncPull(payload)

    syncRuns.value = syncRuns.value.map((entry) =>
      entry.id === run.id
        ? {
          ...entry,
          status: 'success',
          finishedAt: new Date().toISOString(),
          summary: result.summary,
          warnings: result.warnings,
          info: result.info,
          report: result.report,
          reportEntries: result.report_entries,
          dry_run: result.dry_run,
        }
        : entry,
    )

    if (result.report && result.report_entries?.length) {
      selectedReport.value = {
        ...result.report,
        entries: result.report_entries,
      }
      selectedReportPath.value = result.report.stored_path || null
    } else if (result.report?.stored_path) {
      await openReport(result.report)
    }
    await loadReports()

    showToast(
      `${actionLabel} ${result.remote}: ${result.summary.created} created, ${result.summary.updated} updated, ${result.summary.skipped} skipped, ${result.summary.failed} failed`,
    )
    if (result.warnings?.length) {
      result.warnings.forEach((warning) => showToast(warning))
    }
    if (result.info?.length) {
      result.info.forEach((note) => showToast(note))
    }
  } catch (err: any) {
    const message = err?.message || `Failed to ${action} ${remoteName}`
    syncRuns.value = syncRuns.value.map((entry) =>
      entry.id === run.id
        ? {
          ...entry,
          status: 'error',
          finishedAt: new Date().toISOString(),
          error: message,
        }
        : entry,
    )
    showToast(message)
  }
}

watch(
  () => inspectData.value?.effective?.sync_write_reports,
  () => ensureWriteReportDefault(),
  { immediate: true },
)

watch(
  project,
  async () => {
    setupSse()
    await loadReports()
  },
  { immediate: true },
)

onUnmounted(() => {
  sseUnsubscribers.splice(0).forEach((fn) => fn())
  if (sse) sse.close()
})

function formatTimestamp(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

async function handleReload() {
  await reload()
}
</script>

<style scoped>
.sync-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding-bottom: 48px;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
  padding: 20px;
  flex-wrap: wrap;
}

.page-headings h1 {
  margin: 0;
  font-size: 26px;
}

.page-headings p {
  margin: 4px 0 0;
}

.page-actions {
  display: flex;
  gap: 12px;
  align-items: flex-end;
  flex-wrap: wrap;
}

.scope-picker {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.sync-dashboard {
  display: grid;
  grid-template-columns: minmax(320px, 1fr) minmax(420px, 2fr);
  grid-template-areas: "remotes reports";
  gap: 16px;
  align-items: start;
}

.sync-card {
  min-width: 0;
}

.sync-card--remotes {
  grid-area: remotes;
}

.sync-card--reports {
  grid-area: reports;
}

.sync-loading {
  padding: 16px 20px;
}

.sync-error {
  padding: 0 20px;
  color: var(--color-danger);
}

.card-header {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: flex-start;
  flex-wrap: wrap;
}

.card-header h3 {
  margin: 0;
}

.sync-card--remotes .card-header {
  margin-bottom: 8px;
}

.card-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.card-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.sync-remote-dialog__overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
  background: var(--color-dialog-overlay);
  z-index: var(--z-modal);
}

.sync-remote-dialog__card {
  width: min(640px, 100%);
  max-height: calc(100vh - 32px);
  overflow-y: auto;
}

.sync-remote-dialog__form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.sync-remote-dialog__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.sync-remote-dialog__field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.sync-remote-dialog__label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.sync-remote-dialog__help {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 999px;
  border: 1px solid var(--color-border);
  background: transparent;
  color: var(--color-muted);
  padding: 0;
  cursor: pointer;
}

.sync-remote-dialog__help:hover {
  border-color: var(--color-accent);
  color: var(--color-accent);
}

.sync-remote-dialog__hint {
  margin: 0;
}

.remote-stack {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.remote-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  padding: 8px 10px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  background: var(--color-surface);
  flex-wrap: wrap;
}

.remote-main {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1 1 260px;
  min-width: 220px;
}

.remote-title {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.remote-provider {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 0.85rem;
  color: var(--color-muted);
}

.remote-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 12px;
  font-size: 0.85rem;
}

.remote-meta__item {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
}

.remote-meta__label {
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.remote-meta__value {
  font-weight: 600;
}

.remote-actions {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.remote-action {
  padding: 4px 10px;
  font-size: 0.8rem;
  border-radius: 8px;
}

.connections-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 12px;
}

.connection-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--color-border);
  border-radius: 12px;
  background: var(--color-surface);
}

.connection-card__header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
}

.connection-card__body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.connection-form,
.connection-status {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.connection-device {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border-radius: 10px;
  border: 1px dashed var(--color-border);
  background: color-mix(in oklab, var(--color-surface) 70%, transparent);
}

.device-code {
  font-family: var(--font-mono);
  font-size: 1.25rem;
  letter-spacing: 0.2em;
  padding: 8px 12px;
  border-radius: 8px;
  background: var(--color-surface-contrast);
  border: 1px solid var(--color-border);
  text-align: center;
}

.link-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 8px;
  border: 1px solid var(--color-border);
  background: var(--color-surface-contrast);
  color: var(--color-fg);
  font-weight: 600;
  text-decoration: none;
}

.link-button:hover {
  text-decoration: none;
  border-color: var(--color-accent);
  color: var(--color-accent);
}

.pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  font-size: 0.7rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  border: 1px solid transparent;
}

.pill--success {
  color: var(--color-success-strong);
  background: color-mix(in oklab, var(--color-success) 20%, transparent);
  border-color: color-mix(in oklab, var(--color-success) 50%, var(--color-border));
}

.pill--info {
  color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 18%, transparent);
  border-color: color-mix(in oklab, var(--color-accent) 40%, var(--color-border));
}

.pill--danger {
  color: var(--color-danger);
  background: color-mix(in oklab, var(--color-danger) 18%, transparent);
  border-color: color-mix(in oklab, var(--color-danger) 45%, var(--color-border));
}

.pill--muted {
  color: var(--color-muted);
  background: color-mix(in oklab, var(--color-muted) 10%, transparent);
  border-color: color-mix(in oklab, var(--color-border) 70%, transparent);
}

.pill--interactive {
  cursor: pointer;
}

.pill--interactive:hover {
  color: var(--color-accent);
  border-color: color-mix(in oklab, var(--color-accent) 40%, var(--color-border));
}

.connection-select {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.connection-generator {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.generator-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 12px;
}

.generator-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
  border: 1px solid var(--color-border);
  border-radius: 12px;
  background: var(--color-surface);
}

.generator-card h5 {
  margin: 0;
}

.code-block {
  margin: 0;
  padding: 12px;
  border-radius: 10px;
  background: var(--color-surface-contrast);
  border: 1px solid var(--color-border);
  font-family: var(--font-mono);
  font-size: 0.8rem;
  white-space: pre-wrap;
  word-break: break-word;
}

.list-stack {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.list-item {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  background: var(--color-surface);
  flex-wrap: wrap;
}

.list-item.interactive {
  cursor: pointer;
  transition: border-color 150ms ease, background 150ms ease;
}

.list-item.interactive:hover {
  border-color: color-mix(in oklab, var(--color-accent) 45%, var(--color-border));
  background: color-mix(in oklab, var(--color-surface) 92%, var(--color-accent) 8%);
}

.list-item__details {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 8px;
  flex: 1 1 320px;
}

.list-item__actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.detail {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 0.85rem;
}

.detail .muted {
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.option-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.9rem;
}

.option-row--inline {
  flex-wrap: wrap;
}

.option-row__hint {
  font-size: 0.8rem;
}

.reports-toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: flex-end;
}

.reports-filter {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
  width: auto;
}

.reports-filter--range {
  flex: 1 1 360px;
  min-width: 210px;
}

.reports-filter--status {
  flex: 0 0 160px;
}

.reports-filter--search {
  flex: 1 1 240px;
  max-width: 320px;
}

.reports-range {
  display: grid;
  grid-template-columns: auto auto auto;
  align-items: center;
  gap: 8px;
}


@media (max-width: 900px) {
  .reports-toolbar {
    flex-direction: column;
    align-items: stretch;
  }

  .reports-filter--status,
  .reports-filter--search,
  .reports-filter--range {
    max-width: 100%;
    flex: 1 1 auto;
  }

  .reports-range {
    grid-template-columns: 1fr;
  }

  .reports-range span {
    justify-self: center;
  }
}

.reports-grid {
  display: grid;
  grid-template-columns: minmax(220px, 1fr) minmax(340px, 2fr);
  gap: 16px;
  min-height: 320px;
}

.reports-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-height: clamp(240px, 45vh, 520px);
  overflow: auto;
  padding-right: 4px;
}

.report-summary {
  font-size: 0.75rem;
  color: var(--color-muted);
  white-space: normal;
  margin-left: auto;
  text-align: right;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  max-width: 260px;
}

.summary-chip {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  font-size: 0.75rem;
  background: color-mix(in oklab, var(--color-surface-contrast) 80%, transparent);
  border: 1px solid var(--color-border);
  color: var(--color-muted);
}

.report-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 10px;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  text-align: left;
}

.report-item__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.report-item__chips {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.report-item__date {
  font-size: 0.75rem;
  text-align: right;
}

.report-item__row--name {
  justify-content: space-between;
  align-items: baseline;
  gap: 12px;
}

.report-item__status-chip {
  font-size: 0.7rem;
}

.report-item__action-chip {
  font-size: 0.7rem;
}

.report-item.active {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 1px color-mix(in oklab, var(--color-accent) 30%, transparent);
}

.reports-detail {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 320px;
}

.report-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
}

.report-header__main {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.report-header__status {
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.report-fields {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 10px 12px;
  margin: 12px 0;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: color-mix(in oklab, var(--color-surface) 90%, transparent);
}

.report-fields__label {
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.report-fields__list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.field-chip {
  display: inline-flex;
  align-items: center;
  padding: 2px 6px;
  border-radius: 8px;
  font-size: 0.7rem;
  border: 1px solid var(--color-border);
  background: color-mix(in oklab, var(--color-surface-contrast) 70%, transparent);
  color: var(--color-muted);
}

.report-path {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  padding: 4px 8px;
  border-radius: 8px;
  border: 1px solid var(--color-border);
  background: color-mix(in oklab, var(--color-surface-contrast) 70%, transparent);
  font-family: var(--font-mono);
  font-size: 0.75rem;
  margin-bottom: 8px;
}

.report-path__label {
  font-size: 0.65rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--color-muted);
}

.report-path__value {
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.reports-entries {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 8px;
  max-height: clamp(240px, 45vh, 520px);
  overflow: auto;
  padding-right: 4px;
}

.report-entry .list-item__details {
  grid-template-columns: minmax(90px, 120px) minmax(160px, 1fr) minmax(220px, 1.2fr);
  flex: 0 0 auto;
  width: 100%;
}

.report-entry {
  flex-direction: column;
  align-items: stretch;
}

.report-entry__row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}

.report-entry__date {
  margin-left: auto;
  font-size: 0.75rem;
}

.form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.form-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 10px;
}

.form-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.form-actions__group {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.validation-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 0.85rem;
  font-weight: 600;
}

.validation-status--ok {
  color: var(--color-success-strong);
}

.validation-status--warn {
  color: var(--color-accent);
}

.validation-status--muted {
  color: var(--color-muted);
}

.sync-textarea {
  width: 100%;
  min-height: 220px;
  resize: vertical;
}

.error {
  color: var(--color-danger);
}

@media (max-width: 1200px) {
  .sync-dashboard {
    grid-template-columns: 1fr;
    grid-template-areas:
      "remotes"
      "monitor"
      "reports";
  }
}

@media (max-width: 820px) {
  .reports-grid {
    grid-template-columns: 1fr;
  }
}
</style>
