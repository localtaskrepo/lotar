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
          <div>
            <h2>Sprint insights</h2>
            <p class="muted">Deep dive into progress, burndown, and velocity without leaving the board.</p>
          </div>
          <button class="btn ghost" type="button" @click="onClose">Close</button>
        </header>

        <div class="sprint-analytics__controls">
          <label class="sprint-analytics__select">
            <span class="muted">Sprint</span>
            <UiSelect v-model="localSprintId">
              <option value="">Select sprint…</option>
              <option v-for="item in sprints" :key="item.id" :value="String(item.id)">
                {{ item.display_name }}
              </option>
            </UiSelect>
          </label>
          <button class="btn" type="button" :disabled="!selectedSprintId" @click="emitRefresh">Refresh</button>
          <UiLoader v-if="loading" size="sm" />
          <span v-if="error" class="sprint-analytics__error">{{ error }}</span>
        </div>

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
        </nav>

        <section class="sprint-analytics__panel" role="tabpanel">
          <template v-if="activeTab === 'health'">
            <SprintHealthWidget :summary="summary" :loading="summaryLoading" :error="summaryError" />
          </template>

          <template v-else-if="activeTab === 'burndown'">
            <div class="sprint-analytics__section">
              <header class="sprint-analytics__section-header">
                <h3>Burndown</h3>
                <div class="sprint-analytics__toggles">
                  <button
                    v-for="metric in burndownMetricOptions"
                    :key="metric.value"
                    type="button"
                    class="btn ghost small"
                    :class="{ active: metric.value === burndownMetric }"
                    :disabled="!metric.available"
                    @click="updateBurndownMetric(metric.value)"
                  >
                    {{ metric.label }}
                  </button>
                </div>
              </header>
              <div v-if="summary && burndown">
                <BurndownChart :series="burndown.series" :metric="burndownMetric" />
              </div>
              <p v-else class="muted">Select a sprint to load burndown data.</p>
            </div>
          </template>

          <template v-else>
            <div class="sprint-analytics__section">
              <header class="sprint-analytics__section-header">
                <h3>Velocity</h3>
                <div class="sprint-analytics__toggles">
                  <button
                    v-for="metric in velocityMetricOptions"
                    :key="metric.value"
                    type="button"
                    class="btn ghost small"
                    :class="{ active: metric.value === velocityMetric }"
                    :disabled="velocityLoading && metric.value === velocityMetric"
                    @click="updateVelocityMetric(metric.value)"
                  >
                    {{ metric.label }}
                  </button>
                </div>
              </header>
              <UiLoader v-if="velocityLoading" size="sm" />
              <span v-else-if="velocityError" class="sprint-analytics__error">{{ velocityError }}</span>
              <VelocityTrend v-else :response="velocity" :metric="velocityMetric" />
            </div>
          </template>
        </section>
      </UiCard>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, toRefs, watch } from 'vue'
import type {
    SprintBurndownResponse,
    SprintListItem,
    SprintSummaryReportResponse,
    SprintVelocityResponse,
} from '../../api/types'
import UiCard from '../UiCard.vue'
import UiLoader from '../UiLoader.vue'
import UiSelect from '../UiSelect.vue'
import BurndownChart from './BurndownChart.vue'
import SprintHealthWidget from './SprintHealthWidget.vue'
import VelocityTrend from './VelocityTrend.vue'

type SprintMetric = 'tasks' | 'points' | 'hours'

type TabKey = 'health' | 'burndown' | 'velocity'

type MetricOption = {
  value: SprintMetric
  label: string
  available?: boolean
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
})

const { summaryLoading, summaryError } = toRefs(props)

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

const localSprintId = ref(props.selectedSprintId ? String(props.selectedSprintId) : '')

watch(
  () => props.selectedSprintId,
  (next) => {
    const normalized = next ? String(next) : ''
    if (localSprintId.value !== normalized) {
      localSprintId.value = normalized
    }
  },
)

watch(localSprintId, (value) => {
  const next = value ? Number(value) : null
  if (next !== props.selectedSprintId) {
    emit('update:selectedSprintId', next)
  }
})

const burndownMetricOptions = ref<MetricOption[]>([])
const velocityMetricOptions = ref<MetricOption[]>([])

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
  () => props.velocity,
  () => {
    velocityMetricOptions.value = ['tasks', 'points', 'hours'].map((value) => ({
      value: value as SprintMetric,
      label: metricLabels[value as SprintMetric],
      available: true,
    }))
  },
  { immediate: true },
)

function onClose() {
  emit('close')
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
  if (metric !== props.velocityMetric) {
    emit('update:velocityMetric', metric)
  }
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
  padding: 24px;
  z-index: 1100;
}

.sprint-analytics__card {
  width: min(920px, 100%);
  max-height: min(90vh, 960px);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.sprint-analytics__header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
}

.sprint-analytics__controls {
  display: flex;
  align-items: flex-end;
  gap: 12px;
  flex-wrap: wrap;
}

.sprint-analytics__select {
  display: flex;
  flex-direction: column;
  gap: 4px;
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
  gap: 16px;
}

.sprint-analytics__section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.sprint-analytics__section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.sprint-analytics__toggles {
  display: flex;
  gap: 8px;
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
