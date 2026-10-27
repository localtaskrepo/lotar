<template>
  <div ref="host" class="column-chart">
    <ul v-if="series.length > 1" class="column-chart__legend">
      <li v-for="(entry, index) in series" :key="entry.label">
        <span class="column-chart__swatch" :style="{ background: colorOf(index) }" aria-hidden="true"></span>
        {{ entry.label }}
      </li>
    </ul>
    <svg
      class="column-chart__svg"
      :width="width"
      :height="height"
      :viewBox="`0 0 ${width} ${height}`"
      role="img"
      :aria-label="summary"
    >
      <g class="column-chart__grid">
        <g v-for="tick in ticks" :key="tick">
          <line :x1="plot.left" :x2="width - plot.right" :y1="yOf(tick)" :y2="yOf(tick)" />
          <text :x="plot.left - 6" :y="yOf(tick)" text-anchor="end" dominant-baseline="middle">
            {{ formatTick(tick) }}
          </text>
        </g>
      </g>
      <line class="column-chart__baseline" :x1="plot.left" :x2="width - plot.right" :y1="yOf(0)" :y2="yOf(0)" />
      <g
        v-for="(label, index) in labels"
        :key="`${label}-${index}`"
        :class="{ dim: active !== null && active !== index, partial: partial[index] }"
        class="column-chart__group"
      >
        <path v-for="segment in segmentsAt(index)" :key="segment.key" :d="segment.d" :fill="segment.color" />
        <text
          v-if="index % labelStep === 0"
          class="column-chart__xlabel"
          :x="bandCenter(index)"
          :y="height - 6"
          text-anchor="middle"
        >
          {{ label }}
        </text>
      </g>
      <rect
        v-for="(label, index) in labels"
        :key="`hit-${label}-${index}`"
        class="column-chart__hit"
        :class="{ selectable }"
        :x="plot.left + index * band"
        :y="plot.top"
        :width="band"
        :height="plotHeight"
        :tabindex="selectable ? 0 : undefined"
        :role="selectable ? 'button' : undefined"
        :aria-label="selectable ? describe(index) : undefined"
        @pointerenter="active = index"
        @pointerleave="active = null"
        @focus="active = index"
        @blur="active = null"
        @click="selectable && emit('select', index)"
        @keydown.enter.prevent="selectable && emit('select', index)"
      />
    </svg>
    <div
      v-if="active !== null && labels[active] !== undefined"
      ref="tooltip"
      class="column-chart__tooltip"
      role="presentation"
      :style="{ left: `${tooltipLeft}px`, top: `${plot.top}px` }"
    >
      <strong class="column-chart__tooltip-title">
        {{ labels[active] }}<span v-if="partial[active]" class="muted"> (partial)</span>
      </strong>
      <div v-for="(entry, s) in series" :key="entry.label" class="column-chart__tooltip-row">
        <span class="column-chart__key" :style="{ background: colorOf(s) }" aria-hidden="true"></span>
        <strong>{{ format(entry.values[active] ?? 0) }}</strong>
        <span class="muted">{{ entry.label }}</span>
      </div>
      <div v-if="stacked && series.length > 1" class="column-chart__tooltip-row column-chart__tooltip-total">
        <strong>{{ format(totalAt(active)) }}</strong>
        <span class="muted">Total</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { labelStride, niceTicks, seriesColor, tooltipX, useChartFrame } from './chartFrame'

export interface ColumnSeries {
  label: string
  values: number[]
  /** Fold-in series such as "Other": drawn in the neutral colour. */
  muted?: boolean
}

const props = withDefaults(
  defineProps<{
    labels: string[]
    series: ColumnSeries[]
    height?: number
    /** Formats values in ticks, tooltips and accessible labels. */
    format?: (value: number) => string
    /** Accessible summary of what the chart shows. */
    summary: string
    /** Counts: keep ticks on whole numbers. */
    integer?: boolean
    /** Stack series into one column per label instead of grouping them. */
    stacked?: boolean
    /** Per label: only part of the period is inside the selected range. */
    partial?: boolean[]
    /** Columns are clickable (and keyboard focusable). */
    selectable?: boolean
  }>(),
  {
    height: 200,
    integer: true,
    stacked: false,
    partial: () => [],
    selectable: false,
    format: (value: number) => String(Math.round(value * 10) / 10),
  },
)

const emit = defineEmits<{ (e: 'select', index: number): void }>()

const colorOf = (index: number) => (props.series[index]?.muted ? 'var(--viz-muted)' : seriesColor(index))

const { host, tooltip, width, tooltipWidth, active } = useChartFrame(() => props.labels, 160)

const plot = { top: 8, right: 8, bottom: 24, left: 40 }
const plotHeight = computed(() => props.height - plot.top - plot.bottom)
const band = computed(() => (width.value - plot.left - plot.right) / Math.max(1, props.labels.length))

function totalAt(index: number) {
  return props.series.reduce((sum, entry) => sum + (entry.values[index] ?? 0), 0)
}

