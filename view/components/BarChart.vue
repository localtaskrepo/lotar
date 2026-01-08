<template>
  <div class="bar-chart" :style="{ width: width + 'px', height: height + 'px' }">
    <canvas ref="canvasEl" :width="width" :height="height"></canvas>
  </div>
</template>

<script setup lang="ts">
import { BarController, BarElement, CategoryScale, Chart, LinearScale, Tooltip } from 'chart.js';
import { computed, nextTick, onBeforeUnmount, onMounted, shallowRef, watch, watchEffect } from 'vue';

Chart.register(BarController, BarElement, CategoryScale, LinearScale, Tooltip)

type BarChartSeriesItem = {
  key: string
  count?: number
  breakdown?: Record<string, number>
}

const props = withDefaults(defineProps<{
  series: BarChartSeriesItem[]
  width?: number
  height?: number
  color?: string
  hoverColor?: string
}>(), { width: 360, height: 160 })

const emit = defineEmits<{ (e:'select', payload: { index: number; value: number }): void }>()

const canvasEl = shallowRef<HTMLCanvasElement | null>(null)
const chart = shallowRef<Chart<'bar'> | null>(null)

const paletteCssVars = [
  '--color-chart-1',
  '--color-chart-2',
  '--color-chart-3',
  '--color-chart-4',
  '--color-chart-5',
  '--color-chart-6',
  '--color-chart-7',
  '--color-chart-8',
]

const labels = computed(() => props.series.map(s => s.key))
const isStacked = computed(() => props.series.some(item => item.breakdown && Object.keys(item.breakdown).some(key => (item.breakdown![key] ?? 0) !== 0)))

const totals = computed(() => props.series.map((item) => {
  if (item.breakdown && Object.keys(item.breakdown).length) {
    return Object.values(item.breakdown).reduce((sum, value) => sum + (typeof value === 'number' ? value : Number(value) || 0), 0)
  }
  const base = typeof item.count === 'number' ? item.count : Number(item.count ?? 0)
  return Number.isFinite(base) ? base : 0
}))

const categories = computed(() => {
  const set = new Set<string>()
  props.series.forEach((item) => {
    Object.entries(item.breakdown ?? {}).forEach(([key, value]) => {
      if ((typeof value === 'number' ? value : Number(value) || 0) !== 0) {
        set.add(key)
      }
    })
  })
  return Array.from(set)
})

function resolveCssVar(name: string, fallback?: string) {
  if (typeof window === 'undefined') return fallback ?? ''
  const value = getComputedStyle(document.documentElement).getPropertyValue(name)?.trim()
  if (value) return value
  if (fallback) return fallback
  return getComputedStyle(document.body).color
}

function formatCategoryLabel(key: string) {
  if (!key) return 'Other'
  return key
    .split(/[_\s-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function datasetColor(index: number) {
  if (!paletteCssVars.length) return resolveCssVar('--color-accent')
  const name = paletteCssVars[index % paletteCssVars.length]
  return resolveCssVar(name, resolveCssVar('--color-accent'))
}

function buildDatasets(baseColor: string, hoverColor: string) {
  if (!isStacked.value) {
    return {
      datasets: [
        {
          data: [...totals.value],
          backgroundColor: baseColor,
          hoverBackgroundColor: hoverColor,
          borderRadius: 6,
          maxBarThickness: 48,
        },
      ],
      legend: false,
    }
  }

  const datasets = categories.value.map((category, index) => {
    const color = datasetColor(index)
    const data = props.series.map((item) => {
      const breakdown = item.breakdown ?? {}
      const raw = breakdown[category]
      const value = typeof raw === 'number' ? raw : Number(raw ?? 0)
      return Number.isFinite(value) ? value : 0
    })
    return {
      label: formatCategoryLabel(category),
      data,
      backgroundColor: color,
      hoverBackgroundColor: color,
      borderRadius: 6,
      maxBarThickness: 48,
      stack: 'stacked',
    }
  })

  return {
    datasets,
    legend: true,
  }
}

function upsertChart(resize = false) {
  if (!canvasEl.value) return

  const baseColor = props.color || resolveCssVar('--color-accent')
  const hoverColor = props.hoverColor || resolveCssVar('--color-accent-strong', baseColor)
  const config = buildDatasets(baseColor, hoverColor)

  if (!chart.value) {
    chart.value = new Chart(canvasEl.value.getContext('2d')!, {
      type: 'bar',
      data: {
        labels: labels.value,
        datasets: config.datasets as any,
      },
      options: {
        responsive: false,
        maintainAspectRatio: false,
        animation: false,
        scales: {
          x: {
            ticks: { color: resolveCssVar('--color-muted') },
            grid: { display: false },
            stacked: isStacked.value,
          },
          y: {
            ticks: { color: resolveCssVar('--color-muted') },
            beginAtZero: true,
            grid: { color: resolveCssVar('--color-border') },
            stacked: isStacked.value,
          },
        },
        plugins: {
          legend: {
            display: config.legend,
            labels: {
              boxWidth: 12,
              boxHeight: 12,
              padding: 10,
            },
          },
          tooltip: { enabled: true },
        },
        onClick: (_event, elements) => {
          if (!elements?.length) return
          const idx = elements[0].index
          const value = totals.value[idx] ?? 0
          if (typeof idx === 'number' && Number.isFinite(value)) {
            emit('select', { index: idx, value })
          }
        },
      },
    })
  } else {
    if (resize) {
      chart.value.resize()
    }
    chart.value.data.labels = [...labels.value]
    chart.value.data.datasets = config.datasets as any
    if (chart.value.options?.scales?.x) {
      chart.value.options.scales.x.stacked = isStacked.value
    }
    if (chart.value.options?.scales?.y) {
      chart.value.options.scales.y.stacked = isStacked.value
    }
    if (chart.value.options?.plugins?.legend) {
      chart.value.options.plugins.legend.display = config.legend
      if (chart.value.options.plugins.legend.labels) {
        chart.value.options.plugins.legend.labels.boxWidth = 12
        chart.value.options.plugins.legend.labels.boxHeight = 12
        chart.value.options.plugins.legend.labels.padding = 10
      }
    }
    chart.value.update()
  }
}

watch([labels, totals, categories, () => props.series], () => upsertChart(), { deep: true, flush: 'post' })

watch(
  () => [props.width, props.height, props.color, props.hoverColor],
  () => {
    nextTick(() => {
      upsertChart(true)
    })
  },
  { flush: 'post' },
)

watchEffect(() => {
  if (canvasEl.value) {
    upsertChart()
  }
})

onMounted(() => {
  upsertChart()
})

onBeforeUnmount(() => {
  chart.value?.destroy()
  chart.value = null
})

const width = computed(() => props.width)
const height = computed(() => props.height)
</script>

<style scoped>
.bar-chart {
  display: inline-block;
}
</style>
