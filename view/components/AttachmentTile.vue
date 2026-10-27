<template>
  <img
    v-if="attachment.kind === 'image' && !failed"
    class="attachment-tile attachment-tile--image"
    :class="{ 'attachment-tile--cover': cover }"
    :src="attachment.url"
    :alt="alt ?? ''"
    :title="title ?? attachment.name"
    :style="sizeStyle"
    loading="lazy"
    decoding="async"
    draggable="false"
    @error="failed = true"
  />
  <span
    v-else
    class="attachment-tile attachment-tile--file"
    :class="[`attachment-tile--${attachment.kind}`, { 'attachment-tile--cover': cover }]"
    :title="title ?? attachment.name"
    :style="sizeStyle"
    role="img"
    :aria-label="alt ?? attachment.name"
    >{{ attachment.label }}</span
  >
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { TaskAttachmentView } from '../utils/taskAttachments'

const props = defineProps<{
  attachment: TaskAttachmentView
  /** Square size in px; ignored for covers, which fill their container's width. */
  size?: number
  /** Fill the width as a card cover. */
  cover?: boolean
  alt?: string
  title?: string
}>()

const failed = ref(false)
const sizeStyle = computed(() =>
  props.cover ? undefined : { width: `${props.size ?? 32}px`, height: `${props.size ?? 32}px` },
)
</script>

<style scoped>
.attachment-tile {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  overflow: hidden;
}

.attachment-tile--image {
  object-fit: cover;
}

.attachment-tile--cover {
  display: block;
  width: 100%;
  height: 100%;
  border: none;
  border-radius: 0;
}

.attachment-tile--file {
  font-family: var(--font-mono, monospace);
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.02em;
  color: var(--color-muted);
}

.attachment-tile--pdf {
  color: var(--color-danger);
  background: color-mix(in oklab, var(--color-danger) 10%, var(--color-surface));
}

.attachment-tile--sheet {
  color: var(--color-success);
  background: color-mix(in oklab, var(--color-success) 10%, var(--color-surface));
}

.attachment-tile--video,
.attachment-tile--audio {
  color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 10%, var(--color-surface));
}

.attachment-tile--archive,
.attachment-tile--doc {
  color: var(--color-warning);
  background: color-mix(in oklab, var(--color-warning) 10%, var(--color-surface));
}
</style>
