<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="sprint-analytics__overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Sprint insights"
      @click.self="onClose"
    >
      <UiCard class="sprint-analytics__card">
        <header class="sprint-analytics__header">
          <div class="sprint-analytics__title">
            <h2>
              Sprint insights
              <span v-if="currentSprintLabel" class="sprint-analytics__title-sprint">· {{ currentSprintLabel }}</span>
            </h2>
          </div>
          <div class="sprint-analytics__header-actions">
            <ReloadButton
              variant="ghost"
              :disabled="!selectedSprintId"
              :loading="loading"
              label="Refresh analytics"
              title="Refresh analytics"
              @click="emitRefresh"
            />
            <span v-if="error" class="sprint-analytics__error">{{ error }}</span>
            <UiButton
              class="sprint-analytics__close"
              icon-only
              variant="ghost"
              type="button"
              aria-label="Close dialog"
              @click="onClose"
            >
              <IconGlyph name="close" />
            </UiButton>
          </div>
        </header>

        <nav class="sprint-analytics__tabs" role="tablist">
          <button
            type="button"
            role="tab"
            :aria-selected="activeTab === 'health'"
            :class="['tab', { 'tab--active': activeTab === 'health' }]"
            @click="updateTab('health')"
          >
            Health
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="activeTab === 'burndown'"
            :class="['tab', { 'tab--active': activeTab === 'burndown' }]"
            @click="updateTab('burndown')"
          >
            Burndown
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="activeTab === 'velocity'"
            :class="['tab', { 'tab--active': activeTab === 'velocity' }]"
            @click="updateTab('velocity')"
          >
            Velocity
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="activeTab === 'history'"
            :class="['tab', { 'tab--active': activeTab === 'history' }]"
            @click="updateTab('history')"
          >
            History
          </button>
        </nav>

        <section class="sprint-analytics__panel" role="tabpanel">
          <template v-if="activeTab === 'health'">
            <div class="sprint-analytics__section">
              <SprintHealthWidget :summary="summary" :loading="summaryLoading" :error="summaryError" />
            </div>
          </template>

          <template v-else-if="activeTab === 'burndown'">
            <div class="sprint-analytics__section">
              <div class="sprint-analytics__toggles">
                <UiButton
                  v-for="metric in burndownMetricOptions"
                  :key="metric.value"
                  class="small"
                  variant="ghost"
                  type="button"
                  :class="{ active: metric.value === burndownMetric }"
                  :disabled="!metric.available"
                  @click="updateBurndownMetric(metric.value)"
                >
                  {{ metric.label }}
                </UiButton>
              </div>
              <div v-if="summary && burndown">
                <div v-if="burndownHasSeries" class="sprint-analytics__chart-area">
                  <BurndownChart :series="burndown.series" :metric="burndownMetric" />
                </div>
                <div v-else class="sprint-analytics__chart-area sprint-analytics__chart-placeholder">
                  <p>{{ burndownPlaceholderMessage }}</p>
                </div>
              </div>
              <p v-else class="sprint-analytics__empty">{{ burndownLoadMessage }}</p>
            </div>
          </template>

          <template v-else-if="activeTab === 'velocity'">
            <div class="sprint-analytics__section">
              <div class="sprint-analytics__toggles">
                <UiButton
                  v-for="metric in velocityMetricOptions"
                  :key="metric.value"
                  class="small"
                  variant="ghost"
                  type="button"
                  :class="{ active: metric.value === velocityMetric }"
                  :disabled="!metric.available || (velocityLoading && metric.value === velocityMetric)"
                  @click="updateVelocityMetric(metric.value)"
                >
                  {{ metric.label }}
                </UiButton>
              </div>
              <UiLoader v-if="velocityLoading" size="sm" />
              <span v-else-if="velocityError" class="sprint-analytics__error">{{ velocityError }}</span>
              <div v-else-if="velocityForDisplay" class="sprint-analytics__chart-area">
                <VelocityTrend
                  :response="velocityForDisplay"
                  :metric="velocityMetric"
                  :current-sprint-id="selectedSprintId"
                  :window-size="velocityWindow"
                />
              </div>
              <p v-else class="sprint-analytics__empty">
                We need at least one completed sprint to build a velocity trend. Completed sprints will appear here automatically after refresh.
              </p>
            </div>
          </template>

          <template v-else>
            <div class="sprint-analytics__section">
              <ol v-if="historyEvents.length" class="sprint-analytics__timeline" aria-label="Sprint history">
                <li v-for="event in historyEvents" :key="event.id" class="timeline-item">
                  <span class="timeline-item__dot" :class="`timeline-item__dot--${event.kind}`" aria-hidden="true"></span>
                  <div class="timeline-item__content">
                    <p class="timeline-item__label">
                      {{ event.label }}
                      <span v-if="event.dateLabel" class="timeline-item__date">· {{ event.dateLabel }}</span>
                    </p>
                    <p v-if="event.description" class="timeline-item__description">{{ event.description }}</p>
                  </div>
                </li>
              </ol>
              <p v-else class="sprint-analytics__empty">{{ historyEmptyMessage }}</p>
            </div>
          </template>
        </section>
      </UiCard>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, toRefs, watch } from 'vue'
