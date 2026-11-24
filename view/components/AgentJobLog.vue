<template>
  <div class="job-log" :class="{ 'job-log--expanded': expanded }">
    <div class="job-log__header">
      <div class="job-log__views" role="group" aria-label="Show in the log">
        <button
          v-for="option in LOG_VIEWS"
          :key="option.value"
          type="button"
          class="job-log__chip"
          :class="{ 'job-log__chip--problems': option.value === 'problems' && counts.problems }"
          :aria-pressed="view === option.value ? 'true' : 'false'"
          @click="setView(option.value)"
        >
          {{ option.label }} <span class="job-log__count">{{ counts[option.value] }}</span>
        </button>
        <button
          v-if="counts.thinking"
          type="button"
          class="job-log__chip"
          :aria-pressed="thinking ? 'true' : 'false'"
          @click="setThinking(!thinking)"
        >
          Thinking <span class="job-log__count">{{ counts.thinking }}</span>
        </button>
      </div>
      <div class="job-log__actions">
        <input
          v-model="query"
          type="search"
          class="input job-log__search"
          placeholder="Search the log"
          aria-label="Search the log"
        />
        <span v-if="query.trim()" class="job-log__matches muted" aria-live="polite">{{ matchText }}</span>
        <UiButton
          v-if="instructions && job"
          variant="ghost"
          class="job-log__instructions"
          :aria-expanded="instructionsOpen ? 'true' : 'false'"
          :aria-controls="instructionsId"
          @click="toggleInstructions"
        >
          {{ instructionsOpen ? 'Hide instructions' : 'Instructions' }}
        </UiButton>
        <a v-if="jobLink" class="job-log__link" :href="jobLink" title="Link to this job's log">Link</a>
        <button
          type="button"
          class="job-log__link"
          :aria-pressed="expanded ? 'true' : 'false'"
          @click="expanded = !expanded"
        >
          {{ expanded ? 'Shorter' : 'Full height' }}
        </button>
      </div>
    </div>
    <AgentRunInputsPanel
      v-if="instructions && job && instructionsSeen"
      v-show="instructionsOpen"
      :id="instructionsId"
      :job-id="job.id"
      :open="instructionsOpen"
      class="job-log__instructions-panel"
    />
    <div class="job-log__body">
      <div ref="listEl" class="log-list" role="log" aria-live="polite" @scroll="onScroll">
        <template v-for="item in items" :key="item.key">
          <details v-if="item.type === 'setup'" class="log-group log-group--setup">
            <summary class="log-row log-group__summary">
              <time class="log-time" :datetime="item.at" :title="formatDateTime(item.at)">{{
                formatLogTime(item.at)
              }}</time>
              <span class="log-kind">Setup</span>
              <span class="log-group__text">{{ item.entries.length }} notes before the first turn</span>
            </summary>
            <div v-for="leaf in item.entries" :key="leaf.key" class="log-row" :class="`log-tone-${leaf.view.tone}`">
              <time class="log-time" :datetime="leaf.at">{{ formatLogTime(leaf.at) }}</time>
              <span class="log-kind">{{ leaf.view.label }}</span>
              <span class="log-message">{{ leaf.view.text }}</span>
            </div>
          </details>

          <div v-else-if="item.type === 'tools' && item.calls.length === 1" class="log-row log-tone-tool">
            <time class="log-time" :datetime="item.at" :title="formatDateTime(item.at)">{{
              formatLogTime(item.at)
            }}</time>
            <span class="log-kind">Tool</span>
            <AgentJobToolCall :call="(item.calls[0] as ToolLeaf).call" />
          </div>

          <details
            v-else-if="item.type === 'tools'"
            class="log-group log-group--tools"
            :open="groupOpen(item) || undefined"
          >
            <summary class="log-row log-group__summary">
              <time class="log-time" :datetime="item.at" :title="formatDateTime(item.at)">{{
                formatLogTime(item.at)
              }}</time>
              <span class="log-kind">Tools</span>
              <span class="log-group__text"
                >{{ item.summary
                }}<span v-if="item.failed" class="log-group__failed"> · {{ item.failed }} failed</span></span
              >
            </summary>
            <div class="log-group__calls">
              <AgentJobToolCall v-for="leaf in item.calls" :key="leaf.key" :call="leaf.call" />
            </div>
          </details>

          <div v-else-if="item.type === 'question'" class="log-row log-tone-warning log-question">
            <time class="log-time" :datetime="item.at" :title="formatDateTime(item.at)">{{
              formatLogTime(item.at)
            }}</time>
            <span class="log-kind">Question</span>
            <div class="log-message-block">
              <span class="log-question__lead"
                >Asks to {{ item.call ? item.call.label.toLowerCase() : 'go on'
                }}<template v-if="item.call">:</template></span
              >
              <code v-if="item.call" class="log-question__input">{{ item.call.input || item.call.what }}</code>
              <span v-if="!item.call || item.title !== item.call.what" class="log-message log-message--mono">{{
                item.title
              }}</span>
              <span v-if="item.options.length" class="log-question__options">
                <span v-for="option in item.options" :key="option" class="log-question__option">{{ option }}</span>
              </span>
            </div>
          </div>

          <div v-else-if="item.type === 'result'" class="log-result" :class="`log-result--${item.view.tone}`">
            <div class="log-result__head">
              <span class="log-kind">{{ item.view.label }}</span>
              <time class="log-time" :datetime="item.at" :title="formatDateTime(item.at)">{{
                formatLogTime(item.at)
              }}</time>
            </div>
            <MarkdownContent v-if="item.reply" class="log-message log-message--prose" :source="item.reply" />
            <p
              v-if="!item.reply || item.view.tone !== 'success'"
              class="log-message log-result__end"
              :class="{ 'log-message--mono': item.view.mono }"
            >
              {{ item.view.text }}
            </p>
          </div>

          <div v-else-if="item.type === 'entry'" class="log-row" :class="`log-tone-${item.view.tone}`">
            <time class="log-time" :datetime="item.at" :title="formatDateTime(item.at)">{{
              formatLogTime(item.at)
            }}</time>
            <span class="log-kind">{{ item.view.label }}</span>
            <details v-if="item.view.folded" class="log-message log-folded">
              <summary>
                <span class="log-folded-preview">{{ firstLine(item.view.text) }}</span>
                <span class="log-folded-hide">Hide</span>
              </summary>
              <MarkdownContent class="log-message--prose" :source="item.view.text" />
            </details>
            <MarkdownContent
              v-else-if="isProse(item.view.tone)"
              class="log-message log-message--prose"
              :source="item.view.text"
            />
            <div v-else class="log-message-block">
              <span class="log-message" :class="{ 'log-message--mono': item.view.mono }">{{
                clipText(item.key, item.view.text)
              }}</span>
              <button
                v-if="isClipped(item.view.text)"
                type="button"
                class="job-log__link"
                @click="toggleClip(item.key)"
              >
                {{ unclipped.has(item.key) ? 'Show less' : 'Show all' }}
              </button>
              <pre v-if="item.view.detail" class="log-detail">{{ item.view.detail }}</pre>
            </div>
          </div>
        </template>
        <p v-if="!items.length" class="muted job-log__empty">
          {{ query.trim() ? 'Nothing in this view matches the search.' : 'Nothing in this view.' }}
        </p>
      </div>
      <button v-if="!following" type="button" class="job-log__jump" @click="jumpToLatest">Jump to latest</button>
    </div>
    <div v-if="running" class="job-log__live" role="status" data-testid="job-log-live">
      <span class="job-log__pulse" aria-hidden="true"></span>
      <span v-if="activity.call" class="job-log__live-text"
        >Running <code>{{ activity.call.what }}</code><template v-if="activitySince">
          · {{ activitySince }}</template
        ></span
      >
      <span v-else-if="activity.thinking" class="job-log__live-text">Thinking…</span>
      <span v-else class="job-log__live-text"
        >Working<template v-if="sinceLast"> · last update {{ sinceLast }} ago</template></span
      >
      <span v-if="context" class="job-log__context" :title="context.title">{{ context.text }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue'
import type { AgentJob, AgentJobLogEntry } from '../api/types'
import { formatDuration, formatLogTime, type LogTone } from '../utils/agentJobs'
import {
  buildLogLeaves,
  countLeaves,
  layoutLog,
  liveActivity,
  LOG_VIEWS,
  type LogItem,
  type LogView,
  type ToolGroup,
  type ToolLeaf,
} from '../utils/agentJobLog'
import { formatDateTime } from '../utils/date'
import { storageGetJson, storageSetJson } from '../utils/storage'
import AgentJobToolCall from './AgentJobToolCall.vue'
import AgentRunInputsPanel from './AgentRunInputsPanel.vue'
import MarkdownContent from './MarkdownContent.vue'
import UiButton from './UiButton.vue'

const props = withDefaults(
  defineProps<{
    entries: AgentJobLogEntry[]
    /** The job: its status and usage drive the live line, its ID the link. */
    job?: AgentJob | null
    /** Where the log is shown; each place remembers its own view. */
    viewer?: string
    /** Offer the job's recorded prompt and settings: the button in the header, the panel below it. */
    instructions?: boolean
  }>(),
  { job: null, viewer: 'agents', instructions: false },
)

// The chosen view and the thinking toggle, remembered per viewer.
const storageKey = computed(() => `lotar.agentJobLog.${props.viewer}`)
function storedView(): { view: LogView; thinking: boolean } {
  const stored = storageGetJson(storageKey.value)
  const record = stored && typeof stored === 'object' ? (stored as Record<string, unknown>) : {}
  const view = LOG_VIEWS.find((option) => option.value === record.view)?.value ?? 'all'
  return { view, thinking: record.thinking === true }
}
const initial = storedView()
const view = ref<LogView>(initial.view)
const thinking = ref(initial.thinking)
const query = ref('')
const expanded = ref(false)

const instructionsId = `job-log-instructions-${useId()}`
const instructionsOpen = ref(false)
// The panel stays mounted once opened; it loads on its first open and again only while there is no record.
const instructionsSeen = ref(false)
function toggleInstructions() {
  instructionsOpen.value = !instructionsOpen.value
  instructionsSeen.value = true
}

function remember() {
  storageSetJson(storageKey.value, { view: view.value, thinking: thinking.value })
}
function setView(next: LogView) {
  view.value = next
  remember()
}
function setThinking(next: boolean) {
  thinking.value = next
  remember()
}

const leaves = computed(() => buildLogLeaves(props.entries))
const counts = computed(() => countLeaves(leaves.value))
const items = computed<LogItem[]>(() =>
  layoutLog(leaves.value, { view: view.value, thinking: thinking.value, query: query.value }),
)

const matchCount = computed(() =>
  items.value.reduce((sum, item) => {
    if (item.type === 'tools') return sum + item.calls.length
    if (item.type === 'setup') return sum + item.entries.length
    return sum + 1
  }, 0),
)
const matchText = computed(() => `${matchCount.value} ${matchCount.value === 1 ? 'match' : 'matches'}`)

// A group opens by itself when the search found something in it.
const groupOpen = (item: ToolGroup) => !!query.value.trim() && item.calls.length > 0

const jobLink = computed(() => (props.job ? `/agents?job=${encodeURIComponent(props.job.id)}` : ''))

// Agents and people write markdown; tool lines and notes stay plain text.
const isProse = (tone: LogTone) => tone === 'agent' || tone === 'user'

// A folded entry (thinking) shows its first line until opened.
const FOLDED_PREVIEW = 120
function firstLine(text: string) {
  const line = text.split('\n', 1)[0] ?? ''
  return line.length > FOLDED_PREVIEW ? `${line.slice(0, FOLDED_PREVIEW)}…` : line
}

// Long notes (a failed check's output) show their start until "Show all".
const NOTE_LINES = 8
const unclipped = ref(new Set<string>())
const isClipped = (text: string) => text.split('\n').length > NOTE_LINES
function clipText(key: string, text: string) {
  if (!isClipped(text) || unclipped.value.has(key)) return text
  return `${text.split('\n').slice(0, NOTE_LINES).join('\n')}\n…`
}
function toggleClip(key: string) {
  const next = new Set(unclipped.value)
  if (next.has(key)) next.delete(key)
  else next.add(key)
  unclipped.value = next
}

// The live line while the job runs.
const running = computed(() => props.job?.status === 'running')
const activity = computed(() => liveActivity(leaves.value))
const now = ref(Date.now())
let clock: ReturnType<typeof setInterval> | null = null
watch(
  running,
  (value) => {
    if (value && !clock) clock = setInterval(() => (now.value = Date.now()), 1000)
    if (!value && clock) {
      clearInterval(clock)
      clock = null
    }
  },
  { immediate: true },
)
onBeforeUnmount(() => {
  if (clock) clearInterval(clock)
  clearHighlights()
})

function since(at?: string) {
  const start = at ? Date.parse(at) : NaN
  return Number.isNaN(start) ? '' : formatDuration(Math.max(0, now.value - start))
}
const activitySince = computed(() => since(activity.value.call?.startedAt))
const sinceLast = computed(() => since(props.entries[props.entries.length - 1]?.at))

const compact = new Intl.NumberFormat(undefined, { notation: 'compact', maximumSignificantDigits: 3 })
const context = computed(() => {
  const usage = props.job?.usage
  if (!usage?.context_used) return null
  if (!usage.context_size)
    return { text: `Context ${compact.format(usage.context_used)}`, title: 'Tokens in the context window' }
  const percent = Math.round((usage.context_used / usage.context_size) * 100)
  return {
    text: `Context ${percent}%`,
    title: `${usage.context_used.toLocaleString()} of ${usage.context_size.toLocaleString()} tokens in the context window`,
  }
})

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

function jumpToLatest() {
  following.value = true
  scrollToEnd()
}

onMounted(scrollToEnd)
watch(
  () => props.entries.length,
  () => nextTick(scrollToEnd),
)

// Search matches are highlighted with the CSS Custom Highlight API, which marks text without
// changing the markup (markdown included). Browsers without it still filter.
const highlightKey = Symbol('job-log')
function clearHighlights() {
  searchRanges.delete(highlightKey)
  applyHighlights()
}
function highlight() {
  const el = listEl.value
  const needle = query.value.trim().toLowerCase()
  const ranges: Range[] = []
  if (el && needle) {
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT)
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = node.textContent?.toLowerCase() ?? ''
      for (let at = text.indexOf(needle); at !== -1; at = text.indexOf(needle, at + needle.length)) {
        const range = document.createRange()
        range.setStart(node, at)
        range.setEnd(node, at + needle.length)
        ranges.push(range)
      }
    }
  }
  searchRanges.set(highlightKey, ranges)
  applyHighlights()
}
watch([items, query], () => nextTick(highlight))
</script>

