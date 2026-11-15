<template>
  <!-- The body takes focus first, so arrow keys and Page Down scroll the file. -->
  <UiModal
    :open="!!location"
    size="full"
    :aria-label="location ? `File ${location.path}` : 'File'"
    initial-focus=".file-viewer__body"
    @close="closeFile"
  >
    <div v-if="location" class="file-viewer" data-testid="file-viewer">
      <header class="file-viewer__bar">
        <div class="file-viewer__title">
          <IconGlyph name="file" class="file-viewer__icon" />
          <strong class="file-viewer__path" :title="location.path">{{ location.path }}</strong>
          <span v-if="lineLabel" class="file-viewer__lines muted">{{ lineLabel }}</span>
        </div>
        <div class="file-viewer__actions">
          <div v-if="isMarkdown && content.kind === 'text'" class="file-viewer__modes" role="group" aria-label="View">
            <UiButton
              variant="ghost"
              class="file-viewer__mode"
              :aria-pressed="showRendered"
              @click="markdownMode = 'rendered'"
              >Rendered</UiButton
            >
            <UiButton
              variant="ghost"
              class="file-viewer__mode"
              :aria-pressed="!showRendered"
              @click="markdownMode = 'source'"
              >Source</UiButton
            >
          </div>
          <template v-if="content.kind !== 'error'">
            <a
              class="btn ghost icon-only"
              :href="rawUrl"
              target="_blank"
              rel="noopener"
              title="Open in a new tab"
              aria-label="Open in a new tab"
              ><IconGlyph name="external"
            /></a>
            <a class="btn ghost icon-only" :href="downloadUrl" download title="Download" aria-label="Download"
              ><IconGlyph name="download"
            /></a>
          </template>
          <UiButton variant="ghost" icon-only title="Close (Esc)" aria-label="Close" @click="closeFile">
            <IconGlyph name="close" />
          </UiButton>
        </div>
      </header>

      <div ref="body" class="file-viewer__body" tabindex="0">
        <div v-if="content.kind === 'loading'" class="file-viewer__status"><UiLoader /></div>
        <div v-else-if="content.kind === 'error'" class="file-viewer__status" role="alert">
          <p>{{ content.message }}</p>
          <UiButton variant="ghost" @click="load(path)">Try again</UiButton>
        </div>
        <div v-else-if="content.kind === 'image'" class="file-viewer__image">
          <img :src="rawUrl" :alt="location.path" @error="showError('The image could not be loaded.')" />
        </div>
        <p v-else-if="content.kind === 'binary'" class="file-viewer__status">
          No preview for this file type. <a :href="downloadUrl" download>Download it</a> instead.
        </p>
        <div v-else-if="showRendered" class="file-viewer__markdown">
          <MarkdownContent :source="content.text" :file-base="directory" />
        </div>
        <div v-else class="file-viewer__code" :style="{ '--file-viewer-gutter': `${String(lines.length).length}ch` }">
          <div
            v-for="(html, index) in lines"
            :key="index"
            class="file-viewer__line"
            :class="{ 'file-viewer__line--selected': isSelected(index + 1) }"
            :data-line="index + 1"
          >
            <a
              class="file-viewer__number"
              :href="fileHref({ path: location.path, line: index + 1 })"
              :aria-label="`Line ${index + 1}`"
              @click="selectLine($event, index + 1)"
              >{{ index + 1 }}</a
            >
            <!-- eslint-disable-next-line vue/no-v-html -- highlightLines escapes the text; highlight.js adds only <span> tags -->
            <code class="file-viewer__source" v-html="html"></code>
          </div>
        </div>
      </div>
    </div>
  </UiModal>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, shallowRef, watch } from 'vue'
import { api } from '../api/client'
import { useFileViewer } from '../composables/useFileViewer'
import { fileDirectory } from '../utils/fileReferences'
import { highlightLines, languageForPath } from '../utils/highlight'
import IconGlyph from './IconGlyph.vue'
import MarkdownContent from './MarkdownContent.vue'
import UiButton from './UiButton.vue'
import UiLoader from './UiLoader.vue'
import UiModal from './UiModal.vue'

type Content =
  | { kind: 'loading' }
  | { kind: 'error'; message: string }
  | { kind: 'image' }
  | { kind: 'binary' }
  | { kind: 'text'; text: string }

