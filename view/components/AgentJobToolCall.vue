<template>
  <details v-if="expandable" class="tool-call" :class="`tool-call--${call.status}`" :open="call.status === 'failed'">
    <summary class="tool-call__line">
      <span class="tool-call__glyph" aria-hidden="true">{{ call.glyph }}</span>
      <span class="tool-call__label">{{ call.label }}</span>
      <span class="tool-call__what">{{ call.what }}</span>
      <span v-if="statusText" class="tool-call__status">{{ statusText }}</span>
      <span v-if="call.duration" class="tool-call__duration">{{ call.duration }}</span>
    </summary>
    <div class="tool-call__body">
      <template v-if="call.input">
        <span class="tool-call__heading">Input</span>
        <pre class="tool-call__pre">{{ call.input }}</pre>
      </template>
      <template v-if="call.output">
        <span class="tool-call__heading">Output</span>
        <pre class="tool-call__pre" :class="{ 'tool-call__pre--failed': call.status === 'failed' }">{{
          showAll ? call.output : clipped.text
        }}</pre>
        <button v-if="clipped.clipped" type="button" class="tool-call__more" @click="showAll = !showAll">
          {{ showAll ? 'Show less' : 'Show all' }}
        </button>
        <span v-if="call.outputBytes" class="tool-call__cut muted"
          >The log keeps the start of {{ formatBytes(call.outputBytes) }} of output.</span
        >
      </template>
    </div>
  </details>
  <div v-else class="tool-call" :class="`tool-call--${call.status}`">
    <div class="tool-call__line">
      <span class="tool-call__glyph" aria-hidden="true">{{ call.glyph }}</span>
      <span class="tool-call__label">{{ call.label }}</span>
      <span class="tool-call__what">{{ call.what }}</span>
      <span v-if="statusText" class="tool-call__status">{{ statusText }}</span>
      <span v-if="call.duration" class="tool-call__duration">{{ call.duration }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { clipOutput, type ToolCall } from '../utils/agentJobLog'

const props = defineProps<{ call: ToolCall }>()

const showAll = ref(false)
const expandable = computed(() => !!(props.call.input || props.call.output))
const clipped = computed(() => clipOutput(props.call.output ?? ''))
const statusText = computed(() =>
  props.call.status === 'failed' || props.call.status === 'running' ? props.call.status : '',
)

function formatBytes(bytes: number) {
  return bytes >= 1024 ? `${Math.round(bytes / 1024)} KB` : `${bytes} bytes`
}
</script>

<style scoped>
.tool-call {
  min-width: 0;
}

.tool-call__line {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
  padding: 2px 0;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--color-muted);
}

summary.tool-call__line {
  cursor: pointer;
  list-style: none;
}

summary.tool-call__line::-webkit-details-marker {
  display: none;
}

summary.tool-call__line::before {
  content: '▸';
  flex: none;
  width: 0.8em;
}

.tool-call[open] > summary.tool-call__line::before {
  content: '▾';
}

div.tool-call__line {
  padding-left: calc(0.8em + 8px);
}

.tool-call__glyph {
  flex: none;
  width: 1em;
  text-align: center;
}

.tool-call__label {
  flex: none;
  font-family: var(--font-sans);
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

.tool-call__what {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--color-fg);
}

.tool-call[open] .tool-call__what {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.tool-call__status,
.tool-call__duration {
  flex: none;
}

.tool-call--failed .tool-call__status,
.tool-call--failed .tool-call__label,
.tool-call--failed .tool-call__glyph {
  color: var(--color-danger);
}

.tool-call--running .tool-call__status {
  color: var(--color-accent-strong);
}

.tool-call__body {
  display: grid;
  gap: 4px;
  margin: 4px 0 6px calc(0.8em + 8px);
}

.tool-call__heading {
  font-size: 11px;
  font-weight: 600;
  color: var(--color-muted);
}

.tool-call__pre {
  margin: 0;
  padding: 6px 8px;
  border-radius: var(--radius-md);
  background: color-mix(in oklab, var(--color-fg) 5%, transparent);
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  color: var(--color-fg);
}

.tool-call__pre--failed {
  background: color-mix(in oklab, var(--color-danger) 8%, transparent);
}

.tool-call__more {
  justify-self: start;
  padding: 0;
  border: none;
  background: none;
  color: var(--color-accent-strong);
  font-size: 12px;
  cursor: pointer;
}

.tool-call__cut {
  font-size: 11px;
}
</style>
