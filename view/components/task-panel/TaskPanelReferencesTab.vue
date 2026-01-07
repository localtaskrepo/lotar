<template>
  <div class="task-panel__tab-panel">
    <div class="task-panel__tab-actions">
      <UiButton
        v-if="mode === 'edit'"
        class="task-panel__tab-action"
        variant="ghost"
        icon-only
        type="button"
        data-testid="references-add"
        aria-label="Add reference"
        title="Add reference"
        :disabled="!taskId"
        @click="openAddReferenceDialog"
      >
        <IconGlyph name="plus" aria-hidden="true" />
      </UiButton>
      <ReloadButton
        class="task-panel__tab-action"
        variant="ghost"
        :disabled="mode !== 'edit'"
        label="Reload references"
        title="Reload references"
        @click="$emit('reload')"
      />
    </div>
    <template v-if="mode === 'edit'">
      <div class="task-panel__references" role="region" aria-label="Task references">
        <p v-if="!references.length" class="muted">No references yet</p>
        <ul v-else class="task-panel__references-list">
          <li
            v-for="(reference, index) in references"
            :key="reference.code || reference.link || reference.file || index"
            :class="[
              'task-panel__reference-item',
              { 'task-panel__reference-item--interactive': !!reference.code }
            ]"
            :tabindex="reference.code ? 0 : undefined"
            @mouseenter="handleReferenceEnter(reference.code, $event)"
            @mouseleave="handleReferenceLeave(reference.code)"
            @focus="handleReferenceEnter(reference.code, $event)"
            @blur="handleReferenceLeave(reference.code)"
          >
            <span
              class="task-panel__reference-kind"
              :title="reference.file ? 'File reference' : reference.link ? 'Link reference' : reference.code ? 'Code reference' : 'Reference'"
              aria-hidden="true"
            >
              <IconGlyph :name="reference.file ? 'file' : 'list'" />
            </span>
            <a
              v-if="reference.link"
              class="task-panel__reference-link"
              :href="reference.link"
              target="_blank"
              rel="noreferrer"
            >
              {{ reference.link }}
            </a>
            <a
              v-else-if="reference.file"
              class="task-panel__reference-link"
              :href="attachmentUrl(reference.file)"
              target="_blank"
              rel="noopener"
              :title="attachmentHoverTitle(reference.file)"
            >
              {{ attachmentDisplayName(reference.file) }}
            </a>
            <span
              v-else-if="reference.code"
              class="task-panel__reference-text"
            >
              {{ reference.code }}
            </span>
            <span v-else class="task-panel__reference-text muted" aria-hidden="true">—</span>

            <UiButton
              class="task-panel__reference-remove"
              variant="ghost"
              icon-only
              type="button"
              :aria-label="reference.link ? 'Remove link' : reference.file ? 'Remove attachment' : reference.code ? 'Remove code reference' : 'Remove reference'"
              :title="reference.link ? 'Remove link' : reference.file ? 'Remove attachment' : reference.code ? 'Remove code reference' : 'Remove reference'"
              :disabled="!taskId || removingReferenceKey === referenceStableKey(reference)"
              @click.prevent.stop="removeReference(reference)"
            >
              <IconGlyph name="close" aria-hidden="true" />
            </UiButton>
          </li>
        </ul>
      </div>
      <Teleport to="body">
        <Transition name="fade">
          <div
            v-if="hoveredReferenceCode"
            class="task-panel__reference-preview"
            :style="hoveredReferenceStyle"
            role="dialog"
            aria-live="polite"
            @mouseenter="handleReferencePreviewEnter(hoveredReferenceCode)"
            @mouseleave="handleReferencePreviewLeave(hoveredReferenceCode)"
            :ref="setPreviewElement"
          >
            <div class="task-panel__reference-meta">
              <strong>{{ previewTitle }}</strong>
              <span v-if="hoveredReferenceSnippet">
                Lines {{ hoveredReferenceSnippet.start_line }}–{{ hoveredReferenceSnippet.end_line }}
              </span>
            </div>
            <p v-if="hoveredReferenceError" class="task-panel__reference-error">
              {{ hoveredReferenceError }}
            </p>
            <UiLoader v-else-if="hoveredReferenceLoading" size="sm">Loading reference…</UiLoader>
            <template v-else-if="hoveredReferenceSnippet">
              <div class="task-panel__reference-actions" v-if="hoveredReferenceCanExpand">
                <UiButton
                  v-if="hoveredReferenceCanExpandBefore"
                  variant="ghost"
                  type="button"
                  @click="expandReference('before')"
                >
                  Show earlier lines
                </UiButton>
                <UiButton
                  v-if="hoveredReferenceCanExpandAfter"
                  variant="ghost"
                  type="button"
                  @click="expandReference('after')"
                >
                  Show later lines
                </UiButton>
              </div>
              <div class="task-panel__reference-snippet">
                <div
                  v-for="line in hoveredReferenceSnippet.lines"
                  :key="line.number"
                  class="task-panel__reference-line"
                  :class="{
                    'task-panel__reference-line--highlight': isReferenceLineHighlighted(
                      hoveredReferenceCode,
                      line.number,
                    ),
                  }"
                >
                  <span class="task-panel__reference-line-number">{{ line.number }}</span>
                  <span class="task-panel__reference-line-text">{{ line.text }}</span>
                </div>
              </div>
            </template>
            <p v-else class="muted">No snippet preview available.</p>
          </div>
        </Transition>
      </Teleport>

      <Teleport to="body">
        <div
          v-if="addReferenceDialogOpen"
          :class="[
            'task-panel-dialog__overlay',
            'task-panel__references-dialog',
            addReferenceTab === 'link'
              ? 'task-panel__references-dialog--link'
              : 'task-panel__references-dialog--code',
            {
              'task-panel__references-dialog--has-preview':
                addReferenceTab === 'code' && !!addCodePreviewSnippet,
            },
          ]"
          role="dialog"
          aria-modal="true"
          aria-label="Add reference"
          data-testid="references-add-dialog"
          @click.self="closeAddReferenceDialog"
        >
          <UiCard class="task-panel-dialog__card">
            <form class="task-panel-dialog__form" @submit.prevent="submitAddReference">
              <header class="task-panel-dialog__header">
                <h2>Add reference</h2>
                <UiButton
                  variant="ghost"
                  icon-only
                  type="button"
                  aria-label="Close dialog"
                  title="Close dialog"
                  :disabled="addReferenceSubmitting"
                  @click="closeAddReferenceDialog"
                >
                  <IconGlyph name="close" />
                </UiButton>
              </header>

              <div class="task-panel__tabs task-panel__tabs--dialog" role="tablist" aria-label="Reference type">
                <button
                  type="button"
                  class="task-panel__tab"
                  :class="{ 'task-panel__tab--active': addReferenceTab === 'link' }"
                  role="tab"
                  data-testid="references-add-tab-link"
                  :aria-selected="addReferenceTab === 'link'"
                  @click="selectAddReferenceTab('link')"
                >
                  Link
                </button>
                <button
                  type="button"
                  class="task-panel__tab"
                  :class="{ 'task-panel__tab--active': addReferenceTab === 'code' }"
                  role="tab"
                  data-testid="references-add-tab-code"
                  :aria-selected="addReferenceTab === 'code'"
                  @click="selectAddReferenceTab('code')"
                >
                  Code
                </button>
              </div>

              <template v-if="addReferenceTab === 'link'">
                <label class="task-panel-dialog__field" for="task-panel-add-link-input">
                  <span class="muted">URL</span>
                  <UiInput
                    id="task-panel-add-link-input"
                    ref="addLinkInputRef"
                    v-model="addLinkUrl"
                    placeholder="https://example.com"
                  />
                </label>
              </template>

              <template v-else>
                <label class="task-panel-dialog__field" for="task-panel-add-code-file-input">
                  <span class="muted">File</span>
                  <UiInput
                    id="task-panel-add-code-file-input"
                    ref="addCodeFileInputRef"
                    v-model="addCodeFile"
                    :list="codeFileDatalistId"
                    placeholder="src/main.rs"
                    autocomplete="off"
                  />
                  <datalist :id="codeFileDatalistId">
                    <option v-for="item in addCodeFileSuggestions" :key="item" :value="item" />
                  </datalist>
                </label>

                <div class="task-panel-dialog__row">
                  <label class="task-panel-dialog__field" for="task-panel-add-code-start">
                    <span class="muted">Line</span>
                    <UiInput
                      id="task-panel-add-code-start"
                      v-model="addCodeStartLine"
                      type="number"
                      min="1"
                      placeholder="1"
                    />
                  </label>
                  <label class="task-panel-dialog__field" for="task-panel-add-code-end">
                    <span class="muted">End (optional)</span>
                    <UiInput
                      id="task-panel-add-code-end"
                      v-model="addCodeEndLine"
                      type="number"
                      min="1"
                      placeholder=""
                    />
                  </label>
                </div>

                <div class="task-panel-dialog__preview" v-if="addCodePreviewRequested">
                  <p v-if="addCodePreviewError" class="task-panel__reference-error">{{ addCodePreviewError }}</p>
                  <UiLoader v-else-if="addCodePreviewLoading" size="sm">Loading preview…</UiLoader>
                  <template v-else-if="addCodePreviewSnippet">
                    <div class="task-panel__reference-meta">
                      <strong>{{ addCodePreviewSnippet.path }}</strong>
                      <span>Lines {{ addCodePreviewSnippet.start_line }}–{{ addCodePreviewSnippet.end_line }}</span>
                    </div>
                    <div class="task-panel__reference-snippet" ref="addCodePreviewSnippetRef">
                      <div
                        v-for="line in addCodePreviewSnippet.lines"
                        :key="line.number"
                        class="task-panel__reference-line"
                        :data-line-number="line.number"
                        :class="{
                          'task-panel__reference-line--highlight':
                            line.number >= addCodePreviewSnippet.highlight_start &&
                            line.number <= addCodePreviewSnippet.highlight_end,
                        }"
                      >
                        <span class="task-panel__reference-line-number">{{ line.number }}</span>
                        <span class="task-panel__reference-line-text">{{ line.text }}</span>
                      </div>
                    </div>
                  </template>
                </div>
              </template>

              <footer class="task-panel-dialog__footer">
                <UiButton
                  variant="primary"
                  type="submit"
                  :disabled="addReferenceSubmitting || !addReferencePayloadReady"
                >
                  {{ addReferenceSubmitting ? 'Adding…' : addReferenceTab === 'code' ? 'Add code' : 'Add link' }}
                </UiButton>
                <UiButton variant="ghost" type="button" :disabled="addReferenceSubmitting" @click="closeAddReferenceDialog">
                  Cancel
                </UiButton>
              </footer>
            </form>
          </UiCard>
        </div>
      </Teleport>
    </template>
    <p v-else class="task-panel__empty-hint">References appear after the task is created.</p>
  </div>
