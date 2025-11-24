<template>
  <div v-if="open" class="backdrop" @click.self="$emit('close')">
    <div class="card" role="dialog" aria-modal="true" aria-label="Recent activity">
      <div class="card-head">
        <div>
          <h2>Recent activity</h2>
          <p class="muted">Combined from task history and recent commits.</p>
        </div>
        <div class="card-actions row">
          <ReloadButton
            variant="ghost"
            :disabled="loading"
            :loading="loading"
            label="Refresh activity feed"
            title="Refresh activity feed"
            @click="refresh"
          />
          <UiButton
            variant="ghost"
            icon-only
            type="button"
            aria-label="Close activity drawer"
            title="Close activity drawer"
            @click="$emit('close')"
          >
            <IconGlyph name="close" />
          </UiButton>
        </div>
      </div>
      <div v-if="loading" class="muted" style="padding: 12px 0;">Loading activity…</div>
      <div v-else-if="error" class="error">{{ error }}</div>
      <ul v-else class="feed">
        <li v-for="item in feed" :key="item.commit + ':' + item.task_id" class="feed-item">
          <header class="feed-item__header">
            <div class="row" style="gap: 8px; align-items: center;">
              <button class="task-pill" type="button" @click="openTask(item.task_id)">{{ item.task_id }}</button>
              <span class="muted">{{ item.task_title || 'Untitled' }}</span>
            </div>
            <div class="row" style="gap: 12px; align-items: center;">
              <span class="commit-hash" :title="item.commit">{{ item.commit.slice(0, 8) }}</span>
              <span class="muted">{{ formatDate(item.date) }} · {{ item.author }}</span>
            </div>
          </header>
          <p class="muted" style="margin: 4px 0 12px;">{{ item.message }}</p>
          <div class="history">
            <section v-for="historyEntry in item.history" :key="historyEntry.at + (historyEntry.actor || '')" class="history-entry">
              <header class="history-entry__meta">
                <span class="muted">{{ formatDate(historyEntry.at) }}</span>
                <span v-if="historyEntry.actor" class="muted"> · {{ historyEntry.actor }}</span>
              </header>
              <ul class="change-list">
                <li v-for="change in historyEntry.changes" :key="change.field + ':' + (change.new ?? '')" class="change">
                  <span class="change-kind" :class="'change-kind--' + change.kind">{{ formatKind(change.kind) }}</span>
                  <span class="change-label">{{ change.field }}</span>
                  <span v-if="change.old || change.new" class="change-values">
                    <template v-if="change.old && change.new">{{ change.old }} → <strong>{{ change.new }}</strong></template>
                    <template v-else-if="change.new"><strong>{{ change.new }}</strong></template>
                    <template v-else>{{ change.old }}</template>
                  </span>
                </li>
              </ul>
            </section>
          </div>
        </li>
        <li v-if="!feed.length" class="muted">No recorded activity in the selected window.</li>
      </ul>
    </div>
  </div>
</template>
<script setup lang="ts">
import { computed, watch } from 'vue'
import { useActivity } from '../composables/useActivity'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { startOfLocalDay } from '../utils/date'
import IconGlyph from './IconGlyph.vue'
import ReloadButton from './ReloadButton.vue'
import UiButton from './UiButton.vue'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'close'): void }>()

const { feed: sharedFeed, feedLoading, feedError, refreshFeed } = useActivity()
const { openTaskPanel } = useTaskPanelController()

const feed = sharedFeed
const loading = computed(() => feedLoading.value)
const error = computed(() => feedError.value || null)

const WINDOW_DAYS = 30
const MS_PER_DAY = 24 * 60 * 60 * 1000

function nowIso() {
  return new Date().toISOString()
}

function sinceIso() {
  const now = new Date()
  const offset = new Date(now.getTime() - (WINDOW_DAYS - 1) * MS_PER_DAY)
  const start = startOfLocalDay(offset)
  return start.toISOString()
}

async function loadIfNeeded() {
  if (!props.open) return
  if (loading.value) return
  await refreshFeed({ since: sinceIso(), until: nowIso(), limit: 200 })
}

async function refresh() {
  await refreshFeed({ since: sinceIso(), until: nowIso(), limit: 200 })
}

function formatDate(value: string | Date) {
  const date = typeof value === 'string' ? new Date(value) : value
  if (Number.isNaN(date.getTime())) return 'Unknown time'
  return date.toLocaleString()
}

