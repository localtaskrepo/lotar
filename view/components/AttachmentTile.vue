<template>
  <img
    v-if="attachment.kind === 'image' && !failed"
    ref="image"
    class="attachment-tile attachment-tile--image"
    :class="[`attachment-tile--fit-${fit}`, { 'attachment-tile--cover': cover }]"
    :src="previewSrc"
    :alt="alt ?? ''"
    :title="title ?? attachment.name"
    :style="sizeStyle"
    loading="lazy"
    decoding="async"
    draggable="false"
    @load="onLoad"
    @error="onError"
  />
  <span
    v-else-if="cover && attachment.kind === 'video' && !failed"
    class="attachment-tile attachment-tile--cover attachment-tile--video-frame"
    :title="title ?? attachment.name"
    role="img"
    :aria-label="alt ?? attachment.name"
  >
    <!-- The first frame; preload=metadata reads only what that needs. -->
    <video
      class="attachment-tile__video"
      :src="`${attachment.url}#t=0.1`"
      preload="metadata"
      muted
      playsinline
      disablepictureinpicture
      tabindex="-1"
      aria-hidden="true"
      @error="onError"
    ></video>
    <span class="attachment-tile__play" aria-hidden="true"><IconGlyph name="play" /></span>
  </span>
  <span
    v-else
    class="attachment-tile attachment-tile--file"
    :class="[`attachment-tile--${attachment.kind}`, { 'attachment-tile--cover': cover }]"
    :title="title ?? attachment.name"
    :style="sizeStyle"
    role="img"
    :aria-label="alt ?? attachment.name"
    ><span class="attachment-tile__label">{{ attachment.label }}</span
    ><span v-if="cover && failed" class="attachment-tile__note">No preview</span></span
  >
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import IconGlyph from './IconGlyph.vue'
import {
  attachmentExtension,
  attachmentPreviewUrl,
  imageFit,
  isAttachmentPreviewBroken,
  markAttachmentPreviewBroken,
  type ImageFit,
  type TaskAttachmentView,
} from '../utils/taskAttachments'

const props = defineProps<{
  attachment: TaskAttachmentView
  /** Square size in px; ignored for covers, which fill their container. */
  size?: number
  /** Fill the container, as a card cover or gallery tile. */
  cover?: boolean
  /**
   * Cover height in px. With it, a cover shrinks to fit small images and thin
   * banners instead of floating them in an empty band; without it the cover
   * fills its container's height.
   */
  coverHeight?: number
  alt?: string
  title?: string
  /**
   * Widest the tile is shown, in CSS pixels, to pick a server preview size.
   * Defaults to `size`, or 320 for covers.
   */
  previewWidth?: number
}>()

// A right-sized server preview instead of the full image (DEV-216).
const previewSrc = computed(() =>
  attachmentPreviewUrl(props.attachment.url, props.previewWidth ?? (props.cover ? 320 : (props.size ?? 32))),
)

const image = ref<HTMLImageElement | null>(null)
const failed = ref(isAttachmentPreviewBroken(props.attachment.url))
// Until the image loads we cannot know its shape; covering is the common case.
const fit = ref<ImageFit>('cover')
const fittedHeight = ref<number | null>(null)

watch(
  () => props.attachment.url,
  (url) => {
    failed.value = isAttachmentPreviewBroken(url)
    fit.value = 'cover'
    fittedHeight.value = null
  },
)

const sizeStyle = computed(() => {
  if (!props.cover) return { width: `${props.size ?? 32}px`, height: `${props.size ?? 32}px` }
  if (!props.coverHeight) return undefined
  return { height: `${fittedHeight.value ?? props.coverHeight}px` }
})

const COVER_PADDING = 8
const MIN_COVER_HEIGHT = 56

/** Height a cover needs to show a contained or natural-size image without empty bands. */
function shrunkCoverHeight(natural: { width: number; height: number }, box: { width: number; height: number }) {
  const inner =
    fit.value === 'natural'
      ? natural.height
      : Math.min(box.height, ((box.width - 2 * COVER_PADDING) * natural.height) / natural.width)
  return Math.round(Math.max(MIN_COVER_HEIGHT, Math.min(box.height, inner + 2 * COVER_PADDING)))
}

