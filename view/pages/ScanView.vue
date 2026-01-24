<template>
  <section class="scan-page">
    <div class="scan-header">
      <div class="scan-header__titles">
        <h1>Scan</h1>
      </div>
    </div>

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
              <label for="scan-reanchor">Keep only newest reference</label>
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
            <UiButton type="button" @click="confirmRun" :disabled="loading">
              Run
            </UiButton>
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
              <UiInput
                v-model="editSignalWords"
                placeholder="TODO, FIXME, HACK"
              />
            </label>
            <label class="scan-field">
              <span class="scan-field__label muted">Ticket patterns</span>
              <UiInput
                v-model="editTicketPatterns"
                placeholder="[A-Z]+-\\d+"
              />
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

            <p v-if="!scanEntries.length" class="muted">No matches found for this scan.</p>
            <div v-else class="scan-results">
              <div v-for="entry in scanEntries" :key="entryKey(entry)" class="scan-entry">
                <div class="scan-entry__header">
                  <div class="scan-entry__meta">
                    <span :class="['scan-pill', statusClass(entry.status)]">{{ formatStatus(entry.status) }}</span>
                    <span class="scan-pill scan-pill--action">{{ formatAction(entry.action) }}</span>
                    <span class="scan-entry__file">{{ entry.file }}:{{ entry.line }}</span>
                    <span v-if="isDryRunMessage(entry)" class="scan-entry__warning">{{ entry.message }}</span>
                  </div>
                  <div class="scan-entry__actions">
                    <UiButton
                      v-if="scanResponse.dry_run && isSelectable(entry)"
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
                  <div v-if="entry.task_id" class="scan-entry__hint muted">
                    Created: {{ entry.task_id }}
                  </div>
                  <div
                    v-if="entry.updated_line && entry.updated_line !== entry.original_line"
                    class="scan-entry__diff"
                  >
                    <div class="scan-entry__diff-line">- {{ entry.original_line }}</div>
                    <div class="scan-entry__diff-line scan-entry__diff-line--add">+ {{ entry.updated_line }}</div>
                  </div>
                  <div v-if="entry.message && !isDryRunMessage(entry)" class="scan-entry__message">{{ entry.message }}</div>
                </div>

                <div v-if="isSnippetOpen(entry)" class="scan-entry__snippet">
                  <p v-if="snippetState(entry).loading" class="muted">Loading preview…</p>
                  <p v-else-if="snippetState(entry).error" class="scan-error">{{ snippetState(entry).error }}</p>
                  <div v-else-if="snippetState(entry).snippet" class="scan-snippet">
                    <div class="scan-snippet__header">
                      <strong>{{ snippetState(entry).snippet?.path }}</strong>
                      <span class="muted">Lines {{ snippetState(entry).snippet?.start_line }}–{{ snippetState(entry).snippet?.end_line }}</span>
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
    <div v-if="showRunConfirmDialog" class="scan-dialog-overlay" @click.self="cancelRunConfirm">
      <div class="scan-dialog">
        <h3>Run scan?</h3>
        <p>This will create or update tasks in your project.</p>
        <p class="muted">Tip: Use "Dry run" first to preview changes safely.</p>
        <label class="scan-dialog__checkbox">
          <input v-model="skipFutureConfirm" type="checkbox" />
          Don't show this again
        </label>
        <div class="scan-dialog__actions">
          <UiButton type="button" @click="cancelRunConfirm">Cancel</UiButton>
          <UiButton variant="primary" type="button" @click="handleRunConfirm(skipFutureConfirm)">Run</UiButton>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { api } from '../api/client'
import type { ConfigInspectResult, ProjectDTO, ReferenceSnippet, ScanEntry, ScanRequest, ScanResponse, ScanTarget } from '../api/types'
import IconGlyph from '../components/IconGlyph.vue'
import ReloadButton from '../components/ReloadButton.vue'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiInput from '../components/UiInput.vue'
import UiLoader from '../components/UiLoader.vue'
import UiSelect from '../components/UiSelect.vue'
import { useProjects } from '../composables/useProjects'

const { projects, refresh: refreshProjects } = useProjects()
const project = ref('')
const pathsInput = ref('')
const includeInput = ref('')
const excludeInput = ref('')
const stripAttributes = ref<'inherit' | 'true' | 'false'>('inherit')
const modifiedOnly = ref(false)
const reanchor = ref(false)

const helpStrip = 'When inserting a new ticket reference into your code, remove any inline attribute blocks like [priority=high] or [assignee=me]. These attributes are parsed and applied to the task, so they don\'t need to stay in the comment.'
const helpModified = 'Only scan files that have uncommitted git changes. Useful for large repos where you only want to process newly touched files rather than the entire codebase.'
const helpReanchor = 'By default, the scanner keeps existing code references from other files (useful when a task spans multiple files). Enable this to remove all other code references and keep only the currently scanned line. Use with caution: this can drop intentional multi-file links.'

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

// Editable scanner settings
const editSignalWords = ref('')
const editTicketPatterns = ref('')
const editEnableMentions = ref(false)
const editStripConfig = ref(false)