import type {
    SprintBurndownResponse,
    SprintListItem,
    SprintSummaryReportResponse,
    SprintVelocityEntryPayload,
    SprintVelocityResponse,
} from '../../api/types'
import IconGlyph from '../IconGlyph.vue'
import ReloadButton from '../ReloadButton.vue'
import UiButton from '../UiButton.vue'
import UiCard from '../UiCard.vue'
import UiLoader from '../UiLoader.vue'
import BurndownChart from './BurndownChart.vue'
import SprintHealthWidget from './SprintHealthWidget.vue'
import VelocityTrend from './VelocityTrend.vue'

type SprintMetric = 'tasks' | 'points' | 'hours'

type TabKey = 'burndown' | 'velocity' | 'health' | 'history'

type MetricOption = {
  value: SprintMetric
  label: string
  available?: boolean
}

type HistoryEvent = {
  id: string
  label: string
  dateLabel: string
  description?: string
  kind: 'plan' | 'actual' | 'projected' | 'warning'
}

const props = withDefaults(defineProps<{
  open: boolean
  sprints: SprintListItem[]
  selectedSprintId: number | null
  summary?: SprintSummaryReportResponse
  summaryLoading: boolean
  summaryError: string | null
  burndown?: SprintBurndownResponse
  velocity?: SprintVelocityResponse
  loading: boolean
  error: string | null
  velocityLoading: boolean
  velocityError: string | null
  activeTab: TabKey
  burndownMetric: SprintMetric
  velocityMetric: SprintMetric
  velocityWindowSize?: number
  velocityFocusSprintIds?: number[]
}>(), {
  sprints: () => [],
  loading: false,
  error: null,
  velocityLoading: false,
  velocityError: null,
  summary: undefined,
  summaryLoading: false,
  summaryError: null,
  burndown: undefined,
  velocity: undefined,
  velocityWindowSize: 8,
  velocityFocusSprintIds: () => [],
})

const { summaryLoading, summaryError } = toRefs(props)
const hasSelection = computed(() => !!props.selectedSprintId)

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'update:selectedSprintId', value: number | null): void
  (e: 'update:activeTab', value: TabKey): void
  (e: 'update:burndownMetric', value: SprintMetric): void
  (e: 'update:velocityMetric', value: SprintMetric): void
  (e: 'refresh'): void
}>()

const metricLabels: Record<SprintMetric, string> = {
  tasks: 'Tasks',
  points: 'Story points',
  hours: 'Hours',
}
const currentSprint = computed(() => props.sprints.find((entry) => entry.id === props.selectedSprintId) ?? null)
const currentSprintLabel = computed(() => {
  if (currentSprint.value?.display_name) return currentSprint.value.display_name
  if (props.selectedSprintId) return `Sprint #${props.selectedSprintId}`
  return ''
})
const metricAvailability = computed<Record<SprintMetric, boolean>>(() => ({
  tasks: true,
  points: supportsMetric('points'),
  hours: supportsMetric('hours'),
}))
const velocityWindow = computed(() => Math.max(1, props.velocityWindowSize ?? 1))

