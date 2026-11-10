<template>
  <section class="scan-page">
    <PageHeader><template #title>Scan</template></PageHeader>

    <div class="scan-layout">
      <aside class="scan-sidebar">
        <UiCard class="scan-card">
          <div class="scan-card__header">
            <h3>Scan inputs</h3>
          </div>
          <div class="scan-sidebar-fields">
            <label class="scan-field">
              <span class="scan-field__label muted">Paths</span>
              <UiInput v-model="pathsInput" placeholder="src, packages/api" />
            </label>
            <label class="scan-field">
              <span class="scan-field__label muted">Include extensions</span>
              <UiInput v-model="includeInput" placeholder="rs, ts, tsx" />
            </label>
            <label class="scan-field">
              <span class="scan-field__label muted">Exclude extensions</span>
              <UiInput v-model="excludeInput" placeholder="lock, png" />
            </label>
            <label class="scan-field">
              <span class="scan-field__label muted">
                Strip attributes
                <button
                  type="button"
                  class="scan-info"
                  :title="helpStrip"
                  :aria-label="helpStrip"
                  @click="showHelp('strip')"
                >
                  ?
                </button>
              </span>
              <UiSelect v-model="stripAttributes">
                <option value="inherit">Use config</option>
                <option value="true">Yes</option>
                <option value="false">No</option>
              </UiSelect>
            </label>
            <div class="scan-field scan-field--checkbox">
              <input id="scan-modified-only" v-model="modifiedOnly" type="checkbox" />
              <label for="scan-modified-only">Git changes only</label>
              <button
                type="button"
                class="scan-info"
                :title="helpModified"
                :aria-label="helpModified"
                @click="showHelp('modified')"
              >
                ?
              </button>
            </div>
            <div class="scan-field scan-field--checkbox">
              <input id="scan-reanchor" v-model="reanchor" type="checkbox" />
              <label for="scan-reanchor">Remove dead anchors</label>
              <button
                type="button"
                class="scan-info"
                :title="helpReanchor"
                :aria-label="helpReanchor"
                @click="showHelp('reanchor')"
              >
                ?
              </button>
            </div>
          </div>
          <div class="scan-card__footer">
            <UiButton variant="primary" type="button" @click="runScan(true)" :disabled="loading">
              <IconGlyph name="search" />
              Dry run
            </UiButton>
            <UiButton type="button" @click="confirmRun" :disabled="loading"> Run </UiButton>
          </div>
        </UiCard>

        <UiCard class="scan-card">
          <div class="scan-card__header">
            <h3>Scanner settings</h3>
            <div class="scan-card__header-actions">
              <ReloadButton
                :disabled="configLoading"
                :loading="configLoading"
                label="Reload config"
                title="Reload config"
                @click="loadConfig"
              />
              <UiButton
                type="button"
                variant="primary"
                :class="{ 'is-hidden': !settingsChanged }"
                :disabled="configLoading || !settingsChanged"
                :aria-hidden="!settingsChanged"
                :tabindex="settingsChanged ? 0 : -1"
                @click="saveAllSettings"
              >
                Save
              </UiButton>
            </div>
          </div>
          <p v-if="settingsError" class="scan-error" role="alert" data-testid="scan-settings-error">
            {{ settingsError }}
          </p>
          <div v-if="settingsWarnings.length" class="scan-warnings" data-testid="scan-settings-warnings">
            <ul>
              <li v-for="warning in settingsWarnings" :key="warning">{{ warning }}</li>
            </ul>
          </div>
          <p v-if="configError" class="scan-error">{{ configError }}</p>
          <div v-else-if="configLoading" class="scan-loading">
            <UiLoader size="sm">Loading…</UiLoader>
          </div>
          <div v-else class="scan-sidebar-fields">
            <label class="scan-field">
              <span class="scan-field__label muted">Project</span>
              <UiSelect v-model="project">
                <option value="">Global</option>
                <option v-for="entry in projects" :key="entry.prefix" :value="entry.prefix">
                  {{ formatProjectLabel(entry) }}
                </option>
              </UiSelect>
            </label>
            <label class="scan-field">
              <span class="scan-field__label muted">Signal words</span>
              <UiInput v-model="editSignalWords" placeholder="TODO, FIXME, HACK" />
            </label>
            <label class="scan-field">
              <span class="scan-field__label muted">Ticket patterns (one per line)</span>
              <textarea
                v-model="editTicketPatterns"
                class="input scan-field__patterns"
                rows="2"
                spellcheck="false"
                data-testid="scan-ticket-patterns"
                placeholder="[A-Z]+-\\d+"
              ></textarea>
            </label>
            <div class="scan-field scan-field--checkbox">
              <input id="scan-mentions" v-model="editEnableMentions" type="checkbox" />
              <label for="scan-mentions">Mentions detection</label>
            </div>
            <div class="scan-field scan-field--checkbox">
              <input id="scan-strip-config" v-model="editStripConfig" type="checkbox" />
              <label for="scan-strip-config">Strip attributes (config default)</label>
            </div>
          </div>
        </UiCard>
      </aside>

      <main class="scan-main">
        <UiCard class="scan-card scan-card--results">
          <div class="scan-card__header">
            <h3>Scan results</h3>
            <div class="scan-card__actions">
              <div v-if="scanResponse" class="scan-summary scan-summary--inline">
                <span class="scan-pill scan-pill--created">{{ scanSummary.created }} create</span>
                <span class="scan-pill scan-pill--updated">{{ scanSummary.updated }} update</span>
                <span class="scan-pill scan-pill--skipped">{{ scanSummary.skipped }} skip</span>
                <span class="scan-pill scan-pill--failed" v-if="scanSummary.failed > 0">
                  {{ scanSummary.failed }} fail
                </span>
              </div>
              <UiButton
                v-if="scanResponse"
                icon-only
                variant="ghost"
                type="button"
                title="Clear results"
                aria-label="Clear results"
                @click="clearResults"
              >
                <IconGlyph name="close" />
              </UiButton>
            </div>
          </div>

          <p v-if="error" class="scan-error">{{ error }}</p>
          <div v-if="loading" class="scan-loading">
            <UiLoader>Running scan…</UiLoader>
          </div>
          <p v-else-if="!scanResponse" class="muted">Run a dry scan to preview the findings.</p>
          <template v-else>
            <div v-if="scanResponse.warnings.length" class="scan-warnings">
              <p class="muted">Warnings:</p>
              <ul>
                <li v-for="warning in scanResponse.warnings" :key="warning">{{ warning }}</li>
              </ul>
            </div>
            <div v-if="scanResponse.info?.length" class="scan-warnings scan-info-list" data-testid="scan-info">
              <ul>
                <li v-for="message in scanResponse.info" :key="message">{{ message }}</li>
              </ul>
            </div>

            <p v-if="!scanEntries.length" class="muted">No matches found for this scan.</p>
            <div v-else class="scan-results">
              <div v-for="entry in scanEntries" :key="entryKey(entry)" class="scan-entry">
                <div class="scan-entry__header">
                  <div class="scan-entry__meta">
                    <span :class="['scan-pill', statusClass(entry.status)]">{{ formatStatus(entry.status) }}</span>
                    <span v-if="isApplied(entry)" class="scan-pill scan-pill--applied" data-testid="scan-entry-applied">
                      Applied
                    </span>
                    <span class="scan-pill scan-pill--action">{{ formatAction(entry.action) }}</span>
                    <span class="scan-entry__file">{{ entry.file }}:{{ entry.line }}</span>
                    <span v-if="isDryRunMessage(entry)" class="scan-entry__warning">{{ entry.message }}</span>
                  </div>
                  <div class="scan-entry__actions">
                    <UiButton
                      v-if="scanResponse.dry_run && isSelectable(entry) && !isApplied(entry)"
                      type="button"
                      :disabled="loading"
                      @click="applyEntry(entry)"
                    >
                      <IconGlyph name="check" />
                      Apply
                    </UiButton>
                    <UiButton
                      icon-only
                      type="button"
                      :title="isSnippetOpen(entry) ? 'Hide preview' : 'Preview code'"
                      :aria-label="isSnippetOpen(entry) ? 'Hide preview' : 'Preview code'"
                      @click="toggleSnippet(entry)"
                    >
                      <IconGlyph :name="isSnippetOpen(entry) ? 'eye-off' : 'eye'" />
                    </UiButton>
                  </div>
                </div>

                <div class="scan-entry__body">
                  <div class="scan-entry__title">{{ entry.title || entry.annotation }}</div>
                  <div class="scan-entry__annotation muted">{{ entry.annotation }}</div>
                  <div v-if="entry.existing_key" class="scan-entry__hint muted">
                    Existing key: {{ entry.existing_key }}
                  </div>
                  <div v-if="entry.task_id" class="scan-entry__hint muted">Created: {{ entry.task_id }}</div>
                  <div v-if="entry.updated_line && entry.updated_line !== entry.original_line" class="scan-entry__diff">
                    <div class="scan-entry__diff-line">- {{ entry.original_line }}</div>
                    <div class="scan-entry__diff-line scan-entry__diff-line--add">+ {{ entry.updated_line }}</div>
                  </div>
                  <div v-if="entry.message && !isDryRunMessage(entry)" class="scan-entry__message">
                    {{ entry.message }}
                  </div>
                </div>

                <div v-if="isSnippetOpen(entry)" class="scan-entry__snippet">
                  <p v-if="snippetState(entry).loading" class="muted">Loading preview…</p>
                  <p v-else-if="snippetState(entry).error" class="scan-error">{{ snippetState(entry).error }}</p>
                  <div v-else-if="snippetState(entry).snippet" class="scan-snippet">
                    <div class="scan-snippet__header">
                      <strong>{{ snippetState(entry).snippet?.path }}</strong>
                      <span class="muted"
                        >Lines {{ snippetState(entry).snippet?.start_line }}–{{
                          snippetState(entry).snippet?.end_line
                        }}</span
                      >
                    </div>
                    <div class="scan-snippet__lines">
                      <div
                        v-for="line in snippetState(entry).snippet?.lines"
                        :key="line.number"
                        :class="['scan-snippet__line', isHighlighted(line.number, snippetState(entry).snippet)]"
                      >
                        <span class="scan-snippet__number">{{ line.number }}</span>
                        <span class="scan-snippet__text">{{ line.text }}</span>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </template>
        </UiCard>
      </main>
    </div>

    <!-- Run confirmation dialog -->
    <UiModal
      :open="showRunConfirmDialog"
      size="sm"
      aria-label="Run scan?"
      :aria-labelledby="runConfirmTitleId"
      :aria-describedby="runConfirmDescId"
      :dismissible="!loading"
      @close="cancelRunConfirm"
    >
      <div class="scan-dialog">
        <h3 :id="runConfirmTitleId">Run scan?</h3>
        <p :id="runConfirmDescId">This will create or update tasks in your project.</p>
        <p class="muted">Tip: Use "Dry run" first to preview changes safely.</p>
        <label class="scan-dialog__checkbox">
          <input v-model="skipFutureConfirm" type="checkbox" />
          Don't show this again
        </label>
        <div class="scan-dialog__actions">
          <UiButton type="button" data-autofocus @click="cancelRunConfirm">Cancel</UiButton>
          <UiButton variant="primary" type="button" @click="handleRunConfirm(skipFutureConfirm)">Run</UiButton>
        </div>
      </div>
    </UiModal>
  </section>