const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'avif', 'bmp', 'ico', 'svg'])

const { location, fileHref, openFile, closeFile } = useFileViewer()

const content = shallowRef<Content>({ kind: 'loading' })
const lines = shallowRef<string[]>([])
const markdownMode = ref<'rendered' | 'source'>('rendered')
const body = ref<HTMLElement | null>(null)
let loadToken = 0

const path = computed(() => location.value?.path ?? '')
const directory = computed(() => fileDirectory(path.value))
const extension = computed(() => path.value.slice(path.value.lastIndexOf('.') + 1).toLowerCase())
const isMarkdown = computed(() => extension.value === 'md' || extension.value === 'markdown')
const showRendered = computed(() => isMarkdown.value && markdownMode.value === 'rendered')
const rawUrl = computed(() => api.fileContentUrl(path.value))
const downloadUrl = computed(() => api.fileContentUrl(path.value, { download: true }))

const lineLabel = computed(() => {
  const { line, endLine } = location.value ?? {}
  if (!line) return ''
  return endLine ? `lines ${line}–${endLine}` : `line ${line}`
})

function isSelected(line: number) {
  const { line: start, endLine } = location.value ?? {}
  return !!start && line >= start && line <= (endLine ?? start)
}

function showError(message: string) {
  content.value = { kind: 'error', message }
}

async function load(target: string) {
  const token = ++loadToken
  lines.value = []
  if (IMAGE_EXTENSIONS.has(extension.value)) {
    content.value = { kind: 'image' }
    return
  }
  content.value = { kind: 'loading' }
  try {
    const response = await fetch(api.fileContentUrl(target))
    if (token !== loadToken) return
    if (!response.ok) {
      showError(await errorMessage(response))
      return
    }
    if (!(response.headers.get('Content-Type') ?? '').startsWith('text/plain')) {
      content.value = { kind: 'binary' }
      return
    }
    const text = (await response.text()).replace(/\r\n/g, '\n')
    if (token !== loadToken) return
    // A final newline ends the last line rather than starting an empty one.
    const highlighted = await highlightLines(text.endsWith('\n') ? text.slice(0, -1) : text, languageForPath(target))
    if (token !== loadToken) return
    lines.value = highlighted
    content.value = { kind: 'text', text }
  } catch {
    if (token === loadToken) showError('The file could not be loaded.')
  }
}

async function errorMessage(response: Response): Promise<string> {
  if (response.status === 404) return 'This file is not in the repository, or git ignores it.'
  try {
    const body = (await response.json()) as { error?: { message?: string } }
    if (body.error?.message) return body.error.message
  } catch {
    // Fall back to the status below.
  }
  return `The file could not be loaded (${response.status}).`
}

function selectLine(event: MouseEvent, line: number) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.altKey) return
  event.preventDefault()
  const current = location.value
  if (!current) return
  // Shift-click extends the selection to a range, as in code hosts.
  if (event.shiftKey && current.line && current.line !== line) {
    const start = Math.min(current.line, line)
    const end = Math.max(current.endLine ?? current.line, line)
    void openFile({ path: current.path, line: start, endLine: end }, { replace: true })
    return
  }
  void openFile({ path: current.path, line }, { replace: true })
}

async function scrollToLine() {
  const line = location.value?.line
  if (!line || !lines.value.length || showRendered.value) return
  await nextTick()
  body.value?.querySelector(`[data-line="${line}"]`)?.scrollIntoView({ block: 'center' })
}

watch(
  path,
  (next) => {
    if (!next) return
    // A reference to a line means the source; otherwise markdown reads rendered.
    markdownMode.value = location.value?.line ? 'source' : 'rendered'
    void load(next)
  },
  { immediate: true },
)

watch([lines, () => location.value?.line, showRendered], () => void scrollToLine())
</script>

<style scoped>
.file-viewer {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
}

.file-viewer__bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
}

.file-viewer__title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
  flex: 1 1 16rem;
}

.file-viewer__icon {
  flex: none;
  color: var(--color-muted);
}

.file-viewer__path {
  font-family: var(--font-mono);
  font-size: 0.92em;
  overflow-wrap: anywhere;
}

.file-viewer__lines {
  flex: none;
  font-size: 0.85em;
}

