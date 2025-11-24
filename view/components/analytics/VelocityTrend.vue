<template>
  <div class="velocity-trend" :data-visible-sprint-ids="visibleSprintIdsAttribute">
    <div class="velocity-trend__summary" v-if="response">
      <div>
        <span class="velocity-trend__label">Average velocity</span>
        <strong class="velocity-trend__value">{{ formatNumber(response.average_velocity) }}</strong>
      </div>
      <div>
        <span class="velocity-trend__label">Average completion</span>
        <strong class="velocity-trend__value">{{ formatPercent(response.average_completion_ratio) }}</strong>
      </div>
      <div>
        <span class="velocity-trend__label">Entries</span>
        <strong class="velocity-trend__value">{{ response.count }}</strong>
      </div>
    </div>
    <BarChart v-if="series.length" :series="series" :width="width" :height="height" />
    <p v-else class="velocity-trend__empty">No velocity data available for this metric.</p>
    <p v-if="response?.truncated" class="velocity-trend__hint">Showing the most recent {{ response.count }} sprints.</p>
    <p v-if="response?.skipped_incomplete" class="velocity-trend__hint">Incomplete sprints were skipped. Enable active sprints to include them.</p>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { SprintVelocityEntryPayload, SprintVelocityResponse } from '../../api/types';
import BarChart from '../BarChart.vue';

type SprintMetric = 'tasks' | 'points' | 'hours'

const props = withDefaults(defineProps<{
  response?: SprintVelocityResponse
  metric?: SprintMetric
  width?: number
  height?: number
  currentSprintId?: number | null
  windowSize?: number
}>(), {
  metric: 'points',
  width: 560,
  height: 220,
  currentSprintId: null,
  windowSize: 8,
})

const fallbackLabel = (entry: SprintVelocityEntryPayload) => entry.summary.label || `#${entry.summary.id}`
const chartWindowSize = computed(() => Math.max(1, props.windowSize ?? 1))

const orderedEntries = computed(() => {
  if (!props.response?.entries?.length) return []
  return [...props.response.entries].reverse()
})

const focusIndex = computed(() => {
  if (!props.currentSprintId) return -1
  return orderedEntries.value.findIndex((entry) => entry.summary.id === props.currentSprintId)
})

const visibleEntries = computed(() => {
  if (!orderedEntries.value.length) return []
  const size = Math.min(chartWindowSize.value, orderedEntries.value.length)
  if (focusIndex.value === -1) {
    return orderedEntries.value.slice(-size)
  }
  const end = focusIndex.value + 1
  const start = Math.max(0, end - size)
  return orderedEntries.value.slice(start, end)
})

const series = computed(() => {
  if (!visibleEntries.value.length) return []
  return visibleEntries.value.map((entry) => ({
    key: fallbackLabel(entry),
    breakdown: {
      committed: Math.max(0, entry.committed),
      completed: Math.max(0, entry.completed),
    },
  }))
})

const visibleSprintIdsAttribute = computed(() =>
  visibleEntries.value.map((entry) => entry.summary.id).join(','),
)

function formatNumber(value?: number | null) {
  if (value === null || value === undefined || Number.isNaN(value)) return '—'
  return value.toLocaleString(undefined, { maximumFractionDigits: 1 })
}

function formatPercent(value?: number | null) {
  if (value === null || value === undefined || Number.isNaN(value)) return '—'
  return `${Math.round(value * 100)}%`
}
</script>

<style scoped>
.velocity-trend {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.velocity-trend__summary {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
}

.velocity-trend__label {
  display: block;
  font-size: 0.75rem;
  color: var(--color-muted, #64748b);
  text-transform: uppercase;
}

.velocity-trend__value {
  font-size: 1.1rem;
}

.velocity-trend__hint {
  font-size: 0.85rem;
  color: var(--color-muted, #64748b);
}

.velocity-trend__empty {
  font-size: 0.95rem;
  color: var(--color-muted, #64748b);
}
</style>