</template>

<script setup lang="ts">
import { Teleport, Transition, computed, nextTick, onUnmounted, ref, watch, type ComponentPublicInstance } from 'vue'
import { api } from '../../api/client'
import type { ReferenceSnippet } from '../../api/types'
import IconGlyph from '../IconGlyph.vue'
import ReloadButton from '../ReloadButton.vue'
import UiButton from '../UiButton.vue'
import UiCard from '../UiCard.vue'
import UiInput from '../UiInput.vue'
import UiLoader from '../UiLoader.vue'
import { showToast } from '../toast'

type ReferenceEntry = { code?: string | null; link?: string | null; file?: string | null }

const props = defineProps<{
  mode: 'create' | 'edit'
  task: { id?: string | null; references?: ReferenceEntry[] | null }
  attachmentsDir?: string | null
  hoveredReferenceCode: string | null
  hoveredReferenceStyle: Record<string, string>
  hoveredReferenceLoading: boolean
  hoveredReferenceError: string | null
  hoveredReferenceSnippet: ReferenceSnippet | null
  hoveredReferenceCanExpand: boolean
  hoveredReferenceCanExpandBefore: boolean
  hoveredReferenceCanExpandAfter: boolean
  onReferenceEnter: (code?: string | null, event?: Event) => void
  onReferenceLeave: (code?: string | null) => void
  onReferencePreviewEnter: (code?: string | null) => void
  onReferencePreviewLeave: (code?: string | null) => void
  expandReferenceSnippet: (code?: string | null, direction?: 'before' | 'after') => void
  isReferenceLineHighlighted: (code: string, lineNumber: number) => boolean
  setReferencePreviewElement: (el: HTMLElement | null) => void
}>()