<script lang="ts">
// Every log on the page shares the one highlight named in the CSS.
const searchRanges = new Map<symbol, Range[]>()
function applyHighlights() {
  if (typeof CSS === 'undefined' || !('highlights' in CSS) || typeof Highlight === 'undefined') return
  const all = [...searchRanges.values()].flat()
  if (all.length) CSS.highlights.set('job-log-search', new Highlight(...all))
  else CSS.highlights.delete('job-log-search')
}
</script>

<style scoped>
.job-log {
  display: grid;
  gap: 8px;
  min-width: 0;
}

.job-log__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.job-log__views,
.job-log__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.job-log__chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  border: 1px solid var(--color-border);
  border-radius: 999px;
  background: transparent;
  color: var(--color-fg);
  font-size: 12px;
  cursor: pointer;
}

.job-log__chip:hover {
  border-color: var(--color-border-strong);
}

.job-log__chip[aria-pressed='true'] {
  border-color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 14%, transparent);
  color: var(--color-accent-strong);
}

.job-log__count {
  color: var(--color-muted);
  font-variant-numeric: tabular-nums;
}

.job-log__chip--problems .job-log__count {
  color: var(--color-danger);
  font-weight: 600;
}

.job-log__search {
  width: 14rem;
  max-width: 100%;
  min-height: 28px;
  font-size: 13px;
}