</template>

<script setup lang="ts">
import { errorMessage } from '../utils/errors'
import { formatProjectLabel } from '../utils/projectLabels'
import PageHeader from '../components/PageHeader.vue'
import { computed, onMounted, reactive, ref, useId, watch } from 'vue'
import { api } from '../api/client'
import type {
  ConfigInspectResult,
  ReferenceSnippet,
  ScanEntry,
  ScanRequest,
  ScanResponse,
  ScanTarget,
} from '../api/types'
import IconGlyph from '../components/IconGlyph.vue'
import ReloadButton from '../components/ReloadButton.vue'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiInput from '../components/UiInput.vue'
import UiLoader from '../components/UiLoader.vue'
import UiModal from '../components/UiModal.vue'
import UiSelect from '../components/UiSelect.vue'
import { showToast } from '../components/toast'
import { useProjects } from '../composables/useProjects'
import { storageGetFlag, storageSetFlag } from '../utils/storage'

const runConfirmTitleId = useId()
const runConfirmDescId = useId()

const { projects, refresh: refreshProjects } = useProjects()
const project = ref('')
const pathsInput = ref('')
const includeInput = ref('')
const excludeInput = ref('')
const stripAttributes = ref<'inherit' | 'true' | 'false'>('inherit')
const modifiedOnly = ref(false)
const reanchor = ref(false)

