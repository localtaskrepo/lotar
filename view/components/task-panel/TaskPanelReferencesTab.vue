<template>
  <div class="task-panel__tab-panel">
    <div class="task-panel__tab-actions">
      <UiButton
        v-if="mode === 'edit'"
        class="task-panel__tab-action"
        variant="ghost"
        icon-only
        type="button"
        data-testid="references-add-link"
        aria-label="Add link"
        title="Add link"
        :disabled="!taskId"
        @click="openAddLinkDialog"
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
          v-if="addLinkDialogOpen"
          class="task-panel-dialog__overlay"
          role="dialog"
          aria-modal="true"
          aria-label="Add link reference"
          data-testid="references-add-link-dialog"
          @click.self="closeAddLinkDialog"
        >
          <UiCard class="task-panel-dialog__card">
            <form class="task-panel-dialog__form" @submit.prevent="submitAddLink">
              <header class="task-panel-dialog__header">
                <h2>Add link reference</h2>
                <UiButton
                  variant="ghost"
                  icon-only
                  type="button"
                  aria-label="Close dialog"
                  title="Close dialog"
                  :disabled="addLinkSubmitting"
                  @click="closeAddLinkDialog"
                >
                  <IconGlyph name="close" />
                </UiButton>
              </header>
              <label class="task-panel-dialog__field" for="task-panel-add-link-input">
                <span class="muted">URL</span>
                <UiInput
                  id="task-panel-add-link-input"
                  ref="addLinkInputRef"
                  v-model="addLinkUrl"
                  placeholder="https://example.com"
                />
              </label>
              <footer class="task-panel-dialog__footer">
                <UiButton variant="primary" type="submit" :disabled="addLinkSubmitting || !addLinkUrl.trim()">
                  {{ addLinkSubmitting ? 'Adding…' : 'Add link' }}
                </UiButton>
                <UiButton variant="ghost" type="button" :disabled="addLinkSubmitting" @click="closeAddLinkDialog">
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

const addLinkDialogOpen = ref(false)
const addLinkUrl = ref('')
const addLinkSubmitting = ref(false)
const addLinkInputRef = ref<HTMLElement | null>(null)

function openAddLinkDialog() {
  if (!taskId.value) return
  addLinkUrl.value = ''
  addLinkDialogOpen.value = true
  nextTick(() => {
    // UiInput renders an input internally; focus if possible.
    ;(addLinkInputRef.value as any)?.focus?.()
  })
}

function closeAddLinkDialog() {
  if (addLinkSubmitting.value) return
  addLinkDialogOpen.value = false
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
    addLinkDialogOpen.value = false
  } catch (error: any) {
    console.warn('Failed to add link reference', error)
    showToast(error?.message || 'Failed to add link')
  } finally {
    addLinkSubmitting.value = false
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