.job-log__matches {
  font-size: 12px;
}

.job-log__link {
  padding: 0;
  border: none;
  background: none;
  color: var(--color-accent-strong);
  font-size: 12px;
  cursor: pointer;
  text-decoration: none;
}

.job-log__link:hover {
  text-decoration: underline;
}

.job-log__instructions-panel {
  padding-bottom: 10px;
  border-bottom: 1px solid var(--color-border);
}

.job-log__body {
  position: relative;
  min-width: 0;
}

.log-list {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 2px;
  max-height: 420px;
  overflow-y: auto;
  font-size: 13px;
  padding-right: 4px;
}

.job-log--expanded .log-list {
  max-height: none;
}

.job-log__empty {
  padding: 6px;
  margin: 0;
}

.job-log__jump {
  position: absolute;
  right: 12px;
  bottom: 8px;
  padding: 4px 12px;
  border: 1px solid var(--color-border-strong);
  border-radius: 999px;
  background: var(--color-surface);
  color: var(--color-fg);
  font-size: 12px;
  cursor: pointer;
  box-shadow: 0 2px 6px color-mix(in oklab, var(--color-fg) 15%, transparent);
}

.job-log__live {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--radius-md);
  background: color-mix(in oklab, var(--color-accent) 8%, transparent);
  font-size: 12px;
}