const helpStrip =
  "When inserting a new ticket reference into your code, remove any inline attribute blocks like [priority=high] or [assignee=me]. These attributes are parsed and applied to the task, so they don't need to stay in the comment."
const helpModified =
  'Only scan files that have uncommitted git changes. Useful for large repos where you only want to process newly touched files rather than the entire codebase.'
const helpReanchor =
  'Also clean up code anchors within the scanned paths: anchors that drifted move to the nearest mention, and anchors whose file is gone or no longer mentions the task ID are removed. Live mentions in any file, links, files and attachments are kept. A preview lists the changes without writing them.'

function showHelp(key: 'strip' | 'modified' | 'reanchor') {
  const messages: Record<typeof key, string> = {
    strip: helpStrip,
    modified: helpModified,
    reanchor: helpReanchor,
  }
  window.alert(messages[key])
}

const loading = ref(false)
const error = ref<string | null>(null)
const scanResponse = ref<ScanResponse | null>(null)

const configLoading = ref(false)
const configError = ref<string | null>(null)
const configState = ref<ConfigInspectResult | null>(null)
const settingsError = ref<string | null>(null)
const settingsWarnings = ref<string[]>([])

// Editable scanner settings
const editSignalWords = ref('')
const editTicketPatterns = ref('')
const editEnableMentions = ref(false)
const editStripConfig = ref(false)