const emit = defineEmits<{ (e: 'reload'): void; (e: 'updated', task: any): void }>()

const taskId = computed(() => (props.task?.id || '').trim())

const references = computed(() =>
  (props.task?.references || []).filter((reference) =>
    Boolean(reference && (reference.code || reference.link || reference.file)),
  ),
)

const addReferenceDialogOpen = ref(false)
const addReferenceTab = ref<'link' | 'code'>('link')

const addLinkUrl = ref('')
const addLinkSubmitting = ref(false)
const addLinkInputRef = ref<HTMLElement | null>(null)

const addCodeFile = ref('')
const addCodeStartLine = ref('')
const addCodeEndLine = ref('')
const addCodeSubmitting = ref(false)
const addCodeFileInputRef = ref<HTMLElement | null>(null)
const addCodeFileSuggestions = ref<string[]>([])
const addCodePreviewLoading = ref(false)
const addCodePreviewError = ref<string | null>(null)
const addCodePreviewSnippet = ref<ReferenceSnippet | null>(null)
const addCodePreviewSnippetRef = ref<HTMLElement | null>(null)
const addCodePreviewScrollFocus = ref<'start' | 'end' | null>(null)

const removingReferenceKey = ref<string | null>(null)

const addReferenceSubmitting = computed(() => addLinkSubmitting.value || addCodeSubmitting.value)

