<template>
  <!-- eslint-disable vue/no-v-html -- html comes from renderMarkdown, which sanitizes with DOMPurify -->
  <div
    class="markdown-content"
    v-bind="$attrs"
    @click="onClick"
    @pointerdown="onPointerDown"
    @keydown.enter="onEnter"
    @mouseover="onPointerOver"
    @mouseout="onPointerOut"
    @focusin="onPointerOver"
    @focusout="onPointerOut"
    v-html="html"
  ></div>
  <!-- eslint-enable vue/no-v-html -->
  <TaskHoverCard v-if="hoverTask && hoverAnchor" :task="hoverTask" :anchor="hoverAnchor" :fields="hoverFields" />
</template>

<script lang="ts">
import type { TaskDTO } from '../api/types'

// Short-lived cache of hovered tasks, shared by every rendered markdown block,
// so moving over the same reference again does not refetch it.
const PREVIEW_TTL_MS = 15_000
const previews = new Map<string, { at: number; task: Promise<TaskDTO | null> }>()

function loadPreview(id: string, fetch: (id: string) => Promise<TaskDTO>): Promise<TaskDTO | null> {
  const cached = previews.get(id)
  if (cached && Date.now() - cached.at < PREVIEW_TTL_MS) return cached.task
  const task = fetch(id).catch(() => null)
  previews.set(id, { at: Date.now(), task })
  return task
}
</script>

<script setup lang="ts">
import { computed, inject, shallowRef, watch } from 'vue'
import { routerKey } from 'vue-router'
import { api } from '../api/client'
import { lookupFile } from '../composables/useFileReferences'
import { useFileViewer } from '../composables/useFileViewer'
import { useProjectNames } from '../composables/useProjectNames'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import {
  FILE_END_LINE_ATTRIBUTE,
  FILE_LINE_ATTRIBUTE,
  FILE_REFERENCE_ATTRIBUTE,
  FILE_REFERENCE_CLASS,
  renderMarkdownToHtml,
  TASK_REFERENCE_ATTRIBUTE,
  TASK_REFERENCE_CLASS,
} from '../utils/markdown'
import { attachmentUrl } from '../utils/taskAttachments'
import TaskHoverCard from './TaskHoverCard.vue'

defineOptions({ inheritAttrs: false })

const props = withDefaults(
  defineProps<{
    source: string
    /** Task whose `attachment:` links and images this text may show. */
    attachmentTaskId?: string | null
    /** Link task IDs such as `DEV-12` and show their hover card. */
    taskReferences?: boolean
    /** Link names of repository files to the file viewer. */
    fileReferences?: boolean
    /** Directory of the repository file this text comes from, for its relative links. */
    fileBase?: string
  }>(),
  { attachmentTaskId: null, taskReferences: true, fileReferences: true, fileBase: '' },
)

const router = inject(routerKey, null)
const { isProject } = useProjectNames()
const { navigateTask } = useTaskPanelController()
const { fileHref, openFile } = useFileViewer()

function taskHref(id: string) {
  const path = `/task/${encodeURIComponent(id)}`
  return router ? router.resolve(path).href : path
}

const html = computed(() =>
  renderMarkdownToHtml(props.source || '', {
    resolveAttachment: props.attachmentTaskId ? (stored) => attachmentUrl(stored, props.attachmentTaskId) : undefined,
    taskReferences: props.taskReferences ? { isProject, href: taskHref } : undefined,
    fileReferences: props.fileReferences
      ? { resolve: (reference) => lookupFile(reference, props.fileBase), href: fileHref }
      : undefined,
  }),
)

function referenceLink(target: EventTarget | null): HTMLAnchorElement | null {
  if (!(target instanceof Element)) return null
  return target.closest<HTMLAnchorElement>(`a.${TASK_REFERENCE_CLASS}[${TASK_REFERENCE_ATTRIBUTE}]`)
}

function fileLink(target: EventTarget | null): HTMLAnchorElement | null {
  if (!(target instanceof Element)) return null
  return target.closest<HTMLAnchorElement>(`a.${FILE_REFERENCE_CLASS}[${FILE_REFERENCE_ATTRIBUTE}]`)
}

