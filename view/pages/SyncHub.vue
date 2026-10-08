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
          :loading="scopeLoading"
          label="Reload sync settings"
          title="Reload sync settings"
          @click="handleReload"
        />
      </div>
    </div>

    <p v-if="scopeLoadError" class="sync-error" data-testid="scope-load-error">{{ scopeLoadError }}</p>

    <div v-if="scopeLoading && !scopedInspect" class="sync-loading">
      <UiLoader>Loading sync settings…</UiLoader>
    </div>

    <div v-else class="sync-dashboard">
      <UiCard class="sync-card sync-card--remotes">
          <div class="card-header">
            <div>
              <h3>Remotes & actions</h3>
            </div>
            <div class="card-actions">
              <UiButton type="button" variant="primary" :disabled="!scopeValid" @click="openAddRemoteDialog">Add remote</UiButton>
              <label class="option-row option-row--inline">
                <input v-model="writeReport" type="checkbox" />
                <span>Write report to disk</span>
                <span class="muted option-row__hint">{{ reportsDirLabel }}</span>
              </label>
            </div>
          </div>

          <div class="card-body">
            <p v-if="!scopeValid" class="muted" data-testid="scope-unavailable">{{ scopeUnavailableHint }}</p>
            <p v-else-if="!remoteEntries.length" class="muted">No remotes configured for this scope.</p>
            <div v-else class="remote-stack">
              <div v-for="entry in remoteEntries" :key="entry.name" class="remote-row">
                <div class="remote-main">
                  <div class="remote-title">
                    <strong>{{ entry.name }}</strong>
                    <span
                      v-if="project && entry.origin === 'global'"
                      class="pill pill--muted remote-origin-chip"
                      title="Defined in the Global config; inherited by this project"
                    >inherited</span>
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
                  <UiButton class="remote-action" type="button" :disabled="isRemoteNameBusy(entry.name)" @click="runSync('pull', entry)">Pull</UiButton>
                  <UiButton class="remote-action" type="button" :disabled="isRemoteNameBusy(entry.name)" @click="runSync('push', entry)">Push</UiButton>
                  <UiButton class="remote-action" type="button" :disabled="isRemoteNameBusy(entry.name)" @click="runSync('check', entry)">Check</UiButton>
                  <UiButton class="remote-action" type="button" :disabled="isRemoteNameBusy(entry.name)" @click="openEditRemoteDialog(entry)">Edit</UiButton>
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
            <p v-if="reportsLoading && !reportListItems.length" class="muted">Loading reports…</p>
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
                      <span v-if="report.source === 'external'" class="pill pill--muted report-item__status-chip">External</span>
                      <span v-else-if="!project && report.project" class="pill pill--muted report-item__status-chip">{{ report.project }}</span>
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
  <UiModal
    :open="remoteDialogOpen"
    :aria-label="remoteDialogTitle"
    :aria-labelledby="remoteDialogHeadingId"
    :aria-describedby="remoteDialogTargetId"
    size="lg"
    :dismissible="!remoteDialogSubmitting && !remoteDialogDeleting"
    @close="closeRemoteDialog"
  >
    <form class="sync-remote-dialog__form" @submit.prevent="submitRemoteDialog">
          <header class="sync-remote-dialog__header">
            <h2 :id="remoteDialogHeadingId">{{ remoteDialogTitle }}</h2>
            <UiButton
              variant="ghost"
              icon-only
              type="button"
              :disabled="remoteDialogSubmitting || remoteDialogDeleting"
              aria-label="Close dialog"
              title="Close dialog"
              @click="closeRemoteDialog"
            >
              <IconGlyph name="close" />
            </UiButton>
          </header>

          <p :id="remoteDialogTargetId" class="sync-remote-dialog__target" data-testid="remote-dialog-target">{{ dialogTargetBanner }}</p>

          <div v-if="dialogNeedsTargetChoice" class="sync-remote-dialog__choice" data-testid="remote-dialog-choice">
            <p class="muted">This remote is inherited from the Global config. Choose where your changes apply:</p>
            <div class="sync-remote-dialog__choice-buttons">
              <UiButton type="button" variant="primary" @click="chooseDialogTarget('override-project')">
                Override in project {{ project }}
              </UiButton>
              <UiButton type="button" @click="chooseDialogTarget('inherit-global')">Edit in Global scope</UiButton>
            </div>
          </div>

          <fieldset
            class="sync-remote-dialog__fieldset"
            :disabled="remoteDialogSubmitting || remoteDialogValidating || remoteDialogDeleting || dialogNeedsTargetChoice"
          >
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
          </fieldset>
          <p class="muted sync-remote-dialog__hint">Saved to {{ dialogTargetLabel }} as YAML. Mapping is required to sync fields.</p>
          <p v-if="mappingErrorPreview.length" class="error">{{ mappingErrorPreview.join(' ') }}</p>
          <p v-if="remoteFormError" class="error" data-testid="remote-dialog-error">{{ remoteFormError }}</p>

          <footer class="form-actions">
            <div class="form-actions__group">
              <UiButton
                variant="primary"
                type="submit"
                :disabled="remoteDialogSubmitting || remoteDialogValidating || remoteDialogDeleting || dialogNeedsTargetChoice || editorBlocked || dialogTargetInvalid"
              >
                {{ remoteDialogSubmitting ? 'Saving…' : remoteDialogMode === 'add' ? 'Add remote' : 'Save remote' }}
              </UiButton>
              <UiButton
                variant="ghost"
                type="button"
                :disabled="remoteDialogSubmitting || remoteDialogValidating || remoteDialogDeleting || dialogNeedsTargetChoice || editorBlocked || dialogTargetInvalid"
                @click="validateRemoteDialog"
              >
                {{ remoteDialogValidating ? 'Validating…' : 'Validate' }}
              </UiButton>
              <span v-if="remoteDialogValidationMessage" :class="['validation-status', remoteDialogValidationClass]">
                <IconGlyph name="check" />
                {{ remoteDialogValidationMessage }}
              </span>
            </div>
            <div class="form-actions__group">
              <UiButton
                v-if="remoteDialogMode === 'edit' && dialogTargetScope === dialogDefOriginScope"
                variant="ghost"
                type="button"
                class="sync-remote-dialog__delete"
                :disabled="remoteDialogSubmitting || remoteDialogValidating || remoteDialogDeleting || dialogNeedsTargetChoice || editorBlocked || dialogTargetInvalid"
                @click="deleteRemoteDialog"
              >
                {{ remoteDialogDeleting ? 'Removing…' : `Remove from ${dialogTargetLabel}` }}
              </UiButton>
              <UiButton variant="ghost" type="button" :disabled="remoteDialogSubmitting || remoteDialogDeleting" @click="closeRemoteDialog">
                Cancel
              </UiButton>
            </div>
          </footer>
    </form>
    <datalist id="sync-auth-profile-options">
      <option v-for="profile in authProfileOptions" :key="profile" :value="profile" />
    </datalist>
  </UiModal>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, useId, watch } from 'vue'