// LocalStorage key for skipping run confirmation
const SKIP_RUN_CONFIRM_KEY = 'scan-skip-run-confirm'
const showRunConfirmDialog = ref(false)
const skipFutureConfirm = ref(false)

const snippetMap = reactive<
  Record<string, { open: boolean; loading: boolean; error: string | null; snippet: ReferenceSnippet | null }>
>({})

const scanSummary = computed(() => {
  return scanResponse.value?.summary ?? { created: 0, updated: 0, skipped: 0, failed: 0 }
})

function sameList(a: readonly string[], b: readonly string[]) {
  return a.length === b.length && a.every((value, index) => value === b[index])
}

/**
 * DEV-264: only the scanner settings the user changed, against what the
 * selected scope resolves to. Untouched (often inherited) values are never
 * resent, so saving one toggle cannot pin copies of the others into a file.
 */
const changedSettings = computed<Record<string, string>>(() => {
  const effective = configState.value?.effective
  if (!effective) return {}
  const values: Record<string, string> = {}
  const signalWords = parseList(editSignalWords.value)
  if (!sameList(signalWords, effective.scan_signal_words ?? [])) {
    values.scan_signal_words = JSON.stringify(signalWords)
  }
  const ticketPatterns = parseLines(editTicketPatterns.value)
  if (!sameList(ticketPatterns, effective.scan_ticket_patterns ?? [])) {
    values.scan_ticket_patterns = JSON.stringify(ticketPatterns)
  }
  if (editEnableMentions.value !== (effective.scan_enable_mentions ?? false)) {
    values.scan_enable_mentions = JSON.stringify(editEnableMentions.value)
  }
  if (editStripConfig.value !== (effective.scan_strip_attributes ?? false)) {
    values.scan_strip_attributes = JSON.stringify(editStripConfig.value)
  }
  return values
})