// LocalStorage key for skipping run confirmation
const SKIP_RUN_CONFIRM_KEY = 'scan-skip-run-confirm'
const showRunConfirmDialog = ref(false)
const skipFutureConfirm = ref(false)

const snippetMap = reactive<Record<string, { open: boolean; loading: boolean; error: string | null; snippet: ReferenceSnippet | null }>>({})

const scanSummary = computed(() => {
  return scanResponse.value?.summary ?? { created: 0, updated: 0, skipped: 0, failed: 0 }
})

// Check if scanner settings have changed from saved config
const settingsChanged = computed(() => {
  const effective = configState.value?.effective
  if (!effective) return false
  const savedSignalWords = (effective.scan_signal_words ?? []).join(', ')
  const savedTicketPatterns = (effective.scan_ticket_patterns ?? []).join(', ')
  const savedMentions = effective.scan_enable_mentions ?? false
  const savedStrip = effective.scan_strip_attributes ?? false
  return (
    editSignalWords.value !== savedSignalWords ||
    editTicketPatterns.value !== savedTicketPatterns ||
    editEnableMentions.value !== savedMentions ||
    editStripConfig.value !== savedStrip
  )
})

const scanEntries = computed(() => Array.isArray(scanResponse.value?.entries) ? scanResponse.value?.entries ?? [] : [])

function formatProjectLabel(entry: ProjectDTO) {
  return entry.name ? `${entry.name} (${entry.prefix})` : entry.prefix
}

function formatStatus(status: string) {
  switch (status) {
    case 'created': return 'Create'
    case 'updated': return 'Update'
    case 'failed': return 'Failed'
    case 'skipped': return 'Skipped'
    default: return status
  }
}

function formatAction(action: string) {
  switch (action) {
    case 'create': return 'Create task'
    case 'refresh': return 'Refresh reference'
    case 'skip': return 'No action'
    default: return action
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

function buildRequest(dryRun: boolean, targets?: ScanTarget[]): ScanRequest {
  const stripValue = stripAttributes.value === 'inherit'
    ? undefined
    : stripAttributes.value === 'true'
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

async function runScan(dryRun: boolean, targets?: ScanTarget[]) {
  error.value = null
  loading.value = true
  try {
    const payload = buildRequest(dryRun, targets)
    const response = await api.scanRun(payload)
    scanResponse.value = {
      ...response,
      summary: response?.summary ?? { created: 0, updated: 0, skipped: 0, failed: 0 },
      entries: Array.isArray(response?.entries) ? response.entries : [],
      warnings: Array.isArray(response?.warnings) ? response.warnings : [],
      info: Array.isArray(response?.info) ? response.info : [],
    }
  } catch (err: any) {
    error.value = err?.message || 'Failed to run scan'
  } finally {
    loading.value = false
  }
}

async function applyEntry(entry: ScanEntry) {
  if (!scanResponse.value?.dry_run || !isSelectable(entry)) return
  await runScan(false, [{ file: entry.file, line: entry.line }])
}

function clearResults() {
  scanResponse.value = null
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
  } catch (err: any) {
    state.error = err?.message || 'Failed to load preview'
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
  editTicketPatterns.value = (effective?.scan_ticket_patterns ?? []).join(', ')
  editEnableMentions.value = effective?.scan_enable_mentions ?? false
  editStripConfig.value = effective?.scan_strip_attributes ?? false
}

async function loadConfig() {
  configLoading.value = true
  configError.value = null
  try {
    configState.value = await api.inspectConfig(project.value || undefined)
    syncEditableFields()
  } catch (err: any) {
    configError.value = err?.message || 'Failed to load scan config'
  } finally {
    configLoading.value = false
  }
}

async function saveAllSettings() {
  configLoading.value = true
  configError.value = null
  try {
    const values: Record<string, string> = {
      scan_signal_words: JSON.stringify(parseList(editSignalWords.value)),
      scan_ticket_patterns: JSON.stringify(parseList(editTicketPatterns.value)),
      scan_enable_mentions: JSON.stringify(editEnableMentions.value),
      scan_strip_attributes: JSON.stringify(editStripConfig.value),
    }
    const payload: { values: Record<string, string>; project?: string } = { values }
    if (project.value) payload.project = project.value
    await api.setConfig(payload)
    await loadConfig()
  } catch (err: any) {
    configError.value = err?.message || 'Failed to save settings'
  } finally {
    configLoading.value = false
  }
}

function confirmRun() {
  const skipConfirm = localStorage.getItem(SKIP_RUN_CONFIRM_KEY) === 'true'
  if (skipConfirm) {
    runScan(false)
    return
  }
  skipFutureConfirm.value = false
  showRunConfirmDialog.value = true
}

function handleRunConfirm(skipFuture: boolean) {
  showRunConfirmDialog.value = false
  if (skipFuture) {
    localStorage.setItem(SKIP_RUN_CONFIRM_KEY, 'true')
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
  await loadConfig()
})
</script>
