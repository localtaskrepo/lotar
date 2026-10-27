<template>
  <div ref="host" class="trend-chart">
    <ul v-if="series.length > 1" class="trend-chart__legend">
      <li v-for="(entry, index) in series" :key="entry.label">
        <span class="trend-chart__key" :style="{ background: colorOf(index) }" aria-hidden="true"></span>
        {{ entry.label }}
      </li>
    </ul>
    <svg
      class="trend-chart__svg"
      :width="width"
      :height="height"
      :viewBox="`0 0 ${width} ${height}`"
      role="img"
      :aria-label="summary"
      @pointermove="onPointer"
      @pointerleave="active = null"
    >
      <g class="trend-chart__grid">
        <g v-for="tick in ticks" :key="tick">
          <line :x1="plot.left" :x2="width - plot.right" :y1="yOf(tick)" :y2="yOf(tick)" />
          <text :x="plot.left - 6" :y="yOf(tick)" text-anchor="end" dominant-baseline="middle">{{ tick }}</text>
        </g>
      </g>
      <line class="trend-chart__baseline" :x1="plot.left" :x2="width - plot.right" :y1="yOf(0)" :y2="yOf(0)" />
      <path v-if="series.length === 1" class="trend-chart__area" :d="areaPath(0)" :fill="colorOf(0)" />
      <path
        v-for="(entry, index) in series"
        :key="entry.label"
        class="trend-chart__line"
        :d="linePath(index)"
        :stroke="colorOf(index)"
      />
      <g v-for="(entry, index) in series" :key="`end-${entry.label}`">
        <circle
          :cx="xOf(lastIndex)"
          :cy="yOf(entry.values[lastIndex] ?? 0)"
          r="4"
          :fill="colorOf(index)"
          class="trend-chart__dot"
        />
        <text
          v-if="series.length === 1"
          class="trend-chart__endlabel"
          :x="xOf(lastIndex)"
          :y="yOf(entry.values[lastIndex] ?? 0) - 10"
          text-anchor="end"
        >
          {{ entry.values[lastIndex] ?? 0 }}
        </text>
      </g>
      <text
        v-for="(label, index) in labels"
        v-show="index % labelStep === 0"
        :key="`x-${label}-${index}`"
        class="trend-chart__xlabel"
        :x="xOf(index)"
        :y="height - 6"
        text-anchor="middle"
      >
        {{ label }}
      </text>
      <line
        v-if="active !== null"
        class="trend-chart__crosshair"
        :x1="xOf(active)"
        :x2="xOf(active)"
        :y1="plot.top"
        :y2="yOf(0)"
      />
    </svg>
    <div
      v-if="active !== null && labels[active] !== undefined"
      ref="tooltip"
      class="trend-chart__tooltip"
      role="presentation"
      :style="{ left: `${tooltipLeft}px`, top: `${plot.top}px` }"
    >
      <strong class="trend-chart__tooltip-title">{{ labels[active] }}</strong>
      <div v-for="(entry, index) in series" :key="entry.label" class="trend-chart__tooltip-row">
        <span class="trend-chart__key" :style="{ background: colorOf(index) }" aria-hidden="true"></span>
        <strong>{{ entry.values[active] ?? 0 }}</strong>
        <span class="muted">{{ entry.label }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { labelStride, niceTicks, seriesColor, tooltipX, useChartFrame } from './chartFrame'

export interface TrendSeries {
  label: string
  values: number[]
}

const props = withDefaults(
  defineProps<{
    labels: string[]
    series: TrendSeries[]
    height?: number
    summary: string
  }>(),
  { height: 180 },
)

const colorOf = seriesColor

const { host, tooltip, width, tooltipWidth, active } = useChartFrame(() => props.labels, 140)

const plot = { top: 16, right: 12, bottom: 24, left: 40 }
const plotHeight = computed(() => props.height - plot.top - plot.bottom)
const lastIndex = computed(() => Math.max(0, props.labels.length - 1))
const step = computed(() => (width.value - plot.left - plot.right) / Math.max(1, props.labels.length - 1))
const xOf = (index: number) =>
  plot.left + (props.labels.length > 1 ? index * step.value : (width.value - plot.left - plot.right) / 2)

const ticks = computed(() => niceTicks(Math.max(0, ...props.series.flatMap((entry) => entry.values))))
const yMax = computed(() => ticks.value[ticks.value.length - 1] || 1)
const yOf = (value: number) => plot.top + plotHeight.value * (1 - value / yMax.value)

function linePath(index: number) {
  const values = props.series[index]?.values ?? []
  return values.map((value, i) => `${i ? 'L' : 'M'}${xOf(i)},${yOf(value)}`).join(' ')
}
function areaPath(index: number) {
  const values = props.series[index]?.values ?? []
  if (!values.length) return ''
  return `${linePath(index)} L${xOf(values.length - 1)},${yOf(0)} L${xOf(0)},${yOf(0)} Z`
}

const labelStep = computed(() => labelStride(props.labels, step.value))

/** The crosshair snaps to the nearest period. */
function onPointer(event: PointerEvent) {
  const svg = event.currentTarget as SVGSVGElement
  const x = event.clientX - svg.getBoundingClientRect().left
  const index = Math.round((x - plot.left) / Math.max(1, step.value))
  active.value = Math.min(lastIndex.value, Math.max(0, index))
}

const tooltipLeft = computed(() => {
  if (active.value === null) return 0
  const x = xOf(active.value)
  return tooltipX(x + 12, x - 12, tooltipWidth.value, width.value)
})
</script>

<style scoped>
.trend-chart {
  position: relative;
  width: 100%;
  min-width: 0;
}

.trend-chart__legend {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3, 0.75rem);
  margin: 0 0 var(--space-2, 0.5rem);
  padding: 0;
  list-style: none;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
}

.trend-chart__legend li {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1, 0.25rem);
}

.trend-chart__svg {
  display: block;
  overflow: visible;
  touch-action: pan-y;
}

.trend-chart__grid line {
  stroke: var(--viz-grid);
  stroke-width: 1;
}

.trend-chart__grid text,
.trend-chart__xlabel {
  fill: var(--color-muted);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.trend-chart__endlabel {
  fill: var(--color-fg);
  font-size: 12px;
  font-weight: 600;
}

.trend-chart__baseline {
  stroke: var(--viz-axis);
  stroke-width: 1;
}

.trend-chart__area {
  opacity: 0.1;
}

.trend-chart__line {
  fill: none;
  stroke-width: 2;
  stroke-linejoin: round;
  stroke-linecap: round;
}

.trend-chart__dot {
  stroke: var(--color-surface);
  stroke-width: 2;
}

.trend-chart__crosshair {
  stroke: var(--viz-axis);
  stroke-width: 1;
}

.trend-chart__key {
  width: 12px;
  height: 2px;
  border-radius: calc(var(--radius-sm) / 2);
}

.trend-chart__tooltip {
  position: absolute;
  z-index: 2;
  min-width: 120px;
  max-width: 240px;
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md, 0.375rem);
  background: var(--color-bg);
  box-shadow: var(--shadow-md);
  font-size: var(--text-xs, 0.75rem);
  pointer-events: none;
}

.trend-chart__tooltip-title {
  display: block;
  margin-bottom: var(--space-1, 0.25rem);
}

.trend-chart__tooltip-row {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  font-variant-numeric: tabular-nums;
}
</style>