const ticks = computed(() =>
  niceTicks(
    props.stacked
      ? Math.max(0, ...props.labels.map((_, index) => totalAt(index)))
      : Math.max(0, ...props.series.flatMap((entry) => entry.values)),
    props.integer,
  ),
)
const yMax = computed(() => ticks.value[ticks.value.length - 1] || 1)
const yOf = (value: number) => plot.top + plotHeight.value * (1 - value / yMax.value)

const columns = computed(() => (props.stacked ? 1 : Math.max(1, props.series.length)))
const barWidth = computed(() => {
  const room = band.value * 0.72 - 2 * (columns.value - 1)
  return Math.max(2, Math.min(24, room / columns.value))
})

function bandCenter(index: number) {
  return plot.left + band.value * (index + 0.5)
}

/** Rectangle with an optional 4px rounded top (the data end). */
function shape(x0: number, x1: number, top: number, bottom: number, rounded: boolean) {
  if (bottom - top <= 0) return ''
  const r = rounded ? Math.min(4, (x1 - x0) / 2, bottom - top) : 0
  return `M${x0},${bottom} V${top + r} Q${x0},${top} ${x0 + r},${top} H${x1 - r} Q${x1},${top} ${x1},${top + r} V${bottom} Z`
}

function segmentsAt(index: number) {
  const out: Array<{ key: string; d: string; color: string }> = []
  if (props.stacked) {
    const x0 = bandCenter(index) - barWidth.value / 2
    const x1 = x0 + barWidth.value
    let lastNonZero = -1
    props.series.forEach((entry, s) => {
      if ((entry.values[index] ?? 0) > 0) lastNonZero = s
    })
    let base = 0
    props.series.forEach((entry, s) => {
      const value = entry.values[index] ?? 0
      if (value <= 0) return
      const bottom = yOf(base)
      base += value
      // 2px surface gap between stacked segments.
      const top = yOf(base) + (s === lastNonZero ? 0 : 2)
      out.push({ key: entry.label, d: shape(x0, x1, top, bottom, s === lastNonZero), color: colorOf(s) })
    })
    return out
  }
  const groupWidth = columns.value * barWidth.value + 2 * (columns.value - 1)
  props.series.forEach((entry, s) => {
    const value = entry.values[index] ?? 0
    if (value <= 0) return
    const x0 = bandCenter(index) - groupWidth / 2 + s * (barWidth.value + 2)
    out.push({ key: entry.label, d: shape(x0, x0 + barWidth.value, yOf(value), yOf(0), true), color: colorOf(s) })
  })
  return out
}

const labelStep = computed(() => labelStride(props.labels, band.value))

/** Right of the column, or left when it would overflow the chart. */
const tooltipLeft = computed(() => {
  if (active.value === null) return 0
  const center = bandCenter(active.value)
  return tooltipX(center + band.value / 2 + 4, center - band.value / 2 - 4, tooltipWidth.value, width.value)
})

function formatTick(value: number) {
  return value >= 1000 ? `${Math.round(value / 100) / 10}K` : props.format(value)
}

function describe(index: number) {
  const values = props.series.map((entry) => `${entry.label} ${props.format(entry.values[index] ?? 0)}`)
  return `${props.labels[index]}${props.partial[index] ? ' (partial)' : ''}: ${values.join(', ')}`
}

const format = (value: number) => props.format(value)
</script>

<style scoped>
.column-chart {
  position: relative;
  width: 100%;
  min-width: 0;
}

.column-chart__legend {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3, 0.75rem);
  margin: 0 0 var(--space-2, 0.5rem);
  padding: 0;
  list-style: none;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
}

.column-chart__legend li {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1, 0.25rem);
}

.column-chart__swatch {
  width: 10px;
  height: 10px;
  border-radius: 2px;
}

.column-chart__svg {
  display: block;
  overflow: visible;
}

.column-chart__grid line {
  stroke: var(--viz-grid);
  stroke-width: 1;
}

.column-chart__grid text,
.column-chart__xlabel {
  fill: var(--color-muted);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.column-chart__baseline {
  stroke: var(--viz-axis);
  stroke-width: 1;
}

.column-chart__group {
  transition: opacity 120ms ease;
}

.column-chart__group.partial path {
  opacity: 0.5;
}

.column-chart__group.dim {
  opacity: 0.45;
}

.column-chart__hit {
  fill: transparent;
  outline: none;
}

.column-chart__hit.selectable {
  cursor: pointer;
}

.column-chart__hit:focus-visible {
  stroke: var(--color-accent);
  stroke-width: 1.5;
}

.column-chart__tooltip {
  position: absolute;
  z-index: 2;
  min-width: 140px;
  max-width: 260px;
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md, 0.375rem);
  background: var(--color-bg);
  box-shadow: var(--shadow-md);
  font-size: var(--text-xs, 0.75rem);
  pointer-events: none;
}

.column-chart__tooltip-title {
  display: block;
  margin-bottom: var(--space-1, 0.25rem);
  font-weight: 600;
}

.column-chart__tooltip-row {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  font-variant-numeric: tabular-nums;
}

.column-chart__tooltip-total {
  margin-top: var(--space-1, 0.25rem);
  padding-top: var(--space-1, 0.25rem);
  padding-left: 20px;
  border-top: 1px solid var(--color-border);
}

.column-chart__key {
  width: 12px;
  height: 2px;
  border-radius: 1px;
}
</style>
