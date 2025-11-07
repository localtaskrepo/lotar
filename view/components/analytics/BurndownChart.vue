<template>
  <div class="burndown-chart" :style="{ width: width + 'px', height: height + 'px' }">
    <canvas ref="canvasEl" :width="width" :height="height"></canvas>
  </div>
</template>

<script setup lang="ts">
import { CategoryScale, Chart, Legend, LineController, LineElement, LinearScale, PointElement, Tooltip } from 'chart.js'
import { computed, onBeforeUnmount, onMounted, shallowRef, watch } from 'vue'
import type { SprintBurndownPointPayload } from '../../api/types'

Chart.register(LineController, LineElement, PointElement, CategoryScale, LinearScale, Tooltip, Legend)

type SprintMetric = 'tasks' | 'points' | 'hours'

const props = withDefaults(defineProps<{
  series: SprintBurndownPointPayload[]
  metric?: SprintMetric
  width?: number
  height?: number
}>(), {
  metric: 'tasks',
  width: 560,
  height: 240,
})

const canvasEl = shallowRef<HTMLCanvasElement | null>(null)
const chart = shallowRef<Chart<'line'> | null>(null)

function resolveCssVar(name: string, fallback: string) {
  if (typeof window === 'undefined') return fallback
  const value = getComputedStyle(document.documentElement).getPropertyValue(name)
  return value?.trim() || fallback
}

function formatLabel(value: string) {
  try {
    const date = new Date(value)
    if (Number.isNaN(date.getTime())) return value
    return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
  } catch {
    return value
  }
}

function extractValue(point: SprintBurndownPointPayload, metric: SprintMetric, kind: 'remaining' | 'ideal'): number | null {
  if (metric === 'points') {
    const key = kind === 'remaining' ? point.remaining_points : point.ideal_points
    return typeof key === 'number' ? key : null
  }
  if (metric === 'hours') {
    const key = kind === 'remaining' ? point.remaining_hours : point.ideal_hours
    return typeof key === 'number' ? key : null
  }
  return kind === 'remaining' ? point.remaining_tasks : point.ideal_tasks
}

const labels = computed(() => props.series.map((point) => formatLabel(point.date)))

const remaining = computed(() => props.series.map((point) => extractValue(point, props.metric, 'remaining')))

const ideal = computed(() => props.series.map((point) => extractValue(point, props.metric, 'ideal')))

function upsertChart() {
  if (!canvasEl.value) return
  const accent = resolveCssVar('--color-accent', '#6366f1')
  const accentSoft = resolveCssVar('--color-accent-soft', '#c7d2fe')
  const border = resolveCssVar('--color-border', '#e2e8f0')
  const muted = resolveCssVar('--color-muted', '#64748b')

  const datasets = [
    {
      label: 'Remaining',
      data: remaining.value,
      borderColor: accent,
      backgroundColor: accent,
      pointRadius: 2,
      tension: 0.3,
      spanGaps: true,
    },
    {
      label: 'Ideal',
      data: ideal.value,
      borderColor: accentSoft,
      backgroundColor: 'transparent',
      borderDash: [4, 4],
      pointRadius: 0,
      tension: 0.2,
      spanGaps: true,
    },
  ]

  if (!chart.value) {
    chart.value = new Chart(canvasEl.value.getContext('2d')!, {
      type: 'line',
      data: {
        labels: labels.value,
        datasets: datasets as any,
      },
      options: {
        responsive: false,
        maintainAspectRatio: false,
        animation: false,
        scales: {
          x: {
            ticks: { color: muted },
            grid: { color: border },
          },
          y: {
            ticks: { color: muted },
            grid: { color: border },
            beginAtZero: true,
          },
        },
        plugins: {
          legend: {
            display: true,
            labels: {
              boxWidth: 12,
              boxHeight: 12,
              padding: 12,
              color: muted,
            },
          },
          tooltip: { enabled: true },
        },
      },
    })
  } else {
    chart.value.data.labels = [...labels.value]
    chart.value.data.datasets = datasets as any
    chart.value.update()
  }
}

watch([labels, remaining, ideal, () => props.metric], () => {
  upsertChart()
})

onMounted(() => {
  upsertChart()
})

onBeforeUnmount(() => {
  if (chart.value) {
    chart.value.destroy()
    chart.value = null
  }
})
</script>

<style scoped>
.burndown-chart {
  position: relative;
}
</style>