const settingsChanged = computed(() => Object.keys(changedSettings.value).length > 0)

const scanEntries = computed(() =>
  Array.isArray(scanResponse.value?.entries) ? (scanResponse.value?.entries ?? []) : [],
)

function formatStatus(status: string) {
  switch (status) {
    case 'created':
      return 'Create'
    case 'updated':
      return 'Update'
    case 'failed':
      return 'Failed'
    case 'skipped':
      return 'Skipped'
    default:
      return status
  }
}

function formatAction(action: string) {
  switch (action) {
    case 'create':
      return 'Create task'
    case 'refresh':
      return 'Refresh reference'
    case 'skip':
      return 'No action'
    default:
      return action
  }
}

function statusClass(status: string) {
  return `scan-pill--${status}`
}

function entryKey(entry: ScanEntry) {
  return `${entry.file}::${entry.line}`
}

function isSelectable(entry: ScanEntry) {
  return entry.action !== 'skip' && entry.status !== 'failed'
}

function isDryRunMessage(entry: ScanEntry) {
  return !!(scanResponse.value?.dry_run && entry.message && entry.message.toLowerCase().includes('dry run'))
}

function parseList(raw: string) {
  return raw
    .split(/,|\n/)
    .map((value) => value.trim())
    .filter(Boolean)
}

/** Regexes may contain commas (`{2,5}`, `[,;]`): one per line, never split on commas. */
function parseLines(raw: string) {
  return raw
    .split(/\r?\n/)
    .map((value) => value.trim())
    .filter(Boolean)
}

function buildRequest(dryRun: boolean, targets?: ScanTarget[]): ScanRequest {
  const stripValue = stripAttributes.value === 'inherit' ? undefined : stripAttributes.value === 'true'
  return {
    paths: parseList(pathsInput.value),
    include: parseList(includeInput.value),
    exclude: parseList(excludeInput.value),
    project: project.value || undefined,
    dry_run: dryRun,
    strip_attributes: stripValue,
    reanchor: reanchor.value,
    modified_only: modifiedOnly.value,
    targets: targets ?? [],
  }
}

function normalizeResponse(response: ScanResponse): ScanResponse {
  return {
    ...response,
    summary: response?.summary ?? { created: 0, updated: 0, skipped: 0, failed: 0 },
    entries: Array.isArray(response?.entries) ? response.entries : [],
    warnings: Array.isArray(response?.warnings) ? response.warnings : [],
    info: Array.isArray(response?.info) ? response.info : [],
  }
}

/** Entries of the current dry run already applied one by one (DEV-264). */
const appliedKeys = reactive(new Set<string>())

function isApplied(entry: ScanEntry) {
  return appliedKeys.has(entryKey(entry))
}

async function runScan(dryRun: boolean, targets?: ScanTarget[]) {
  error.value = null
  loading.value = true
  try {
    const payload = buildRequest(dryRun, targets)
    const response = await api.scanRun(payload)
    scanResponse.value = normalizeResponse(response)
    appliedKeys.clear()
  } catch (err: unknown) {
    error.value = errorMessage(err) || 'Failed to run scan'
  } finally {
    loading.value = false
  }
}

/**
 * Apply one finding of the dry run, then show its result in place: the rest
 * of the preview, and its Apply buttons, stay as they were (DEV-264).
 */
async function applyEntry(entry: ScanEntry) {
  const preview = scanResponse.value
  if (!preview?.dry_run || !isSelectable(entry) || isApplied(entry)) return
  error.value = null
  loading.value = true
  try {
    const response = normalizeResponse(await api.scanRun(buildRequest(false, [{ file: entry.file, line: entry.line }])))
    if (scanResponse.value !== preview) return
    const key = entryKey(entry)
    const result =
      response.entries.find((candidate) => entryKey(candidate) === key) ??
      (response.entries.length === 1 ? response.entries[0] : undefined)
    const merged = { ...entry, ...result, file: entry.file, line: entry.line }
    scanResponse.value = {
      ...preview,
      entries: preview.entries.map((candidate) => (entryKey(candidate) === key ? merged : candidate)),
      warnings: Array.from(new Set([...preview.warnings, ...response.warnings])),
      info: Array.from(new Set([...(preview.info ?? []), ...(response.info ?? [])])),
    }
    if (merged.status !== 'failed') appliedKeys.add(key)
  } catch (err: unknown) {
    error.value = (err instanceof Error && err.message) || 'Failed to apply finding'
  } finally {
    loading.value = false
  }
}