function formatKind(kind: string) {
  switch (kind) {
    case 'created':
      return 'Created'
    case 'status':
      return 'Status'
    case 'assignment':
      return 'Assignment'
    case 'comment':
      return 'Comment'
    case 'tags':
      return 'Tags'
    case 'relationships':
      return 'Relations'
    case 'custom':
      return 'Custom field'
    case 'content':
      return 'Content'
    case 'planning':
      return 'Planning'
    default:
      return kind.charAt(0).toUpperCase() + kind.slice(1)
  }
}

function openTask(taskId: string) {
  openTaskPanel({ taskId })
}

watch(
  () => props.open,
  async (value) => {
    if (value) {
      await loadIfNeeded()
    }
  },
  { immediate: true },
)
</script>
<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  background: color-mix(in oklab, black 40%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 32px 16px;
  overflow-y: hidden;
  z-index: 1000;
}

.card {
  width: 100%;
  max-width: 880px;
  max-height: min(90vh, calc(100vh - 64px));
  margin: 0 auto;
  padding: 20px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 16px;
  box-shadow: 0 12px 34px color-mix(in oklab, black 18%, transparent);
  display: flex;
  flex-direction: column;
  overflow-y: auto;
}

.card-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
  margin-bottom: 16px;
  flex-wrap: wrap;
}

.card-head > * {
  flex: 1 1 240px;
  min-width: 0;
}

.card-actions {
  justify-content: flex-end;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
}

.card-actions .btn {
  flex: 0 0 auto;
}

.feed {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
  overflow-y: auto;
}

.feed-item {
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 16px;
  background: color-mix(in oklab, var(--bg) 92%, white 8%);
}

.feed-item__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.task-pill {
  font-size: 12px;
  font-weight: 600;
  padding: 4px 10px;
  border-radius: 999px;
  border: 1px solid var(--border-strong, var(--border));
  background: color-mix(in oklab, #16a34a 12%, transparent);
  color: #065f46;
  cursor: pointer;
}

.task-pill:hover {
  background: color-mix(in oklab, #16a34a 18%, transparent);
}

.commit-hash {
  font-family: var(--font-mono, ui-monospace);
  font-size: 12px;
  padding: 2px 6px;
  border-radius: 6px;
  background: color-mix(in oklab, #1f2937 12%, transparent);
  color: #1f2937;
}

.history {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.history-entry {
  border-left: 2px solid var(--border);
  padding-left: 12px;
}

.history-entry__meta {
  font-size: 12px;
  margin-bottom: 4px;
}

.change-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.change {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: baseline;
  font-size: 13px;
}

.change-kind {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 999px;
  border: 1px solid var(--border);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.change-kind--created {
  background: color-mix(in oklab, #16a34a 12%, transparent);
  border-color: #16a34a33;
  color: #166534;
}

.change-kind--status {
  background: color-mix(in oklab, #2563eb 12%, transparent);
  border-color: #2563eb33;
  color: #1e3a8a;
}

.change-kind--assignment {
  background: color-mix(in oklab, #f59e0b 12%, transparent);
  border-color: #f59e0b33;
  color: #92400e;
}

.change-kind--comment {
  background: color-mix(in oklab, #0ea5e9 12%, transparent);
  border-color: #0ea5e933;
  color: #155e75;
}

.change-kind--tags {
  background: color-mix(in oklab, #10b981 12%, transparent);
  border-color: #10b98133;
  color: #047857;
}

.change-kind--relationships {
  background: color-mix(in oklab, #a855f7 12%, transparent);
  border-color: #a855f733;
  color: #6b21a8;
}

.change-kind--custom {
  background: color-mix(in oklab, #f97316 12%, transparent);
  border-color: #f9731633;
  color: #9a3412;
}

.change-kind--content {
  background: color-mix(in oklab, #ec4899 12%, transparent);
  border-color: #ec489933;
  color: #9d174d;
}

.change-kind--planning {
  background: color-mix(in oklab, #14b8a6 12%, transparent);
  border-color: #14b8a633;
  color: #0f766e;
}

.change-kind--other {
  background: color-mix(in oklab, #6b7280 12%, transparent);
  border-color: #6b728033;
  color: #374151;
}

.change-label {
  font-weight: 600;
}

.change-values {
  color: var(--muted, #475569);
}

.error {
  color: #b91c1c;
  padding: 12px 0;
}

.muted {
  color: var(--muted, #64748b);
}
</style>
