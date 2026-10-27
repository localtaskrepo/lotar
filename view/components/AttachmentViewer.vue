<template>
  <Teleport to="body">
    <div
      v-if="current"
      class="attachment-viewer"
      role="dialog"
      aria-modal="true"
      :aria-label="`Attachment ${current.name}`"
      @click.self="close"
    >
      <header class="attachment-viewer__bar">
        <div class="attachment-viewer__title">
          <strong>{{ current.name }}</strong>
          <span v-if="details" class="attachment-viewer__details">{{ details }}</span>
        </div>
        <span v-if="attachments.length > 1" class="attachment-viewer__count"
          >{{ index + 1 }} / {{ attachments.length }}</span
        >
        <a
          class="btn ghost icon-only attachment-viewer__action"
          :href="downloadUrl"
          :download="current.name"
          title="Download"
          aria-label="Download"
          ><IconGlyph name="download"
        /></a>
        <a
          class="btn ghost icon-only attachment-viewer__action"
          :href="current.url"
          target="_blank"
          rel="noopener"
          title="Open in a new tab"
          aria-label="Open in a new tab"
          ><IconGlyph name="external"
        /></a>
        <UiButton
          ref="closeButton"
          variant="ghost"
          icon-only
          class="attachment-viewer__action"
          title="Close (Esc)"
          aria-label="Close"
          @click="close"
        >
          <IconGlyph name="close" />
        </UiButton>
      </header>

      <div class="attachment-viewer__stage" @click.self="close">
        <UiButton
          v-if="attachments.length > 1"
          variant="ghost"
          icon-only
          class="attachment-viewer__nav attachment-viewer__nav--prev"
          title="Previous (←)"
          aria-label="Previous attachment"
          @click="step(-1)"
        >
          <IconGlyph name="chevron-left" />
        </UiButton>

        <template v-if="current.kind === 'image' && !failed">
          <img
            :key="current.url"
            class="attachment-viewer__image"
            :class="{ 'attachment-viewer__image--actual': actualSize }"
            :src="imageSrc"
            :alt="current.name"
            :title="actualSize ? 'Click to fit the screen' : 'Click for actual size'"
            @click="actualSize = !actualSize"
            @load="onImageLoad"
            @error="failed = true"
          />
        </template>
        <video
          v-else-if="current.kind === 'video' && !failed"
          :key="current.url"
          class="attachment-viewer__media"
          :src="current.url"
          controls
          playsinline
          preload="metadata"
          @error="failed = true"
        ></video>
        <audio
          v-else-if="current.kind === 'audio' && !failed"
          :key="current.url"
          class="attachment-viewer__audio"
          :src="current.url"
          controls
          preload="metadata"
          @error="failed = true"
        ></audio>
        <iframe
          v-else-if="current.kind === 'pdf'"
          :key="current.url"
          class="attachment-viewer__document"
          :src="current.url"
          :title="current.name"
        ></iframe>
        <pre v-else-if="textContent !== null" class="attachment-viewer__text">{{ textContent }}</pre>
        <div v-else class="attachment-viewer__fallback">
          <AttachmentTile :attachment="current" :size="72" />
          <p v-if="failed">This browser can’t show {{ current.label }} files.</p>
          <p v-else-if="textLoading">Loading…</p>
          <p v-else>No preview for {{ current.label }} files.</p>
          <a class="btn" :href="downloadUrl" :download="current.name">Download</a>
        </div>

        <UiButton
          v-if="attachments.length > 1"
          variant="ghost"
          icon-only
          class="attachment-viewer__nav attachment-viewer__nav--next"
          title="Next (→)"
          aria-label="Next attachment"
          @click="step(1)"
        >
          <IconGlyph name="chevron-right" />
        </UiButton>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import AttachmentTile from './AttachmentTile.vue'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'
import {
  attachmentExtension,
  attachmentPreviewUrl,
  formatFileSize,
  needsConvertedPreview,
  type TaskAttachmentView,
} from '../utils/taskAttachments'
import type { AttachmentInfo } from '../api/types'

const props = defineProps<{
  attachments: TaskAttachmentView[]
  /** Index of the attachment to show; null keeps the viewer closed. */
  openIndex: number | null
  /** Known sizes and dimensions, keyed by stored path. */
  info?: Record<string, AttachmentInfo>
}>()

const emit = defineEmits<{ (e: 'close'): void; (e: 'update:openIndex', value: number | null): void }>()

const TEXT_KINDS = new Set(['txt', 'md', 'log', 'json', 'yaml', 'yml', 'csv', 'tsv', 'xml'])
const MAX_TEXT_BYTES = 512 * 1024

const index = computed(() => {
  if (props.openIndex === null || !props.attachments.length) return -1
  return Math.max(0, Math.min(props.openIndex, props.attachments.length - 1))
})
const current = computed<TaskAttachmentView | null>(() =>
  index.value >= 0 ? (props.attachments[index.value] ?? null) : null,
)

const failed = ref(false)
const actualSize = ref(false)
const natural = ref<{ width: number; height: number } | null>(null)
const textContent = ref<string | null>(null)
const textLoading = ref(false)
const closeButton = ref<{ $el: HTMLElement } | null>(null)
let returnFocus: HTMLElement | null = null