const ALPHA_FORMATS = new Set(['png', 'gif', 'webp', 'avif', 'ico'])

/** Whether the image's border is mostly see-through, like a logo or icon. */
function hasTransparentEdges(img: HTMLImageElement): boolean {
  if (!ALPHA_FORMATS.has(attachmentExtension(props.attachment.path))) return false
  try {
    const side = 16
    const canvas = document.createElement('canvas')
    canvas.width = side
    canvas.height = side
    const context = canvas.getContext('2d', { willReadFrequently: true })
    if (!context) return false
    context.drawImage(img, 0, 0, side, side)
    const data = context.getImageData(0, 0, side, side).data
    let edge = 0
    let clear = 0
    for (let y = 0; y < side; y += 1) {
      for (let x = 0; x < side; x += 1) {
        if (x !== 0 && y !== 0 && x !== side - 1 && y !== side - 1) continue
        edge += 1
        if (data[(y * side + x) * 4 + 3]! < 200) clear += 1
      }
    }
    return clear / edge > 0.25
  } catch {
    return false
  }
}

function onLoad() {
  const img = image.value
  if (!img) return
  const natural = { width: img.naturalWidth, height: img.naturalHeight }
  const box = { width: img.clientWidth, height: img.clientHeight }
  fit.value = imageFit(natural, box, {
    vector: attachmentExtension(props.attachment.path) === 'svg',
    transparent: hasTransparentEdges(img),
  })
  if (props.coverHeight && natural.width > 0 && (fit.value === 'contain' || fit.value === 'natural')) {
    fittedHeight.value = shrunkCoverHeight(natural, box)
  }
}

function onError() {
  markAttachmentPreviewBroken(props.attachment.url)
  failed.value = true
}
</script>

<style scoped>
.attachment-tile {
  --attachment-backdrop: color-mix(in oklab, var(--color-border) 30%, var(--color-surface));
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  overflow: hidden;
}

.attachment-tile--image {
  object-fit: cover;
}

.attachment-tile--fit-top {
  object-position: center top;
}

.attachment-tile--fit-contain,
.attachment-tile--fit-natural {
  background: var(--attachment-backdrop);
}

.attachment-tile--fit-contain {
  object-fit: contain;
}

/* Never enlarge an image past its own pixels; small icons stay crisp. */
.attachment-tile--fit-natural {
  object-fit: none;
}

.attachment-tile--cover {
  display: block;
  width: 100%;
  height: 100%;
  border: none;
  border-radius: 0;
}

.attachment-tile--cover.attachment-tile--fit-contain {
  padding: 8px;
}

.attachment-tile--cover.attachment-tile--file {
  min-height: 56px;
}

.attachment-tile--video-frame {
  position: relative;
  display: block;
  /* Letterbox behind video stays black regardless of theme. */
  background: #000;
}

.attachment-tile__video {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: cover;
  pointer-events: none;
}

.attachment-tile__play {
  position: absolute;
  inset: 50% auto auto 50%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  transform: translate(-50%, -50%);
  /* Play badge sits on video content, not on the theme surface. */
  color: #fff;
  background: rgb(0 0 0 / 55%);
  font-size: 16px;
}

.attachment-tile--file {
  font-family: var(--font-mono);
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.02em;
  color: var(--color-muted);
}

.attachment-tile--file.attachment-tile--cover {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 14px;
  background: var(--attachment-backdrop);
}

.attachment-tile__note {
  font-family: var(--font-sans, inherit);
  font-size: 12px;
  font-weight: 500;
  letter-spacing: normal;
}

.attachment-tile--pdf {
  color: var(--color-danger-strong);
  background: color-mix(in oklab, var(--color-danger) 10%, var(--color-surface));
}

.attachment-tile--sheet {
  color: var(--color-success-strong);
  background: color-mix(in oklab, var(--color-success) 10%, var(--color-surface));
}

.attachment-tile--video,
.attachment-tile--audio {
  color: var(--color-accent-strong);
  background: color-mix(in oklab, var(--color-accent) 10%, var(--color-surface));
}

.attachment-tile--archive,
.attachment-tile--doc {
  color: var(--color-warning-strong);
  background: color-mix(in oklab, var(--color-warning) 10%, var(--color-surface));
}
</style>