.file-viewer__actions {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  margin-left: auto;
}

.file-viewer__modes {
  display: inline-flex;
  margin-right: var(--space-1);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  overflow: hidden;
}

.file-viewer__mode {
  border-radius: 0;
}

.file-viewer__mode[aria-pressed='true'] {
  background: var(--color-surface);
  color: var(--color-fg);
  font-weight: 600;
}

.file-viewer__body {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  overscroll-behavior: contain;
}

.file-viewer__body:focus {
  outline: none;
}

.file-viewer__body:focus-visible {
  outline: 2px solid var(--color-accent);
  outline-offset: -2px;
}

.file-viewer__status {
  margin: 0;
  padding: var(--space-5) var(--space-4);
  color: var(--color-muted);
  text-align: center;
}

.file-viewer__status p {
  margin: 0 0 var(--space-3);
}

.file-viewer__image {
  display: flex;
  justify-content: center;
  padding: var(--space-4);
}

.file-viewer__image img {
  max-width: 100%;
  height: auto;
}

.file-viewer__markdown {
  max-width: 50rem;
  margin: 0 auto;
  padding: var(--space-4) var(--space-5);
  line-height: 1.6;
}

.file-viewer__code {
  padding: var(--space-2) 0;
  font-family: var(--font-mono);
  font-size: 0.85rem;
  line-height: 1.5;
}

.file-viewer__line {
  display: grid;
  grid-template-columns: calc(var(--file-viewer-gutter) + 2rem) minmax(0, 1fr);
}

.file-viewer__line--selected {
  background: color-mix(in oklab, var(--color-warning) 18%, transparent);
}

.file-viewer__number {
  padding: 0 var(--space-3) 0 var(--space-2);
  color: var(--color-muted);
  text-align: right;
  text-decoration: none;
  user-select: none;
}

.file-viewer__number:hover {
  color: var(--color-fg);
}

.file-viewer__source {
  padding-right: var(--space-3);
  font: inherit;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  tab-size: 4;
}

/* Token colors from the theme, so every theme gets readable code. */
.file-viewer__source :deep(.hljs-comment),
.file-viewer__source :deep(.hljs-quote),
.file-viewer__source :deep(.hljs-meta) {
  color: var(--color-muted);
}

.file-viewer__source :deep(.hljs-comment),
.file-viewer__source :deep(.hljs-quote) {
  font-style: italic;
}

.file-viewer__source :deep(.hljs-keyword),
.file-viewer__source :deep(.hljs-selector-tag),
.file-viewer__source :deep(.hljs-name),
.file-viewer__source :deep(.hljs-section) {
  color: var(--color-accent-strong);
}

.file-viewer__source :deep(.hljs-string),
.file-viewer__source :deep(.hljs-regexp),
.file-viewer__source :deep(.hljs-addition) {
  color: color-mix(in oklab, var(--color-success) 75%, var(--color-fg));
}

.file-viewer__source :deep(.hljs-number),
.file-viewer__source :deep(.hljs-literal),
.file-viewer__source :deep(.hljs-symbol),
.file-viewer__source :deep(.hljs-bullet) {
  color: color-mix(in oklab, var(--color-warning) 60%, var(--color-fg));
}

.file-viewer__source :deep(.hljs-title),
.file-viewer__source :deep(.hljs-type),
.file-viewer__source :deep(.hljs-built_in) {
  color: color-mix(in oklab, var(--color-accent-strong) 50%, var(--color-danger));
}

.file-viewer__source :deep(.hljs-attr),
.file-viewer__source :deep(.hljs-attribute),
.file-viewer__source :deep(.hljs-variable),
.file-viewer__source :deep(.hljs-template-variable),
.file-viewer__source :deep(.hljs-property),
.file-viewer__source :deep(.hljs-deletion) {
  color: color-mix(in oklab, var(--color-danger) 70%, var(--color-fg));
}

.file-viewer__source :deep(.hljs-emphasis) {
  font-style: italic;
}

.file-viewer__source :deep(.hljs-strong) {
  font-weight: 600;
}

@media (max-width: 640px) {
  .file-viewer__markdown {
    padding: var(--space-3);
  }

  .file-viewer__code {
    font-size: 0.8rem;
  }
}
</style>
