<template>
  <div ref="listEl" class="log-list" role="log" aria-live="polite" @scroll="onScroll">
    <div v-for="row in rows" :key="row.sequence" class="log-row" :class="`log-tone-${row.view.tone}`">
      <time class="log-time" :datetime="row.at" :title="formatDateTime(row.at)">{{ formatLogTime(row.at) }}</time>
      <span class="log-kind">{{ row.view.label }}</span>
      <details v-if="row.view.folded" class="log-message log-folded">
        <summary>
          <span class="log-folded-preview">{{ firstLine(row.view.text) }}</span>
          <span class="log-folded-hide">Hide</span>
        </summary>
        <MarkdownContent class="log-message--prose" :source="row.view.text" />
      </details>
      <MarkdownContent
        v-else-if="isProse(row.view.tone)"
        class="log-message log-message--prose"
        :source="row.view.text"
      />
      <div v-else class="log-message-block">
        <span class="log-message" :class="{ 'log-message--mono': row.view.mono }">{{ row.view.text }}</span>
        <pre v-if="row.view.detail" class="log-detail">{{ row.view.detail }}</pre>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import type { AgentJobLogEntry } from '../api/types'
import { describeLogEntry, formatLogTime, visibleLogEntries, type LogTone } from '../utils/agentJobs'
import MarkdownContent from './MarkdownContent.vue'
import { formatDateTime } from '../utils/date'

const props = defineProps<{ entries: AgentJobLogEntry[] }>()

const rows = computed(() =>
  visibleLogEntries(props.entries)
    .map((entry) => ({ sequence: entry.sequence, at: entry.at, view: describeLogEntry(entry) }))
    .filter((row) => row.view.text),
)

// Agents and people write markdown; tool lines and notes stay plain text.
const isProse = (tone: LogTone) => tone === 'agent' || tone === 'user'

// A folded entry (thinking) shows its first line until opened.
const FOLDED_PREVIEW = 120
function firstLine(text: string) {
  const line = text.split('\n', 1)[0] ?? ''
  return line.length > FOLDED_PREVIEW ? `${line.slice(0, FOLDED_PREVIEW)}…` : line
}

// Follow new output while the reader is at the bottom; stay put once they
// scroll up to read something.
const listEl = ref<HTMLElement | null>(null)
const following = ref(true)

function onScroll() {
  const el = listEl.value
  if (!el) return
  following.value = el.scrollHeight - el.scrollTop - el.clientHeight < 24
}

function scrollToEnd() {
  const el = listEl.value
  if (el && following.value) el.scrollTop = el.scrollHeight
}

onMounted(scrollToEnd)
watch(
  () => rows.value.length,
  () => nextTick(scrollToEnd),
)
</script>

<style scoped>
.log-list {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 2px;
  max-height: 420px;
  overflow-y: auto;
  font-size: 13px;
  padding-right: 4px;
}

.log-row {
  display: grid;
  grid-template-columns: max-content 4.5rem minmax(0, 1fr);
  gap: 10px;
  align-items: baseline;
  padding: 4px 6px;
  border-radius: var(--radius-md);
}

.log-time {
  color: var(--color-muted);
  font-family: var(--font-mono);
  font-size: 11px;
}

.log-kind {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: var(--color-muted);
}

.log-message {
  color: var(--color-fg);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  line-height: 1.45;
}

.log-message--prose {
  white-space: normal;
}

.log-message-block {
  display: grid;
  min-width: 0;
}

.log-folded {
  white-space: normal;
}

.log-folded summary {
  cursor: pointer;
  color: var(--color-muted);
  font-style: italic;
}

.log-folded-hide,
.log-folded[open] .log-folded-preview {
  display: none;
}

.log-folded[open] .log-folded-hide {
  display: inline;
}

.log-folded[open] summary {
  margin-bottom: 4px;
}

.log-detail {
  margin: 4px 0 0;
  padding: 6px 8px;
  border-radius: var(--radius-md);
  background: color-mix(in oklab, var(--color-danger) 6%, transparent);
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  color: var(--color-fg);
}

.log-message--mono {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--color-muted);
}

.log-tone-agent .log-kind {
  color: var(--color-accent-strong);
}

.log-tone-user {
  background: color-mix(in oklab, var(--color-accent) 8%, transparent);
}

.log-tone-user .log-kind {
  color: var(--color-accent-strong);
}

.log-tone-warning .log-kind,
.log-tone-warning .log-message {
  color: var(--color-warning);
}

.log-tone-success .log-kind {
  color: var(--color-success);
}

.log-tone-danger .log-kind,
.log-tone-danger .log-message {
  color: var(--color-danger);
}

@media (max-width: 640px) {
  .log-row {
    grid-template-columns: max-content minmax(0, 1fr);
  }

  /* :deep reaches MarkdownContent's root, which has no scope attribute of this component. */
  .log-row :deep(.log-message),
  .log-message-block {
    grid-column: 1 / -1;
  }
}
</style>