const codeFileDatalistId = 'task-panel-code-file-suggestions'
let suggestFilesTimer: number | null = null
let previewTimer: number | null = null

function resetAddReferenceForm() {
  addLinkUrl.value = ''
  addCodeFile.value = ''
  addCodeStartLine.value = ''
  addCodeEndLine.value = ''
  addCodeFileSuggestions.value = []
  addCodePreviewSnippet.value = null
  addCodePreviewError.value = null
  addCodePreviewLoading.value = false
  addCodePreviewScrollFocus.value = null
}

function openAddReferenceDialog() {
  if (!taskId.value) return
  resetAddReferenceForm()
  addReferenceTab.value = 'link'
  addReferenceDialogOpen.value = true
  nextTick(() => {
    // UiInput renders an input internally; focus if possible.
    ;(addLinkInputRef.value as any)?.focus?.()
  })
}

function closeAddReferenceDialog() {
  if (addReferenceSubmitting.value) return
  addReferenceDialogOpen.value = false
}

function selectAddReferenceTab(tab: 'link' | 'code') {
  addReferenceTab.value = tab
  nextTick(() => {
    if (tab === 'link') {
      ;(addLinkInputRef.value as any)?.focus?.()
    } else {
      ;(addCodeFileInputRef.value as any)?.focus?.()
    }
  })
}

const addCodePayload = computed(() => {
  const file = addCodeFile.value.trim()
  if (!file) return null
  const start = Number(addCodeStartLine.value)
  if (!Number.isFinite(start) || start <= 0) return null

  const rawEnd = addCodeEndLine.value.trim()
  if (!rawEnd) {
    return { file, code: `${file}#${Math.floor(start)}` }
  }

  const end = Number(rawEnd)
  if (!Number.isFinite(end) || end <= 0) return null
  const startLine = Math.floor(start)
  const endLine = Math.floor(end)
  if (endLine < startLine) return null
  if (endLine === startLine) {
    return { file, code: `${file}#${startLine}` }
  }
  return { file, code: `${file}#${startLine}-${endLine}` }
})