function clearResults() {
  scanResponse.value = null
  appliedKeys.clear()
}

function snippetState(entry: ScanEntry) {
  const key = entryKey(entry)
  if (!snippetMap[key]) {
    snippetMap[key] = { open: false, loading: false, error: null, snippet: null }
  }
  return snippetMap[key]
}

function isSnippetOpen(entry: ScanEntry) {
  return snippetState(entry).open
}

async function toggleSnippet(entry: ScanEntry) {
  const state = snippetState(entry)
  state.open = !state.open
  if (!state.open) return
  if (state.snippet || state.loading) return
  state.loading = true
  state.error = null
  try {
    state.snippet = await api.referenceSnippet(entry.code_reference, { before: 4, after: 4 })
  } catch (err: unknown) {
    state.error = errorMessage(err) || 'Failed to load preview'
  } finally {
    state.loading = false
  }
}

function isHighlighted(lineNumber: number, snippet?: ReferenceSnippet | null) {
  if (!snippet) return ''
  if (lineNumber < snippet.highlight_start || lineNumber > snippet.highlight_end) return ''
  return 'is-highlighted'
}

function syncEditableFields() {
  const effective = configState.value?.effective
  editSignalWords.value = (effective?.scan_signal_words ?? []).join(', ')
  editTicketPatterns.value = (effective?.scan_ticket_patterns ?? []).join('\n')
  editEnableMentions.value = effective?.scan_enable_mentions ?? false
  editStripConfig.value = effective?.scan_strip_attributes ?? false
}

async function loadConfig() {
  configLoading.value = true
  configError.value = null
  try {
    configState.value = await api.inspectConfig(project.value || undefined)
    syncEditableFields()
  } catch (err: unknown) {
    configError.value = errorMessage(err) || 'Failed to load scan config'
  } finally {
    configLoading.value = false
  }
}

async function saveAllSettings() {
  const values = { ...changedSettings.value }
  if (!Object.keys(values).length) return
  const scope = project.value
  configLoading.value = true
  settingsError.value = null
  settingsWarnings.value = []
  try {
    // DEV-264: "Global" writes the global config, never the default project.
    const result = await api.setConfig({ values, project: scope || undefined, global: !scope })
    const warnings = Array.isArray(result?.warnings) ? result.warnings : []
    if (Array.isArray(result?.errors) && result.errors.length) throw new Error(result.errors.join('\n'))
    settingsWarnings.value = warnings
    showToast(warnings.length ? 'Scanner settings saved with warnings' : 'Scanner settings saved')
    await loadConfig()
  } catch (err: unknown) {
    settingsError.value = errorMessage(err) || 'Failed to save settings'
  } finally {
    configLoading.value = false
  }
}

function confirmRun() {
  const skipConfirm = storageGetFlag(SKIP_RUN_CONFIRM_KEY)
  if (skipConfirm) {
    runScan(false)
    return
  }
  skipFutureConfirm.value = false
  showRunConfirmDialog.value = true
}

function handleRunConfirm(skipFuture: boolean) {
  if (!showRunConfirmDialog.value) return
  showRunConfirmDialog.value = false
  if (skipFuture) {
    storageSetFlag(SKIP_RUN_CONFIRM_KEY, true)
  }
  runScan(false)
}

function cancelRunConfirm() {
  showRunConfirmDialog.value = false
  skipFutureConfirm.value = false
}

onMounted(async () => {
  await refreshProjects()
  await loadConfig()
})

watch(project, async () => {
  settingsError.value = null
  settingsWarnings.value = []
  await loadConfig()
})
</script>