const burndownMetricOptions = ref<MetricOption[]>([])
const velocityMetricOptions = ref<MetricOption[]>([])
const normalizedVelocityFocusIds = computed(() =>
  (props.velocityFocusSprintIds ?? []).filter((id): id is number => typeof id === 'number' && Number.isFinite(id)),
)

const filteredVelocity = computed<SprintVelocityResponse | undefined>(() => {
  const payload = props.velocity
  if (!payload) return undefined
  if (!normalizedVelocityFocusIds.value.length) {
    return payload
  }
  const focusSet = new Set(normalizedVelocityFocusIds.value)
  const entries = (payload.entries ?? []).filter(
    (entry): entry is SprintVelocityEntryPayload =>
      Boolean(entry?.summary) && focusSet.has(entry!.summary.id),
  )
  const averageVelocity = computeAverageVelocity(entries)
  const averageCompletion = computeAverageCompletion(entries)
  return {
    ...payload,
    count: entries.length,
    entries,
    average_velocity: averageVelocity,
    average_completion_ratio: averageCompletion,
  }
})

const velocityForDisplay = computed(() => filteredVelocity.value ?? props.velocity)

watch(
  () => props.burndown,
  (payload) => {
    const options: MetricOption[] = ['tasks', 'points', 'hours'].map((value) => ({
      value: value as SprintMetric,
      label: metricLabels[value as SprintMetric],
      available: (() => {
        if (!payload) return false
        if (value === 'tasks') return true
        const totals = value === 'points' ? payload.totals.points : payload.totals.hours
        if (typeof totals === 'number') return true
        return payload.series?.some((point) => {
          const metricValue = value === 'points'
            ? point.remaining_points ?? point.ideal_points
            : point.remaining_hours ?? point.ideal_hours
          return typeof metricValue === 'number' && Number.isFinite(metricValue)
        }) ?? false
      })(),
    }))
    burndownMetricOptions.value = options
  },
  { immediate: true },
)

watch(
  [() => props.velocity, metricAvailability],
  () => {
    velocityMetricOptions.value = ['tasks', 'points', 'hours'].map((value) => ({
      value: value as SprintMetric,
      label: metricLabels[value as SprintMetric],
      available: value === 'tasks' ? true : metricAvailability.value[value as SprintMetric],
    }))
  },
  { immediate: true },
)

watch(
  metricAvailability,
  (availability) => {
    const currentMetric = props.velocityMetric
    if (!availability[currentMetric]) {
      const fallback = availability.points ? 'points' : 'tasks'
      if (fallback !== currentMetric) {
        emit('update:velocityMetric', fallback)
      }
    }
  },
  { immediate: true },
)

const historyEvents = computed<HistoryEvent[]>(() => {
  const summary = props.summary
  if (!summary) return []
  const { lifecycle, sprint } = summary
  type RichEvent = HistoryEvent & { order: number }
  const events: RichEvent[] = []

  const pushEvent = (
    date: string | null | undefined,
    label: string,
    description: string,
    kind: HistoryEvent['kind'],
  ) => {
    if (!date) return
    const timestamp = parseTimestamp(date)
    events.push({
      id: `${kind}-${label}-${date}`,
      label,
      dateLabel: formatFullDate(date),
      description,
      kind,
      order: timestamp ?? Number.MAX_SAFE_INTEGER,
    })
  }

  pushEvent(lifecycle.planned_start, 'Planned start', 'Initial schedule', 'plan')
  pushEvent(lifecycle.actual_start, 'Started', 'Sprint kicked off', 'actual')
  pushEvent(lifecycle.planned_end, 'Planned end', 'Target completion date', 'plan')
  pushEvent(lifecycle.computed_end, 'Projected end', 'Projection based on current progress', 'projected')
  pushEvent(lifecycle.actual_end, 'Completed', 'Sprint closed', 'actual')

  if (Array.isArray(sprint.status_warnings)) {
    sprint.status_warnings.forEach((warning, index) => {
      events.push({
        id: `warning-${warning.code || index}`,
        label: warning.code || 'Warning',
        dateLabel: '',
        description: warning.message,
        kind: 'warning',
        order: Number.MAX_SAFE_INTEGER + index + 1,
      })
    })
  }

  return events
    .sort((a, b) => a.order - b.order)
    .map(({ order, ...rest }) => rest)
})