const addCodePreviewRequested = computed(
  () => addReferenceDialogOpen.value && addReferenceTab.value === 'code' && !!addCodePayload.value,
)

const addReferencePayloadReady = computed(() => {
  if (addReferenceTab.value === 'link') {
    return addLinkUrl.value.trim().length > 0
  }
  return !!addCodePayload.value
})

async function submitAddReference() {
  if (addReferenceTab.value === 'link') {
    await submitAddLink()
    return
  }
  await submitAddCode()
}

function scrollAddCodePreviewToFocusedLine(focus: 'start' | 'end') {
  const snippetContainer = addCodePreviewSnippetRef.value
  const snippet = addCodePreviewSnippet.value
  if (!snippetContainer || !snippet) return

  const endRequested = addCodeEndLine.value.trim().length > 0
  const lineNumber =
    focus === 'end' && endRequested ? snippet.highlight_end || snippet.highlight_start : snippet.highlight_start
  if (!lineNumber) return

  const target = snippetContainer.querySelector(`[data-line-number="${lineNumber}"]`) as HTMLElement | null
  if (!target) return

  const card = snippetContainer.closest('.task-panel-dialog__card') as HTMLElement | null
  const scrollContainer =
    snippetContainer.scrollHeight > snippetContainer.clientHeight + 2 ? snippetContainer : card
  if (!scrollContainer) return

  const containerRect = scrollContainer.getBoundingClientRect()
  const targetRect = target.getBoundingClientRect()
  const targetTop = targetRect.top - containerRect.top + scrollContainer.scrollTop
  const padding = 12

  scrollContainer.scrollTo({
    top: Math.max(0, targetTop - padding),
    behavior: 'auto',
  })
}

async function submitAddCode() {
  const id = taskId.value
  if (!id) return
  const payload = addCodePayload.value
  if (!payload) return

  addCodeSubmitting.value = true
  try {
    const response = await api.addTaskCodeReference({ id, code: payload.code })
    emit('updated', response.task)
    showToast(response.added ? 'Code reference added' : 'Code reference already attached')
    addReferenceDialogOpen.value = false
  } catch (error: any) {
    console.warn('Failed to add code reference', error)
    showToast(error?.message || 'Failed to add code reference')
  } finally {
    addCodeSubmitting.value = false
  }
}

async function refreshFileSuggestions(query: string) {
  const cleaned = query.trim()
  if (!cleaned) {
    addCodeFileSuggestions.value = []
    return
  }
  try {
    addCodeFileSuggestions.value = await api.suggestReferenceFiles(cleaned, 30)
  } catch {
    addCodeFileSuggestions.value = []
  }
}

watch(addCodeFile, (value) => {
  if (!(addReferenceDialogOpen.value && addReferenceTab.value === 'code')) return
  if (suggestFilesTimer !== null && typeof window !== 'undefined') {
    window.clearTimeout(suggestFilesTimer)
  }
  if (typeof window === 'undefined') return
  suggestFilesTimer = window.setTimeout(() => {
    refreshFileSuggestions(value)
  }, 120)
})

function schedulePreviewFetch() {
  if (previewTimer !== null && typeof window !== 'undefined') {
    window.clearTimeout(previewTimer)
  }
  if (typeof window === 'undefined') return
  previewTimer = window.setTimeout(() => {
    fetchAddCodePreview()
  }, 140)
}

watch([addCodeFile, addCodeStartLine, addCodeEndLine, addReferenceDialogOpen, addReferenceTab], () => {
  if (!(addReferenceDialogOpen.value && addReferenceTab.value === 'code')) return
  schedulePreviewFetch()
})

watch(addCodeStartLine, () => {
  if (!(addReferenceDialogOpen.value && addReferenceTab.value === 'code')) return
  addCodePreviewScrollFocus.value = 'start'
})

watch(addCodeEndLine, () => {
  if (!(addReferenceDialogOpen.value && addReferenceTab.value === 'code')) return
  addCodePreviewScrollFocus.value = 'end'
})