const imageSrc = computed(() => {
  const view = current.value
  if (!view) return ''
  // TIFF only shows as a server-converted image; everything else in full.
  return needsConvertedPreview(view) ? attachmentPreviewUrl(view.url, 1600) : view.url
})

const downloadUrl = computed(() => {
  const url = current.value?.url ?? ''
  return `${url}${url.includes('?') ? '&' : '?'}download=1`
})

const details = computed(() => {
  const view = current.value
  if (!view) return ''
  const known = props.info?.[view.path]
  const parts: string[] = []
  const width = known?.width ?? natural.value?.width
  const height = known?.height ?? natural.value?.height
  if (width && height) parts.push(`${width}×${height}`)
  if (known) parts.push(formatFileSize(known.size))
  return parts.join(' · ')
})

function onImageLoad(event: Event) {
  const img = event.target as HTMLImageElement
  natural.value = { width: img.naturalWidth, height: img.naturalHeight }
}

async function loadText(view: TaskAttachmentView) {
  if (!TEXT_KINDS.has(attachmentExtension(view.path))) return
  textLoading.value = true
  try {
    const response = await fetch(view.url)
    if (!response.ok) return
    const text = await response.text()
    if (current.value?.url !== view.url) return
    textContent.value =
      text.length > MAX_TEXT_BYTES ? `${text.slice(0, MAX_TEXT_BYTES)}\n\n… (truncated; download to see it all)` : text
  } catch {
    // Leave the download fallback in place.
  } finally {
    textLoading.value = false
  }
}

watch(
  current,
  (view, previous) => {
    failed.value = false
    actualSize.value = false
    natural.value = null
    textContent.value = null
    if (view) void loadText(view)
    if (view && !previous) {
      returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
      window.addEventListener('keydown', onKeydown, true)
      void nextTick(() => closeButton.value?.$el?.focus())
    }
    if (!view && previous) {
      window.removeEventListener('keydown', onKeydown, true)
      returnFocus?.focus()
      returnFocus = null
    }
  },
  { immediate: true },
)

onUnmounted(() => window.removeEventListener('keydown', onKeydown, true))

function close() {
  emit('update:openIndex', null)
  emit('close')
}

function step(delta: number) {
  const count = props.attachments.length
  if (count < 2) return
  emit('update:openIndex', (index.value + delta + count) % count)
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    // Close only the viewer, not the task panel underneath.
    event.preventDefault()
    event.stopImmediatePropagation()
    close()
  } else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
    // Arrow keys still seek inside a focused video or audio player.
    if (event.target instanceof Element && event.target.closest('video, audio')) return
    event.preventDefault()
    step(event.key === 'ArrowLeft' ? -1 : 1)
  }
}
</script>

<style scoped>
.attachment-viewer {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal-high);
  display: flex;
  flex-direction: column;
  /* Lightbox is always near-black with light controls, independent of theme. */
  background: rgb(10 12 16 / 96%);
  backdrop-filter: blur(6px);
  color: #f1f5f9;
}

.attachment-viewer__bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 16px;
}

.attachment-viewer__title {
  display: flex;
  flex-direction: column;
  min-width: 0;
  margin-right: auto;
}

.attachment-viewer__title strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.attachment-viewer__details,
.attachment-viewer__count {
  font-size: var(--text-xs, 0.75rem);
  color: #cbd5e1;
}

.attachment-viewer__count {
  margin-right: 8px;
}

.attachment-viewer :deep(.btn.ghost) {
  color: #f1f5f9;
}

.attachment-viewer :deep(.btn.ghost:hover),
.attachment-viewer :deep(.btn.ghost:focus-visible) {
  background: rgb(255 255 255 / 12%);
}

.attachment-viewer__stage {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 56px 24px;
  overflow: auto;
}

.attachment-viewer__image {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  cursor: zoom-in;
  /* Checkerboard reveals transparency; fixed to suit the dark lightbox. */
  background: repeating-conic-gradient(#2a2f37 0% 25%, #353b45 0% 50%) 50% / 16px 16px;
}

.attachment-viewer__image--actual {
  max-width: none;
  max-height: none;
  cursor: zoom-out;
  margin: auto;
}

.attachment-viewer__media {
  max-width: 100%;
  max-height: 100%;
}

.attachment-viewer__audio {
  width: min(480px, 100%);
}

.attachment-viewer__document {
  width: min(960px, 100%);
  height: 100%;
  border: 0;
  border-radius: 6px;
  /* Documents (PDF etc.) are authored for a white page. */
  background: #fff;
}

.attachment-viewer__text {
  width: min(960px, 100%);
  max-height: 100%;
  margin: 0;
  padding: 16px;
  overflow: auto;
  border-radius: 6px;
  /* Text preview matches the dark lightbox rather than the theme. */
  background: #0f172a;
  color: #e2e8f0;
  font-size: 13px;
  white-space: pre-wrap;
  word-break: break-word;
}

.attachment-viewer__fallback {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  text-align: center;
}

.attachment-viewer__nav {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
}

.attachment-viewer__nav--prev {
  left: 8px;
}

.attachment-viewer__nav--next {
  right: 8px;
}

@media (max-width: 640px) {
  .attachment-viewer__stage {
    padding: 0 8px 64px;
  }

  .attachment-viewer__nav {
    top: auto;
    bottom: 12px;
    transform: none;
  }
}
</style>