const historyEmptyMessage = computed(() => {
  if (!hasSelection.value) {
    return 'Open analytics from a sprint to view lifecycle milestones.'
  }
  if (summaryLoading.value) {
    return 'Loading sprint history…'
  }
  const label = currentSprintLabel.value || 'this sprint'
  return `Once ${label} has start or end dates we will build a timeline automatically.`
})

const burndownHasSeries = computed(() => {
  if (!props.burndown) return false
  return burndownSeriesHasMetric(props.burndown.series, props.burndownMetric)
})

const burndownPlaceholderMessage = computed(() => {
  if (!hasSelection.value) {
    return 'Open analytics from a sprint to load burndown data.'
  }
  if (!props.summary?.lifecycle.actual_start) {
    const label = currentSprintLabel.value || 'this sprint'
    return `Burndown will render once ${label} has an actual start date.`
  }
  return 'No scope changes recorded yet. Refresh after additional work has been logged.'
})

const burndownLoadMessage = computed(() => {
  if (!hasSelection.value) {
    return 'Open analytics from a sprint to load burndown data.'
  }
  return 'No burndown data yet. Refresh after this sprint has activity.'
})

function onClose() {
  emit('close')
}

function supportsMetric(metric: Exclude<SprintMetric, 'tasks'>): boolean {
  if (props.summary) {
    if (metric === 'points' && props.summary.metrics.points) {
      return true
    }
    if (metric === 'hours' && props.summary.metrics.hours) {
      return true
    }
  }
  if (currentSprint.value) {
    if (metric === 'points' && typeof currentSprint.value.capacity_points === 'number') {
      return true
    }
    if (metric === 'hours' && typeof currentSprint.value.capacity_hours === 'number') {
      return true
    }
  }
  return false
}

function parseTimestamp(value?: string | null) {
  if (!value) return null
  const date = new Date(value)
  const ts = date.getTime()
  return Number.isFinite(ts) ? ts : null
}

function formatFullDate(value?: string | null) {
  if (!value) return ''
  try {
    return new Date(value).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
  } catch {
    return value
  }
}

function burndownSeriesHasMetric(series: SprintBurndownResponse['series'], metric: SprintMetric) {
  if (!Array.isArray(series) || !series.length) return false
  return series.some((point) => {
    const values = metric === 'points'
      ? [point.remaining_points, point.ideal_points]
      : metric === 'hours'
        ? [point.remaining_hours, point.ideal_hours]
        : [point.remaining_tasks, point.ideal_tasks]
    return values.some((value) => typeof value === 'number' && Number.isFinite(value))
  })
}

function emitRefresh() {
  if (props.selectedSprintId) {
    emit('refresh')
  }
}

function updateTab(tab: TabKey) {
  if (tab !== props.activeTab) {
    emit('update:activeTab', tab)
  }
}

function updateBurndownMetric(metric: SprintMetric) {
  if (!burndownMetricOptions.value.find((option) => option.value === metric)?.available) {
    return
  }
  emit('update:burndownMetric', metric)
}

function updateVelocityMetric(metric: SprintMetric) {
  if (!velocityMetricOptions.value.find((option) => option.value === metric)?.available) {
    return
  }
  if (metric !== props.velocityMetric) {
    emit('update:velocityMetric', metric)
  }
}

function computeAverageVelocity(entries: SprintVelocityEntryPayload[]): number | null {
  const values = entries
    .map((entry) => entry.completed)
    .filter((value): value is number => typeof value === 'number' && Number.isFinite(value))
  if (!values.length) return null
  const total = values.reduce((sum, value) => sum + value, 0)
  return total / values.length
}

function computeAverageCompletion(entries: SprintVelocityEntryPayload[]): number | null {
  const values = entries
    .map((entry) => entry.completion_ratio)
    .filter((value): value is number => typeof value === 'number' && Number.isFinite(value))
  if (!values.length) return null
  const total = values.reduce((sum, value) => sum + value, 0)
  return total / values.length
}