.job-log__live-text {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.job-log__live code {
  font-family: var(--font-mono);
}

.job-log__context {
  flex: none;
  color: var(--color-muted);
  font-variant-numeric: tabular-nums;
}

.job-log__pulse {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--color-accent);
  animation: job-log-pulse 1.4s ease-in-out infinite;
}

@keyframes job-log-pulse {
  50% {
    opacity: 0.35;
  }
}

@media (prefers-reduced-motion: reduce) {
  .job-log__pulse {
    animation: none;
  }
}

.log-row {
  display: grid;
  grid-template-columns: max-content 4.5rem minmax(0, 1fr);
  gap: 10px;
  align-items: baseline;
  padding: 4px 6px;
  border-radius: var(--radius-md);
}

.log-group > summary {
  cursor: pointer;
  list-style: none;
}

.log-group > summary::-webkit-details-marker {
  display: none;
}

.log-group__summary:hover {
  background: color-mix(in oklab, var(--color-fg) 4%, transparent);
}

.log-group__text {
  color: var(--color-muted);
  font-family: var(--font-mono);
  font-size: 12px;
}

.log-group__text::before {
  content: '▸ ';
}

.log-group[open] .log-group__text::before {
  content: '▾ ';
}

.log-group__failed {
  color: var(--color-danger);
  font-weight: 600;
}