async function fetchAddCodePreview() {
  const payload = addCodePayload.value
  if (!payload) {
    addCodePreviewSnippet.value = null
    addCodePreviewError.value = null
    addCodePreviewLoading.value = false
    return
  }
  addCodePreviewLoading.value = true
  addCodePreviewError.value = null
  try {
    addCodePreviewSnippet.value = await api.referenceSnippet(payload.code, { before: 6, after: 6 })
    const focus = addCodePreviewScrollFocus.value
    if (focus) {
      addCodePreviewScrollFocus.value = null
      await nextTick()
      if (typeof window !== 'undefined') {
        await new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()))
      }
      scrollAddCodePreviewToFocusedLine(focus)
    }
  } catch (error: any) {
    addCodePreviewSnippet.value = null
    addCodePreviewError.value = error?.message || 'Failed to load preview'
  } finally {
    addCodePreviewLoading.value = false
  }
}

async function submitAddLink() {
  const id = taskId.value
  if (!id) return

  const url = addLinkUrl.value.trim()
  if (!url) return
  if (!(url.startsWith('http://') || url.startsWith('https://'))) {
    showToast('Link must start with http:// or https://')
    return
  }

  addLinkSubmitting.value = true
  try {
    const response = await api.addTaskLinkReference({ id, url })
    emit('updated', response.task)
    showToast(response.added ? 'Link added' : 'Link already attached')
    addReferenceDialogOpen.value = false
  } catch (error: any) {
    console.warn('Failed to add link reference', error)
    showToast(error?.message || 'Failed to add link')
  } finally {
    addLinkSubmitting.value = false
  }
}

function referenceStableKey(reference: ReferenceEntry): string {
  if (!reference) return ''
  const link = typeof reference.link === 'string' ? reference.link.trim() : ''
  if (link) return `link:${link}`
  const file = typeof reference.file === 'string' ? reference.file.trim() : ''
  if (file) return `file:${file}`
  const code = typeof reference.code === 'string' ? reference.code.trim() : ''
  if (code) return `code:${code}`
  return ''
}

async function removeReference(reference: ReferenceEntry) {
  const id = taskId.value
  if (!id) return

  const key = referenceStableKey(reference)
  if (!key) return
  if (removingReferenceKey.value) return

  // If we're removing the currently-hovered code ref, dismiss the preview immediately.
  if (reference.code && props.hoveredReferenceCode === reference.code) {
    props.onReferenceLeave(reference.code)
    props.onReferencePreviewLeave(reference.code)
  }

  removingReferenceKey.value = key
  try {
    const link = typeof reference.link === 'string' ? reference.link.trim() : ''
    const file = typeof reference.file === 'string' ? reference.file.trim() : ''
    const code = typeof reference.code === 'string' ? reference.code.trim() : ''

    if (link) {
      const response = await api.removeTaskLinkReference({ id, url: link })
      emit('updated', response.task)
      showToast(response.removed ? 'Link removed' : 'Link already removed')
      return
    }

    if (file) {
      const response = await api.removeTaskAttachment({ id, stored_path: file })
      emit('updated', response.task)
      if (response.deleted) {
        showToast('Attachment removed')
      } else if (response.still_referenced) {
        showToast('Attachment removed (file kept: used by other tasks)')
      } else {
        showToast('Attachment removed (file may already be gone)')
      }
      return
    }

    if (code) {
      const response = await api.removeTaskCodeReference({ id, code })
      emit('updated', response.task)
      showToast(response.removed ? 'Code reference removed' : 'Code reference already removed')
      return
    }
  } catch (error: any) {
    console.warn('Failed to remove reference', { reference, error })
    showToast(error?.message || 'Failed to remove reference')
  } finally {
    removingReferenceKey.value = null
  }
}

function attachmentUrl(relPath: string): string {
  const stored = (relPath || '').trim()
  if (!stored) return '/api/attachments/get?path='

  const taskId = (props.task?.id || '').trim()
  const dashPos = taskId.indexOf('-')
  const project = dashPos > 0 ? taskId.slice(0, dashPos) : ''

  const hash = extractAttachmentHash(stored)
  const display = attachmentDisplayName(stored)
  if (hash) {
    const qs = project ? `?${new URLSearchParams({ project }).toString()}` : ''
    return `/api/attachments/h/${encodeURIComponent(hash)}/${encodeURIComponent(display)}${qs}`
  }

  const params = new URLSearchParams({ path: stored })
  if (project) params.set('project', project)
  return `/api/attachments/get?${params.toString()}`
}

