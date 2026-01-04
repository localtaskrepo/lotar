<template>
  <div class="markdown-content" v-html="html"></div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { renderMarkdownToHtml } from '../utils/markdown';

const props = defineProps<{
  source: string
}>()

const html = computed(() => renderMarkdownToHtml(props.source || ''))
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
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  font-size: 0.92em;
}

.markdown-content :deep(blockquote) {
  margin: var(--space-2) 0;
  padding-left: var(--space-3);
  border-left: 3px solid var(--color-border);
  color: var(--color-muted);
}
</style>