// Reference links sit inside clickable areas such as the description, which
// would otherwise start editing; keep their presses to themselves.
function onPointerDown(event: PointerEvent) {
  if (referenceLink(event.target) || fileLink(event.target)) event.stopPropagation()
}

function onEnter(event: KeyboardEvent) {
  if (referenceLink(event.target) || fileLink(event.target)) event.stopPropagation()
}

function onClick(event: MouseEvent) {
  const link = referenceLink(event.target)
  const file = link ? null : fileLink(event.target)
  if (!link && !file) return
  event.stopPropagation()
  // Let the browser handle new-tab and new-window clicks.
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
  event.preventDefault()
  hideCard()
  if (file) {
    const path = file.getAttribute(FILE_REFERENCE_ATTRIBUTE)
    const line = Number(file.getAttribute(FILE_LINE_ATTRIBUTE)) || undefined
    const endLine = Number(file.getAttribute(FILE_END_LINE_ATTRIBUTE)) || undefined
    if (path) void openFile({ path, line, endLine })
    return
  }
  const id = link?.getAttribute(TASK_REFERENCE_ATTRIBUTE)
  if (id) void navigateTask(id)
}

const hoverAnchor = shallowRef<HTMLElement | null>(null)
const hoverTask = shallowRef<TaskDTO | null>(null)
let hoverToken = 0

// Every field the task has, including its custom fields: there is no per-view
// field choice for references.
const hoverFields = computed(() =>
  Object.fromEntries(Object.keys(hoverTask.value?.custom_fields ?? {}).map((key) => [`custom:${key}`, true])),
)

async function onPointerOver(event: Event) {
  const link = referenceLink(event.target)
  if (!link || link === hoverAnchor.value) return
  const id = link.getAttribute(TASK_REFERENCE_ATTRIBUTE)
  if (!id) return
  const token = ++hoverToken
  hoverAnchor.value = null
  hoverTask.value = null
  const task = await loadPreview(id, (taskId) => api.getTask(taskId))
  if (token !== hoverToken || !task || !link.isConnected) return
  hoverTask.value = task
  hoverAnchor.value = link
}

function onPointerOut(event: Event) {
  const link = referenceLink(event.target)
  if (!link) return
  const next = (event as MouseEvent | FocusEvent).relatedTarget
  if (next instanceof Node && link.contains(next)) return
  hideCard()
}

function hideCard() {
  hoverToken += 1
  hoverAnchor.value = null
  hoverTask.value = null
}

// A re-render replaces the links, so the hovered one is gone.
watch(html, hideCard)
</script>

<style scoped>
.markdown-content :deep(p:first-child) {
  margin-top: 0;
}

.markdown-content :deep(p:last-child) {
  margin-bottom: 0;
}

.markdown-content :deep(ul),
.markdown-content :deep(ol) {
  margin: var(--space-1) 0 var(--space-3);
  padding-left: 1.25rem;
}

.markdown-content :deep(li) {
  margin: 0.15rem 0;
}

.markdown-content :deep(pre) {
  margin: var(--space-2) 0;
  padding: var(--space-2);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  overflow-x: auto;
}

.markdown-content :deep(code) {
  font-family: var(--font-mono);
  font-size: 0.92em;
}

.markdown-content :deep(img) {
  max-width: 100%;
  height: auto;
  border-radius: var(--radius-md);
}

.markdown-content :deep(a.task-ref) {
  white-space: nowrap;
}

/* Inline code that names a file reads as a link, not as plain code. */
.markdown-content :deep(a.file-ref > code) {
  color: inherit;
  text-decoration: underline;
  text-decoration-color: color-mix(in oklab, currentColor 40%, transparent);
  text-underline-offset: 2px;
}

.markdown-content :deep(table) {
  margin: var(--space-2) 0 var(--space-3);
  border-collapse: collapse;
  display: block;
  max-width: 100%;
  overflow-x: auto;
}

.markdown-content :deep(th),
.markdown-content :deep(td) {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border);
  text-align: left;
}

.markdown-content :deep(th) {
  background: var(--color-surface);
}

.markdown-content :deep(blockquote) {
  margin: var(--space-2) 0;
  padding-left: var(--space-3);
  border-left: 3px solid var(--color-border);
  color: var(--color-muted);
}
</style>
