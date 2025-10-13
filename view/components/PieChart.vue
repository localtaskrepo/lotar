<template>
  <div class="pie-chart" :style="{ width: size + 'px', height: size + 'px' }">
    <canvas ref="canvasEl" :width="size" :height="size"></canvas>
  </div>
  <div class="legend" v-if="legend && legendItems.length">
    <div
      class="legend-item"
      v-for="(item, index) in legendItems"
      :key="item.label + index"
      @click="emitSlice(index)"
    >
      <span class="swatch" :style="{ background: item.color }"></span>
      <span>{{ item.label }}</span>
      <span class="muted">{{ item.value }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ArcElement, Chart, Legend as ChartLegend, DoughnutController, Tooltip } from 'chart.js';
import { computed, onBeforeUnmount, onMounted, shallowRef, watch } from 'vue';

Chart.register(DoughnutController, ArcElement, Tooltip, ChartLegend)

const props = withDefaults(defineProps<{
  data?: Array<{ label: string; value: number; color?: string }>
  series?: Array<{ key: string; count: number; color?: string }>
  size?: number
  thickness?: number
  withHole?: boolean
  legend?: boolean
  colors?: string[]
}>(), { size: 160, thickness: 18, withHole: true, legend: true })

const emit = defineEmits<{ (e:'select', payload: unknown): void }>()

const canvasEl = shallowRef<HTMLCanvasElement | null>(null)
const chart = shallowRef<Chart<'doughnut'> | null>(null)

const defaultPalette = ['#6366f1', '#a855f7', '#22d3ee', '#f59e0b', '#f97316', '#10b981', '#ef4444', '#f472b6']
const palette = computed(() => (props.colors?.length ? props.colors : defaultPalette))

type Slice = { label: string; value: number; color?: string; raw: unknown }

const normalizedData = computed<Slice[]>(() => {
  if (props.data?.length) {
    return props.data.map(item => ({ label: item.label, value: item.value, color: item.color, raw: item }))
  }
  if (props.series?.length) {
    return props.series.map(item => ({ label: item.key, value: item.count, color: item.color, raw: item }))
  }
  return []
})

const slices = computed(() => normalizedData.value.map((item, index) => {
  const color = item.color ?? palette.value[index % palette.value.length] ?? resolveCssVar('--color-accent', '#6366f1')
  return { ...item, color }
}))

const labels = computed(() => slices.value.map(item => item.label))
const values = computed(() => slices.value.map(item => item.value))
const backgroundColors = computed(() => slices.value.map(item => item.color))
const borderWidth = computed(() => props.withHole ? Math.max(1, Math.floor(props.thickness / 8)) : 0)
const cutoutValue = computed(() => {
  if (!props.withHole) return '0%'
  const diameter = props.size || 0
  if (diameter <= 0) return '65%'
  const innerDiameter = Math.max(0, diameter - props.thickness * 2)
  const percent = diameter ? Math.min(95, Math.max(0, (innerDiameter / diameter) * 100)) : 0
  return `${percent}%`
})

function resolveCssVar(name: string, fallback: string) {
  if (typeof window === 'undefined') return fallback
  const value = getComputedStyle(document.documentElement).getPropertyValue(name)
  return value?.trim() || fallback
}

function upsertChart() {
  if (!canvasEl.value) return
  const borderColor = resolveCssVar('--color-surface', '#0f172a')
  if (!chart.value) {
    chart.value = new Chart(canvasEl.value.getContext('2d')!, {
      type: 'doughnut',
      data: {
        labels: labels.value,
        datasets: [{
          data: values.value,
          backgroundColor: backgroundColors.value,
          borderColor,
          borderWidth: borderWidth.value,
        }],
      },
      options: {
        responsive: false,
        maintainAspectRatio: false,
        animation: false,
        cutout: cutoutValue.value,
        plugins: {
          legend: { display: false },
          tooltip: { enabled: true },
        },
        onClick: (_event, elements) => {
          if (!elements?.length) return
          const idx = elements[0].index
          emitSlice(idx)
        },
      },
    })
  } else {
    const dataset = chart.value.data.datasets[0]
    dataset.data = [...values.value]
    dataset.backgroundColor = [...backgroundColors.value]
    dataset.borderWidth = borderWidth.value
    chart.value.data.labels = [...labels.value]
    chart.value.options.cutout = cutoutValue.value
    chart.value.update()
  }
}

watch([labels, values, backgroundColors, cutoutValue, borderWidth], upsertChart, { deep: true })

watch(() => props.size, newSize => {
  if (!canvasEl.value) return
  if (newSize) {
    canvasEl.value.width = newSize
    canvasEl.value.height = newSize
  }
  chart.value?.resize()
})

function emitSlice(index: number) {
  const slice = slices.value[index]
  if (!slice) return
  emit('select', slice.raw ?? { label: slice.label, value: slice.value })
}

onMounted(() => {
  upsertChart()
})

onBeforeUnmount(() => {
  chart.value?.destroy()
  chart.value = null
})

const size = computed(() => props.size)
const legend = computed(() => props.legend)
const legendItems = computed(() => slices.value)
</script>

<style scoped>
.pie-chart {
  display: inline-block;
}

.legend {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.legend-item {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}

.swatch {
  width: 12px;
  height: 12px;
  border-radius: 999px;
  background: var(--color-accent, #6366f1);
}

.legend-item .muted {
  margin-left: auto;
  color: var(--color-muted, #94a3b8);
}
</style>