import { parse as parseYaml, stringify as stringifyYaml } from 'yaml'
import { api } from '../api/client'
import type {
  ConfigInspectResult,
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
import UiModal from '../components/UiModal.vue'
import UiSelect from '../components/UiSelect.vue'
import { useConfigScope } from '../composables/useConfigScope'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import {
  useSyncRuns,
  type ExternalSyncRun,
  type SyncAction,
  type SyncRun,
  type SyncRunOriginContext,
  type SyncScope,
} from '../composables/useSyncRuns'
import { formatProjectLabel } from '../utils/projectLabels'

const { projects, project } = useConfigScope()
const { openTaskPanel } = useTaskPanelController()

const scope = computed<SyncScope>(() => project.value || '')

let alive = true

// ---- Scope-stamped config ----------------------------------------------------
//
// The shared useConfigScope keeps its last inspectData during reloads and on
// failures. Displaying remotes from it would show the previous scope's rows
// (clickable) while the new scope is still loading or has failed to load, and
// a run started from such a row would execute the NEW scope's definition.
// SyncHub therefore renders only from its own stamped snapshot: rows exist
// exactly when the displayed data belongs to the current scope.
const scopedInspect = ref<ConfigInspectResult | null>(null)
const inspectStamp = ref<SyncScope | null>(null)
const scopeLoading = ref(false)
const scopeLoadError = ref<string | null>(null)
let inspectGen = 0

const scopeValid = computed(() => inspectStamp.value !== null && inspectStamp.value === scope.value)

const scopeUnavailableHint = computed(() =>
  scopeLoading.value
    ? 'Loading sync settings for this scope…'
    : `Sync settings for ${scopeLabelFor(scope.value)} are unavailable (load failed or was superseded). Actions are disabled until a successful reload.`,
)

async function loadScopeConfig(scopeKey: SyncScope = scope.value) {
  const gen = ++inspectGen
  scopeLoading.value = true
  scopeLoadError.value = null
  try {
    const fresh = await api.inspectConfig(scopeKey || undefined)
    if (!alive || gen !== inspectGen) return
    scopedInspect.value = fresh
    inspectStamp.value = scopeKey
  } catch (err: any) {
    if (!alive || gen !== inspectGen) return
    // Keep the previous stamp: a failed load never validates a scope's rows.
    scopeLoadError.value = err?.message || `Failed to load ${scopeLabelFor(scopeKey)} config`
  } finally {
    if (gen === inspectGen) {
      scopeLoading.value = false
    }
  }
}

const globalRemotes = computed<Record<string, SyncRemoteConfig>>(() => scopedInspect.value?.global_raw?.remotes ?? {})
const projectRemotes = computed<Record<string, SyncRemoteConfig>>(() => scopedInspect.value?.project_raw?.remotes ?? {})
const effectiveRemotes = computed<Record<string, SyncRemoteConfig>>(() => scopedInspect.value?.effective?.remotes ?? {})

const remoteEntries = computed(() =>
  Object.entries(effectiveRemotes.value)
    .map(([name, remote]) => ({
      name,
      remote,
      origin: projectRemotes.value[name] ? ('project' as const) : ('global' as const),
    }))
    .sort((a, b) => a.name.localeCompare(b.name)),
)

// ---- Remote editor ----------------------------------------------------------

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
const remoteDialogDeleting = ref(false)
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

/** Bumped on every dialog open/close so late async results cannot leak across sessions. */
let remoteDialogGen = 0
/** Config scope the dialog will write to; captured when the dialog opens or an explicit target is chosen. */
const dialogTargetScope = ref<SyncScope>('')
/** Inherited remotes require an explicit override/edit-in-global choice before fields unlock. */
const dialogNeedsTargetChoice = ref(false)
const dialogRemoteInherited = ref(false)
/** Serialized remote definition at dialog open, used to detect concurrent edits (no server CAS exists). */
const dialogBaseDefYaml = ref<string | null>(null)
/**
 * Scope the edited definition actually lives in ('' global / project prefix).
 * When the dialog target differs from this scope the save is an override/add
 * in the target map, not a version-checked replacement.
 */
const dialogDefOriginScope = ref<SyncScope>('')

const filterHelpOpen = ref(false)

const authProfileOptions = computed(() => {
  const profiles = scopedInspect.value?.auth_profiles ?? {}
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

/** Stable ids binding the dialog's accessible name/description to its heading and target banner. */
const remoteDialogHeadingId = useId()
const remoteDialogTargetId = useId()

/** Saving is paused while the scope config is (re)loading, e.g. during a project switch. */
const editorBlocked = computed(() => scopeLoading.value)
/** A dialog targeting the current scope is blocked while that scope's data is not validated. */
const dialogTargetInvalid = computed(() => dialogTargetScope.value === scope.value && !scopeValid.value)

function scopeLabelFor(scopeKey: SyncScope): string {
  return scopeKey ? `Project ${scopeKey}` : 'Global'
}

const dialogTargetLabel = computed(() => scopeLabelFor(dialogTargetScope.value))

const dialogTargetBanner = computed(() => {
  if (dialogNeedsTargetChoice.value) {
    return 'Inherited from Global — choose where changes apply before editing.'
  }
  if (remoteDialogMode.value === 'add') return `Adds the remote to ${dialogTargetLabel.value} config.`
  if (dialogRemoteInherited.value && dialogTargetScope.value) {
    return `Saves a project override in ${dialogTargetLabel.value} config (inherited definition stays in Global).`
  }
  return `Saves the remote in ${dialogTargetLabel.value} config.`
})

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

function resetRemoteDialogFlags() {
  remoteDialogSubmitting.value = false
  remoteDialogValidating.value = false
  remoteDialogDeleting.value = false
}

function openAddRemoteDialog() {
  if (!scopeValid.value) return
  remoteDialogGen += 1
  remoteDialogMode.value = 'add'
  remoteFormOriginalName.value = null
  remoteFormError.value = null
  resetRemoteForm()
  resetRemoteDialogFlags()
  dialogTargetScope.value = scope.value
  dialogNeedsTargetChoice.value = false
  dialogRemoteInherited.value = false
  dialogBaseDefYaml.value = null
  dialogDefOriginScope.value = scope.value
  remoteDialogOpen.value = true
}

function openEditRemoteDialog(entry: { name: string; remote: SyncRemoteConfig; origin: 'project' | 'global' }) {
  if (!scopeValid.value) return
  remoteDialogGen += 1
  remoteDialogMode.value = 'edit'
  remoteFormOriginalName.value = entry.name
  remoteFormError.value = null
  filterHelpOpen.value = false
  remoteDialogValidationStatus.value = 'idle'
  remoteDialogValidationMessage.value = null
  resetRemoteDialogFlags()
  remoteForm.value = {
    name: entry.name,
    provider: entry.remote.provider,
    project: entry.remote.project ?? '',
    repo: entry.remote.repo ?? '',
    filter: entry.remote.filter ?? '',
    auth_profile: entry.remote.auth_profile ?? '',
    mapping: formatMapping(entry.remote.mapping),
  }
  dialogBaseDefYaml.value = stringifyYaml(remoteDefFromForm(entry.remote)).trim()
  const inherited = Boolean(scope.value) && entry.origin === 'global'
  dialogDefOriginScope.value = inherited ? '' : scope.value
  dialogRemoteInherited.value = inherited
  dialogNeedsTargetChoice.value = inherited
  dialogTargetScope.value = inherited ? '' : scope.value
  remoteDialogOpen.value = true
}

function remoteDefFromForm(remote: SyncRemoteConfig): SyncRemoteConfig {
  return {
    provider: remote.provider,
    project: remote.project ?? null,
    repo: remote.repo ?? null,
    filter: remote.filter ?? null,
    auth_profile: remote.auth_profile ?? null,
    mapping: remote.mapping ?? {},
  }
}

function chooseDialogTarget(target: 'override-project' | 'inherit-global') {
  dialogTargetScope.value = target === 'override-project' ? scope.value : ''
  dialogNeedsTargetChoice.value = false
  remoteFormError.value = null
  remoteDialogValidationStatus.value = 'idle'
  remoteDialogValidationMessage.value = null
}

function closeRemoteDialog() {
  if (remoteDialogSubmitting.value || remoteDialogDeleting.value) return
  remoteDialogGen += 1
  remoteDialogOpen.value = false
  dialogNeedsTargetChoice.value = false
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

/**
 * Fresh inspect of the CAPTURED target scope right before writing; merges the
 * single-remote delta into the latest server state. There is no server-side
 * compare-and-swap, so a small race window remains between this fetch and the
 * setConfig write; unrelated concurrent edits are preserved, and same-name
 * changes are detected and surfaced instead of silently overwritten.
 */
async function fetchFreshTargetRemotes(targetScope: SyncScope): Promise<Record<string, SyncRemoteConfig>> {
  const fresh = await api.inspectConfig(targetScope || undefined)
  const raw = targetScope ? fresh.project_raw : fresh.global_raw
  return raw?.remotes ?? {}
}

async function submitRemoteDialog() {
  if (remoteDialogSubmitting.value || remoteDialogValidating.value || remoteDialogDeleting.value) return
  if (dialogNeedsTargetChoice.value || dialogTargetInvalid.value) return
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

  const gen = remoteDialogGen
  const target = dialogTargetScope.value
  const originalName = remoteFormOriginalName.value
  const baseDefYaml = dialogBaseDefYaml.value

  remoteDialogSubmitting.value = true
  try {
    const freshRemotes = await fetchFreshTargetRemotes(target)
    if (!alive || gen !== remoteDialogGen || !remoteDialogOpen.value) return

    // Adding, renaming, or overriding in a different scope is an add in the
    // target map: the name must be free there. Only a replacement in the
    // definition's own scope is version-checked against the open snapshot.
    const defOrigin = remoteDialogMode.value === 'edit' ? dialogDefOriginScope.value : target
    const isAddInTarget = remoteDialogMode.value === 'add' || defOrigin !== target
    if ((isAddInTarget || (originalName && originalName !== name)) && freshRemotes[name]) {
      remoteFormError.value = `Remote '${name}' already exists in ${scopeLabelFor(target)} config.`
      return
    }

    if (remoteDialogMode.value === 'edit' && originalName && !isAddInTarget) {
      const freshDefYaml = freshRemotes[originalName]
        ? stringifyYaml(remoteDefFromForm(freshRemotes[originalName])).trim()
        : null
      if (freshDefYaml !== baseDefYaml) {
        remoteFormError.value =
          freshDefYaml === null
            ? `Remote '${originalName}' no longer exists in ${scopeLabelFor(target)} config; close and reopen.`
            : `Remote '${originalName}' changed in ${scopeLabelFor(target)} config since this dialog opened; close and reopen to merge.`
        return
      }
    }

    const merged: Record<string, SyncRemoteConfig> = { ...freshRemotes }
    if (remoteDialogMode.value === 'edit' && originalName && originalName !== name) {
      delete merged[originalName]
    }
    merged[name] = config

    const remotesPayload = Object.keys(merged).length ? stringifyYaml(merged).trim() : ''
    const response = await api.setConfig(
      target
        ? { values: { remotes: remotesPayload }, project: target }
        : { values: { remotes: remotesPayload }, global: true },
    )
    if (!alive || gen !== remoteDialogGen || !remoteDialogOpen.value) return
    if (response.errors?.length) {
      remoteFormError.value = response.errors.join(' ')
      return
    }
    remoteDialogOpen.value = false
    remoteDialogGen += 1
    await loadScopeConfig()
    showToast(response.warnings?.length ? 'Remote saved with warnings' : 'Remote saved')
  } catch (err: any) {
    if (alive && gen === remoteDialogGen && remoteDialogOpen.value) {
      remoteFormError.value = err?.message || 'Failed to save remote'
    }
  } finally {
    // Reset for the current session, or when the dialog already closed so a
    // stuck busy flag cannot leak into a later session.
    if (gen === remoteDialogGen || !remoteDialogOpen.value) {
      remoteDialogSubmitting.value = false
    }
  }
}

async function deleteRemoteDialog() {
  if (remoteDialogSubmitting.value || remoteDialogValidating.value || remoteDialogDeleting.value) return
  if (remoteDialogMode.value !== 'edit' || dialogNeedsTargetChoice.value || dialogTargetInvalid.value) return
  if (dialogTargetScope.value !== dialogDefOriginScope.value) return
  remoteFormError.value = null

  const gen = remoteDialogGen
  const target = dialogTargetScope.value
  const originalName = remoteFormOriginalName.value
  if (!originalName) return
  const baseDefYaml = dialogBaseDefYaml.value

  remoteDialogDeleting.value = true
  try {
    const freshRemotes = await fetchFreshTargetRemotes(target)
    if (!alive || gen !== remoteDialogGen || !remoteDialogOpen.value) return

    const freshDefYaml = freshRemotes[originalName]
      ? stringifyYaml(remoteDefFromForm(freshRemotes[originalName])).trim()
      : null
    if (freshDefYaml === null) {
      remoteFormError.value = `Remote '${originalName}' was already removed from ${scopeLabelFor(target)} config.`
      return
    }
    if (freshDefYaml !== baseDefYaml) {
      remoteFormError.value = `Remote '${originalName}' changed in ${scopeLabelFor(target)} config since this dialog opened; close and reopen.`
      return
    }

    const merged: Record<string, SyncRemoteConfig> = { ...freshRemotes }
    delete merged[originalName]
    const remotesPayload = Object.keys(merged).length ? stringifyYaml(merged).trim() : ''
    const response = await api.setConfig(
      target
        ? { values: { remotes: remotesPayload }, project: target }
        : { values: { remotes: remotesPayload }, global: true },
    )
    if (!alive || gen !== remoteDialogGen || !remoteDialogOpen.value) return
    if (response.errors?.length) {
      remoteFormError.value = response.errors.join(' ')
      return
    }
    remoteDialogOpen.value = false
    remoteDialogGen += 1
    await loadScopeConfig()
    showToast('Remote removed')
  } catch (err: any) {
    if (alive && gen === remoteDialogGen && remoteDialogOpen.value) {
      remoteFormError.value = err?.message || 'Failed to remove remote'
    }
  } finally {
    // Reset for the current session, or when the dialog already closed so a
    // stuck busy flag cannot leak into a later session.
    if (gen === remoteDialogGen || !remoteDialogOpen.value) {
      remoteDialogDeleting.value = false
    }
  }
}

async function validateRemoteDialog() {
  if (remoteDialogValidating.value || remoteDialogSubmitting.value || remoteDialogDeleting.value) return
  if (dialogNeedsTargetChoice.value || dialogTargetInvalid.value) return
  remoteFormError.value = null
  remoteDialogValidationStatus.value = 'idle'
  remoteDialogValidationMessage.value = null

  const { config, errors } = buildRemoteConfigFromForm()
  if (!config || errors.length) {
    remoteFormError.value = formatMappingErrors(errors)
    return
  }

  const gen = remoteDialogGen
  remoteDialogValidating.value = true
  try {
    const payload = {
      remote: remoteForm.value.name.trim() || undefined,
      project: dialogTargetScope.value || undefined,
      auth_profile: remoteForm.value.auth_profile.trim() || undefined,
      remote_config: config,
    }
    const result = await api.syncValidate(payload)
    if (!alive || gen !== remoteDialogGen || !remoteDialogOpen.value) return
    const warningCount = result.warnings?.length ?? 0
    remoteDialogValidationStatus.value = warningCount ? 'warn' : 'ok'
    remoteDialogValidationMessage.value = warningCount
      ? `Validated with ${warningCount} warning${warningCount === 1 ? '' : 's'}`
      : 'Validated'
  } catch (err: any) {
    if (alive && gen === remoteDialogGen && remoteDialogOpen.value) {
      remoteFormError.value = err?.message || 'Validation failed'
      remoteDialogValidationStatus.value = 'idle'
      remoteDialogValidationMessage.value = null
    }
  } finally {
    // Reset for the current session, or when the dialog already closed so a
    // stuck busy flag cannot leak into a later session.
    if (gen === remoteDialogGen || !remoteDialogOpen.value) {
      remoteDialogValidating.value = false
    }
  }
}

// ---- Sync runs (scoped state, persistent SSE) --------------------------------

type SyncLiveEvent = SyncReportEntry & {
  runId: string
  remote: string
  action: SyncAction
}

type ReportItemSource = 'listed' | 'run' | 'external'

type ReportListItem = SyncReportMeta & {
  runId?: string
  entries?: SyncReportEntry[]
  source: ReportItemSource
  /** Scope whose reports root resolves stored_path (query-scope, not report.project). */
  queryScope: SyncScope
}

const liveEvents = ref<SyncLiveEvent[]>([])
const writeReport = ref(true)

const reportsLoading = ref(false)
const reportsError = ref<string | null>(null)
const reports = ref<ReportListItem[]>([])
const selectedReport = ref<SyncReport | null>(null)
const selectedReportItem = ref<ReportListItem | null>(null)
const selectedReportPath = ref<string | null>(null)
const reportRangeStart = ref('')
const reportRangeEnd = ref('')
const reportEntryFilter = ref<'all' | SyncReportStatus>('all')
const reportEntrySearch = ref('')

let reportsGen = 0
let detailGen = 0

const {
  knownRuns,
  connect,
  disconnect,
  startRun,
  settleRunFromResponse,
  failRun,
  isRemoteBusy,
  lastUnknownRunFor,
  runsForScope,
  externalRunsForScope,
  reconcileFromList,
} = useSyncRuns({
  onRunProgress: (run) => updateLiveSelectedFromRun(run),
  onRunFinalized: (run) => {
    void handleRunFinalized(run)
  },
  onExternalFinalized: () => {
    if (alive) void loadReports()
  },
  onReconnect: () => {
    if (alive) void loadReports()
  },
})

connect()

const runsInScope = computed(() => runsForScope(scope.value))

function isRemoteNameBusy(name: string): boolean {
  return isRemoteBusy(scope.value, name) || isPreflightBusy(scope.value, name)
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
  return 'Unknown'
}

function statusClass(status: SyncRun['status']): string {
  if (status === 'success') return 'pill--success'
  if (status === 'error') return 'pill--danger'
  if (status === 'unknown') return 'pill--warn'
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
  if (normalized === 'unknown') return 'pill--warn'
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
  for (const run of runsInScope.value) {
    if (!map[run.remote]) {
      map[run.remote] = run
    }
  }
  return map
})

const reportsDir = computed(() => scopedInspect.value?.effective?.sync_reports_dir || '@reports')
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
  if (run.status === 'unknown') return 'unknown'
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
    project: report?.project ?? run.context.expectedExecProject ?? null,
    dry_run: report?.dry_run ?? run.dryRun ?? run.action === 'check',
    summary,
    warnings: report?.warnings || run.warnings || [],
    info: report?.info || run.info || [],
    entries_total: report?.entries_total ?? run.reportEntries?.length ?? 0,
    stored_path: report?.stored_path || null,
    runId: run.id,
    entries: run.reportEntries,
    source: 'run',
    queryScope: run.context.reportQueryScope,
  }
}

function buildReportItemFromExternal(ext: ExternalSyncRun): ReportListItem {
  const report = ext.report
  const status =
    ext.status === 'running'
      ? 'running'
      : ext.status === 'error'
        ? 'failed'
        : ext.status === 'unknown'
          ? 'unknown'
          : report?.status || 'success'
  return {
    id: report?.id || ext.id,
    created_at: report?.created_at || ext.startedAt,
    status,
    direction: report?.direction || (ext.action === 'check' ? 'pull' : ext.action),
    provider: report?.provider || 'jira',
    remote: report?.remote || ext.remote,
    project: report?.project ?? ext.execProject ?? null,
    dry_run: report?.dry_run ?? false,
    summary: report?.summary || emptySummary,
    warnings: report?.warnings || [],
    info: report?.info || [],
    entries_total: report?.entries_total ?? ext.reportEntries?.length ?? 0,
    stored_path: report?.stored_path || null,
    runId: ext.id,
    entries: ext.reportEntries,
    source: 'external',
    queryScope: ext.execProject || '',
  }
}

const reportListItems = computed<ReportListItem[]>(() => {
  const map = new Map<string, ReportListItem>()
  reports.value.forEach((report) => {
    map.set(report.id, report)
  })
  runsInScope.value.forEach((run) => {
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
  externalRunsForScope(scope.value).forEach((ext) => {
    const item = buildReportItemFromExternal(ext)
    if (map.has(item.id)) return
    map.set(item.id, item)
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

/**
 * The remote definition behind "Fields synced" must come from a map that is
 * known to describe the report's scope. A global-aggregator item for another
 * project, or an external run of unclear origin, must not infer fields from
 * the current scope's homonym definition.
 */
function fieldsAttributionKnown(item: ReportListItem | null): boolean {
  if (!item) return false
  if (item.source === 'run') {
    // Run items are built from runs of the current scope only.
    return true
  }
  if (item.source === 'listed') {
    // A global-listed report carrying a project may be defined by that
    // project's override, which the global map cannot see.
    return !(scope.value === '' && !!item.project)
  }
  // External: attribute only when the task project is the current scope, or
  // when it is project-less in the global view.
  if (scope.value === '') return !item.project
  return item.project === scope.value
}

const selectedReportFields = computed(() => {
  const report = selectedReport.value
  if (!report) return []
  if (!fieldsAttributionKnown(selectedReportItem.value)) return []
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
  const configured = scopedInspect.value?.effective?.sync_write_reports
  if (typeof configured === 'boolean') {
    writeReport.value = configured
  }
}

async function loadReports() {
  const gen = ++reportsGen
  const scopeNow = scope.value
  reportsLoading.value = true
  try {
    const payload = await api.syncReportsList({ project: scopeNow || undefined })
    if (!alive || gen !== reportsGen) return
    reports.value = payload.reports.map((report) => ({
      ...report,
      source: 'listed' as const,
      queryScope: scopeNow,
    }))
    reportsError.value = null
    reconcileFromList(scopeNow, payload.reports)
  } catch (err: any) {
    if (!alive || gen !== reportsGen) return
    // A failed same-scope refresh keeps the previous valid list (no false
    // empty state); stale cross-scope responses are dropped by the guard above.
    reportsError.value = err?.message || 'Failed to load reports'
  } finally {
    if (gen === reportsGen) {
      reportsLoading.value = false
    }
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

function dedupeScopes(candidates: Array<string | null | undefined>): string[] {
  const seen = new Set<string>()
  const out: string[] = []
  for (const candidate of candidates) {
    if (candidate === null || candidate === undefined) continue
    if (seen.has(candidate)) continue
    seen.add(candidate)
    out.push(candidate)
  }
  return out
}

function openReportItem(item: ReportListItem) {
  if (item.stored_path) {
    void openReportByItem(item)
    return
  }

  const run = item.runId ? knownRuns.value.find((entry) => entry.id === item.runId) : null
  const entries = run?.reportEntries || item.entries || []
  selectedReportItem.value = item
  selectedReport.value = buildReportFromItem(item, entries)
  selectedReportPath.value = null
  reportsError.value = null
}

/**
 * Resolve a stored report through the scope roots that may hold it: the query
 * scope the item was produced under first, then the report's task project and
 * the global root. A project param resolves a project reports dir override;
 * the global aggregator always keeps its own query root.
 */
async function openReportByItem(item: ReportListItem) {
  const storedPath = item.stored_path
  if (!storedPath) {
    reportsError.value = 'Report file not available for this run.'
    return
  }
  const gen = ++detailGen
  reportsError.value = null
  const candidates = dedupeScopes([item.queryScope, item.project, ''])
  let lastError: any = null
  for (const candidate of candidates) {
    if (!alive || gen !== detailGen) return
    try {
      const report = await api.syncReportGet(storedPath, candidate || undefined)
      if (!alive || gen !== detailGen) return
      selectedReportItem.value = item
      selectedReport.value = { ...report, entries: report.entries ?? [] }
      selectedReportPath.value = storedPath
      return
    } catch (err) {
      lastError = err
    }
  }
  if (!alive || gen !== detailGen) return
  // Keep the previous valid selection; surface the load failure instead.
  reportsError.value = lastError?.message || 'Failed to load report'
}

function recordLiveEvent(run: SyncRun) {
  const entry = run.reportEntries?.[0]
  if (!entry) return
  liveEvents.value = [
    {
      ...entry,
      runId: run.id,
      remote: run.remote,
      action: run.action,
    },
    ...liveEvents.value,
  ].slice(0, 50)
}

function updateLiveSelectedFromRun(run: SyncRun) {
  recordLiveEvent(run)
  const current = selectedReport.value
  if (!current || current.id !== run.id || selectedReportPath.value) return
  if (scope.value !== run.context.originScope) return
  selectedReport.value = {
    ...current,
    status: run.status === 'running' ? 'running' : current.status,
    dry_run: run.dryRun ?? current.dry_run,
    summary: run.summary || current.summary,
    entries: run.reportEntries || current.entries,
  }
}

function shouldAutoSelectRunReport(run: SyncRun): boolean {
  if (scope.value !== run.context.originScope) return false
  const current = selectedReport.value
  return !current || current.id === run.id
}

async function handleRunFinalized(run: SyncRun) {
  if (!alive) return
  await loadReports()
  if (!alive) return
  if (!shouldAutoSelectRunReport(run)) return
  const meta = run.report
  if (meta?.stored_path) {
    await openReportByItem(buildReportItemFromRun(run))
    return
  }
  const current = selectedReport.value
  if (current?.id === run.id) {
    selectedReportItem.value = buildReportItemFromRun(run)
    selectedReport.value = {
      ...current,
      status: run.status === 'error' ? 'failed' : reportStatusFromRun(run),
      summary: run.summary || current.summary,
      warnings:
        run.status === 'error'
          ? [...(current.warnings || []), run.error || 'Sync failed']
          : run.report?.warnings || current.warnings,
      info: run.report?.info || current.info,
      dry_run: run.dryRun ?? current.dry_run,
      entries: run.reportEntries || current.entries,
    }
  } else if (!current && (run.reportEntries?.length || run.report)) {
    const item = buildReportItemFromRun(run)
    selectedReportItem.value = item
    selectedReport.value = buildReportFromItem(item, run.reportEntries || [])
  }
}

/**
 * Mirror the backend's project resolution (sync_service resolve_project_prefix
 * and resolve_pull_project_prefix) so the origin scope, expected execution
 * project, and report root are captured together at request time.
 */
function computeRunContext(action: SyncAction, originScope: SyncScope, remote: SyncRemoteConfig): SyncRunOriginContext {
  if (originScope) {
    return { originScope, reportQueryScope: originScope, expectedExecProject: originScope }
  }
  const defaultProject = String(scopedInspect.value?.global_effective?.default_project ?? '').trim()
  if (defaultProject) {
    return { originScope, reportQueryScope: defaultProject, expectedExecProject: defaultProject }
  }
  if (action !== 'push' && remote.provider === 'jira') {
    const jiraProject = String(remote.project ?? '').trim()
    if (jiraProject) {
      return { originScope, reportQueryScope: '', expectedExecProject: jiraProject }
    }
  }
  return { originScope, reportQueryScope: '', expectedExecProject: null }
}

const preflightBusyKeys = ref<Set<string>>(new Set())

function preflightKey(scopeKey: SyncScope, remote: string): string {
  return `${scopeKey}::${remote}`
}

function isPreflightBusy(scopeKey: SyncScope, remote: string): boolean {
  return preflightBusyKeys.value.has(preflightKey(scopeKey, remote))
}

function setPreflightBusy(scopeKey: SyncScope, remote: string) {
  preflightBusyKeys.value = new Set(preflightBusyKeys.value).add(preflightKey(scopeKey, remote))
}

function clearPreflightBusy(key: string) {
  const next = new Set(preflightBusyKeys.value)
  next.delete(key)
  preflightBusyKeys.value = next
}

/**
 * A run started from the Global scope without an explicit project resolves
 * remotes through the default project's EFFECTIVE config on the server, so a
 * project override with the same name would silently replace the definition
 * the user sees. Preflight inspects the default project and blocks the run on
 * divergence with an actionable message instead of executing the wrong
 * remote. Failures fail closed; navigation away aborts.
 */
async function verifyDefaultProjectRemote(action: SyncAction, remoteName: string, displayedRemote: SyncRemoteConfig, originScope: SyncScope): Promise<boolean> {
  const defaultProject = String(scopedInspect.value?.global_effective?.default_project ?? '').trim()
  if (!defaultProject) return true

  const actionLabelNow = action === 'check' ? 'CHECK' : action.toUpperCase()
  const busyKey = preflightKey(originScope, remoteName)
  setPreflightBusy(originScope, remoteName)
  const gen = inspectGen
  try {
    const fresh = await api.inspectConfig(defaultProject)
    if (!alive) return false
    if (gen !== inspectGen || scope.value !== originScope || inspectStamp.value !== originScope) {
      // Scope navigated or its data lost validity while verifying: abort.
      return false
    }
    const overrideRemote = fresh.effective?.remotes?.[remoteName]
    if (overrideRemote) {
      const displayedYaml = stringifyYaml(remoteDefFromForm(displayedRemote)).trim()
      const overrideYaml = stringifyYaml(remoteDefFromForm(overrideRemote)).trim()
      if (overrideYaml !== displayedYaml) {
        showToast(
          `Remote '${remoteName}' is overridden in default project ${defaultProject}, so a Global run would execute that override. Switch to ${defaultProject} to run it, or align the definitions.`,
        )
        return false
      }
    }
    showToast(`${actionLabelNow} runs through default project ${defaultProject}; its effective config applies.`)
    return true
  } catch (err: any) {
    if (alive && scope.value === originScope) {
      showToast(
        `Cannot verify remote '${remoteName}' against default project ${defaultProject}: ${err?.message || 'inspection failed'}. ${actionLabelNow} was blocked.`,
      )
    }
    return false
  } finally {
    clearPreflightBusy(busyKey)
  }
}

async function runSync(action: SyncAction, entry: { name: string; remote: SyncRemoteConfig }) {
  if (!entry?.name) return
  const remoteName = entry.name
  const originScope = scope.value
  if (inspectStamp.value !== originScope) return
  if (isRemoteBusy(originScope, remoteName) || isPreflightBusy(originScope, remoteName)) return

  const unknownRun = lastUnknownRunFor(originScope, remoteName)
  if (unknownRun && action !== 'check') {
    showToast(
      `Previous ${unknownRun.actionLabel} for ${remoteName} has unknown status; re-running may duplicate in-flight work`,
    )
  }

  if (!originScope) {
    const defaultProject = String(scopedInspect.value?.global_effective?.default_project ?? '').trim()
    if (defaultProject) {
      const allowed = await verifyDefaultProjectRemote(action, remoteName, entry.remote, originScope)
      if (!allowed) return
    } else if (action !== 'push') {
      if (entry.remote.provider === 'jira') {
        const jiraProject = String(entry.remote.project ?? '').trim()
        if (!jiraProject) {
          showToast('Pull requires a project scope or default_project.')
          return
        }
        showToast(`Pull without project will use Jira project ${jiraProject} as the local prefix.`)
      } else {
        showToast('Pull for GitHub requires a project scope or default_project.')
        return
      }
    }
  }

  const context = computeRunContext(action, originScope, entry.remote)
  const run = startRun({ action, remote: remoteName, context })

  const payload: { remote: string; project?: string; dry_run?: boolean; include_report?: boolean; write_report?: boolean; client_run_id?: string } = {
    remote: remoteName,
    project: originScope || undefined,
  }
  if (action === 'check') {
    payload.dry_run = true
  }
  payload.include_report = true
  payload.write_report = writeReport.value
  payload.client_run_id = run.id

  try {
    const result = action === 'push'
      ? await api.syncPush(payload)
      : await api.syncPull(payload)
    if (!alive) return
    const outcome = settleRunFromResponse(run.id, result)
    if (outcome.finalized === 'now' && shouldAutoSelectRunReport(run)) {
      if (result.report && result.report_entries?.length) {
        selectedReportItem.value = {
          ...result.report,
          source: 'run',
          queryScope: context.reportQueryScope,
          runId: run.id,
        }
        selectedReport.value = {
          ...result.report,
          entries: result.report_entries,
        }
        selectedReportPath.value = result.report.stored_path || null
      } else if (result.report?.stored_path) {
        await openReportByItem({
          ...result.report,
          source: 'run',
          queryScope: context.reportQueryScope,
          runId: run.id,
        })
      }
    }
    await loadReports()

    showToast(
      `${run.actionLabel} ${result.remote}: ${result.summary.created} created, ${result.summary.updated} updated, ${result.summary.skipped} skipped, ${result.summary.failed} failed`,
    )
    if (result.warnings?.length) {
      result.warnings.forEach((warning) => showToast(warning))
    }
    if (result.info?.length) {
      result.info.forEach((note) => showToast(note))
    }
  } catch (err: any) {
    if (!alive) return
    const message = err?.message || `Failed to ${action} ${remoteName}`
    failRun(run.id, `Request failed: ${message}`, true)
    showToast(`Request failed: ${message}`)
  }
}

watch(
  () => scopedInspect.value?.effective?.sync_write_reports,
  () => ensureWriteReportDefault(),
  { immediate: true },
)

watch(
  project,
  () => {
    // Invalidate in-flight report work from the previous scope before loading
    // the new one; selections never survive a scope switch.
    reportsGen += 1
    detailGen += 1
    reports.value = []
    selectedReport.value = null
    selectedReportItem.value = null
    selectedReportPath.value = null
    reportsError.value = null
    void loadScopeConfig(scope.value)
    void loadReports()
  },
  { immediate: true },
)

onUnmounted(() => {
  alive = false
  disconnect()
})

function formatTimestamp(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

async function handleReload() {
  await loadScopeConfig()
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

.sync-remote-dialog__target {
  margin: 0;
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid var(--color-border);
  background: color-mix(in oklab, var(--color-surface-contrast) 70%, transparent);
  font-size: 0.85rem;
}

.sync-remote-dialog__choice {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  border: 1px dashed var(--color-border);
  border-radius: 10px;
}

.sync-remote-dialog__choice p {
  margin: 0;
}

.sync-remote-dialog__choice-buttons {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.sync-remote-dialog__fieldset {
  border: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.sync-remote-dialog__fieldset:disabled {
  opacity: 0.7;
}

.sync-remote-dialog__delete {
  color: var(--color-danger);
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

.remote-origin-chip {
  font-size: 0.65rem;
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

.pill--warn {
  color: var(--color-danger);
  background: color-mix(in oklab, var(--color-danger) 12%, transparent);
  border-color: color-mix(in oklab, var(--color-danger) 30%, var(--color-border));
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
      "reports";
  }
}

@media (max-width: 820px) {
  .reports-grid {
    grid-template-columns: 1fr;
  }
}
</style>