function handleKeydown(event: KeyboardEvent) {
  if (!props.open) return
  if (event.key === 'Escape') {
    event.preventDefault()
    emit('close')
  }
}

onMounted(() => {
  if (typeof window !== 'undefined') {
    window.addEventListener('keydown', handleKeydown)
  }
})

onUnmounted(() => {
  if (typeof window !== 'undefined') {
    window.removeEventListener('keydown', handleKeydown)
  }
})
</script>

<style scoped>
.sprint-analytics__overlay {
  position: fixed;
  inset: 0;
  background: color-mix(in srgb, #0f172a 60%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
  z-index: 1100;
}

.sprint-analytics__card {
  width: min(608px, 100%);
  max-height: min(78vh, 820px);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.sprint-analytics__header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: center;
  flex-wrap: wrap;
}

.sprint-analytics__title {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.sprint-analytics__title-sprint {
  font-size: 1rem;
  font-weight: 500;
  color: var(--color-muted, #64748b);
  margin-left: 8px;
}

.sprint-analytics__header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.sprint-analytics__close {
  align-self: center;
}

.sprint-analytics__error {
  color: var(--color-danger, #ef4444);
  font-size: 0.85rem;
}

.sprint-analytics__tabs {
  display: flex;
  gap: 8px;
  border-bottom: 1px solid var(--color-border, #e2e8f0);
  padding-bottom: 4px;
}

.sprint-analytics__empty {
  font-size: 0.95rem;
  color: var(--color-muted, #64748b);
}

.sprint-analytics__chart-placeholder {
  border: 1px dashed var(--color-border, #e2e8f0);
  border-radius: var(--radius-md, 6px);
  min-height: 220px;
  display: flex;
  align-items: center;
  justify-content: center;
  text-align: center;
  padding: 24px;
  color: var(--color-muted, #64748b);
}

.sprint-analytics__chart-area {
  width: 100%;
  display: flex;
  justify-content: center;
  align-items: center;
}

.sprint-analytics__chart-area > * {
  max-width: 100%;
}

.tab {
  padding: 6px 12px;
  border-radius: var(--radius-sm, 4px);
  border: 1px solid transparent;
  background: transparent;
}

.tab--active {
  border-color: var(--color-border, #e2e8f0);
  background: color-mix(in oklab, var(--color-accent, #6366f1) 12%, transparent);
}

.sprint-analytics__panel {
  overflow-y: auto;
  padding-right: 4px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.sprint-analytics__section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.sprint-analytics__toggles {
  display: flex;
  justify-content: center;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  row-gap: 6px;
  margin-bottom: 4px;
  align-self: center;
}

.sprint-analytics__timeline {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.timeline-item {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}

.timeline-item__dot {
  width: 12px;
  height: 12px;
  border-radius: 50%;
  margin-top: 6px;
  background: var(--color-border, #e2e8f0);
}

.timeline-item__dot--actual { background: color-mix(in oklab, var(--color-accent, #6366f1) 85%, #fff); }
.timeline-item__dot--projected { background: color-mix(in oklab, var(--color-info, #0ea5e9) 70%, #fff); }
.timeline-item__dot--warning { background: color-mix(in oklab, var(--color-danger, #ef4444) 85%, #fff); }
.timeline-item__dot--plan { background: color-mix(in oklab, var(--color-muted, #64748b) 70%, #fff); }

.timeline-item__content {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.timeline-item__label {
  font-weight: 600;
  display: inline-flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: baseline;
  margin: 0;
}

.timeline-item__date {
  font-size: 0.85rem;
  color: var(--color-muted, #64748b);
  font-weight: 500;
}

.timeline-item__description {
  font-size: 0.9rem;
  color: var(--color-muted, #64748b);
  margin: 0;
}

.btn.small {
  padding: 4px 10px;
  font-size: 0.85rem;
}

.btn.small.active {
  background: var(--color-accent, #6366f1);
  color: #fff;
}
</style>