function extractAttachmentHash(relPath: string): string | null {
  const cleaned = (relPath || '').trim()
  if (!cleaned) return null
  const parts = cleaned.split('/')
  const leaf = parts[parts.length - 1] || cleaned

  const lastDot = leaf.lastIndexOf('.')
  const base = lastDot > 0 ? leaf.slice(0, lastDot) : leaf
  const ext = lastDot > 0 ? leaf.slice(lastDot + 1) : ''

  if (ext.length === 32 && /^[0-9a-f]{32}$/i.test(ext)) {
    return ext
  }

  const m = base.match(/^(.*)[.-]([0-9a-f]{32})$/i)
  return m ? m[2] : null
}

function attachmentDisplayName(relPath: string): string {
  const cleaned = (relPath || '').trim()
  if (!cleaned) return 'attachment'
  const parts = cleaned.split('/')
  const leaf = parts[parts.length - 1] || cleaned

  const dot = leaf.lastIndexOf('.')
  const stem = dot > 0 ? leaf.slice(0, dot) : leaf
  const ext = dot > 0 ? leaf.slice(dot) : ''
  const hashMatch = stem.match(/^(.*)[.-]([0-9a-f]{32})$/i)
  if (hashMatch) {
    const displayStem = (hashMatch[1] || '').trim()
    return `${displayStem || 'attachment'}${ext}`
  }
  return leaf
}

function attachmentHoverTitle(relPath: string): string {
  const cleaned = (relPath || '').trim()
  if (!cleaned) return ''

  const configured = (props.attachmentsDir || '').trim()
  if (configured.startsWith('/')) {
    return `${configured.replace(/\/+$/, '')}/${cleaned.replace(/^\/+/, '')}`
  }

  const dir = (configured || '@attachments').replace(/^\/+/, '').replace(/\/+$/, '')
  const leaf = cleaned.replace(/^\/+/, '')
  return `.tasks/${dir}/${leaf}`
}

const previewTitle = computed(() => {
  if (props.hoveredReferenceSnippet?.path) {
    return props.hoveredReferenceSnippet.path
  }
  if (props.hoveredReferenceCode) {
    return props.hoveredReferenceCode
  }
  return 'Reference'
})

const previewElement = ref<HTMLElement | null>(null)

watch(previewElement, (element) => {
  props.setReferencePreviewElement(element)
})

onUnmounted(() => {
  props.setReferencePreviewElement(null)
  if (typeof window !== 'undefined') {
    if (suggestFilesTimer !== null) window.clearTimeout(suggestFilesTimer)
    if (previewTimer !== null) window.clearTimeout(previewTimer)
  }
  suggestFilesTimer = null
  previewTimer = null
})

function handleReferenceEnter(code: string | null | undefined, event?: Event) {
  props.onReferenceEnter(code ?? null, event)
}

function handleReferenceLeave(code: string | null | undefined) {
  props.onReferenceLeave(code ?? null)
}

function handleReferencePreviewEnter(code: string | null | undefined) {
  props.onReferencePreviewEnter(code ?? null)
}

function handleReferencePreviewLeave(code: string | null | undefined) {
  props.onReferencePreviewLeave(code ?? null)
}

function expandReference(direction: 'before' | 'after') {
  if (!props.hoveredReferenceCode) return
  props.expandReferenceSnippet(props.hoveredReferenceCode, direction)
}

function setPreviewElement(refValue: Element | ComponentPublicInstance | null) {
  if (!refValue) {
    previewElement.value = null
    return
  }
  if (refValue instanceof HTMLElement) {
    previewElement.value = refValue
    return
  }
  const element = (refValue as ComponentPublicInstance).$el as HTMLElement | null
  previewElement.value = element || null
}
</script>