.log-group__calls {
  display: grid;
  gap: 1px;
  margin: 0 6px 4px;
  padding: 2px 6px 4px 12px;
  border-left: 2px solid var(--color-border);
}

.log-group--setup[open] {
  padding-bottom: 4px;
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
  gap: 2px;
  min-width: 0;
  justify-items: start;
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

.log-question__lead {
  color: var(--color-fg);
}

.log-question__input {
  max-width: 100%;
  padding: 2px 6px;
  border-radius: var(--radius-md);
  background: color-mix(in oklab, var(--color-fg) 6%, transparent);
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.log-question__options {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.log-question__option {
  padding: 1px 8px;
  border: 1px solid var(--color-border);
  border-radius: 999px;
  font-size: 11px;
}

.log-result {
  display: grid;
  gap: 4px;
  margin-top: 4px;
  padding: 8px 10px;
  border: 1px solid var(--color-border);
  border-left-width: 3px;
  border-radius: var(--radius-md);
}

.log-result__head {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.log-result__end {
  margin: 0;
}

.log-result--success {
  border-left-color: var(--color-success);
}

.log-result--success .log-kind {
  color: var(--color-success);
}

.log-result--danger {
  border-left-color: var(--color-danger);
}

.log-result--danger .log-kind,
.log-result--danger .log-result__end {
  color: var(--color-danger);
}

.log-result--warning {
  border-left-color: var(--color-warning);
}

.log-result--warning .log-kind,
.log-result--warning .log-result__end {
  color: var(--color-warning);
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

.log-list :deep(*)::highlight(job-log-search),
.log-list::highlight(job-log-search) {
  background: color-mix(in oklab, var(--color-warning) 45%, transparent);
  color: inherit;
}

@media (max-width: 640px) {
  .log-row {
    grid-template-columns: max-content minmax(0, 1fr);
  }

  /* :deep reaches child components' roots, which have no scope attribute of this component. */
  .log-row :deep(.log-message),
  .log-row :deep(.tool-call),
  .log-message-block,
  .log-group__text {
    grid-column: 1 / -1;
  }

  .job-log__search {
    width: 100%;
  }

  .job-log__actions {
    width: 100%;
  }
}
</style>
