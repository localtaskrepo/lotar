<template>
  <ol class="bar-list" :aria-label="label">
    <li v-for="row in visibleRows" :key="row.key" class="bar-list__row">
      <component
        :is="isClickable(row) ? 'button' : 'div'"
        :type="isClickable(row) ? 'button' : undefined"
        class="bar-list__item"
        :class="{ 'bar-list__item--clickable': isClickable(row) }"
        :title="row.hint || undefined"
        @click="isClickable(row) && emit('select', row.key)"
      >
        <span class="bar-list__label">
          <IconGlyph v-if="row.alert" name="flag" class="bar-list__alert" aria-hidden="true" />
          {{ row.label }}
        </span>
        <span class="bar-list__track" aria-hidden="true">
          <span
            class="bar-list__bar"
            :class="{ muted: row.muted }"
            :style="{ width: `${barPercent(row.value)}%` }"
          ></span>
        </span>
        <span class="bar-list__value">{{ row.display ?? row.value }}</span>
      </component>
    </li>
    <li v-if="hiddenCount > 0 || expanded" class="bar-list__more">
      <button type="button" class="bar-list__toggle" :aria-expanded="expanded" @click="expanded = !expanded">
        {{ expanded ? 'Show fewer' : `Show ${hiddenCount} more` }}
      </button>
    </li>
  </ol>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import IconGlyph from '../IconGlyph.vue'

export interface BarRow {
  key: string
  label: string
  value: number
  /** Text shown instead of the raw value (e.g. "3.5d"). */
  display?: string
  hint?: string
  /** Needs attention: shows a flag icon next to the label. */
  alert?: boolean
  /** De-emphasised (e.g. "unassigned", "none"). */
  muted?: boolean
  /** Set false for rows with no matching drill-down. */
  clickable?: boolean
}

const props = withDefaults(
  defineProps<{
    rows: BarRow[]
    label: string
    clickable?: boolean
    limit?: number
  }>(),
  { clickable: false, limit: 8 },
)

const emit = defineEmits<{ (e: 'select', key: string): void }>()

const expanded = ref(false)
const visibleRows = computed(() => (expanded.value ? props.rows : props.rows.slice(0, props.limit)))
const hiddenCount = computed(() => Math.max(0, props.rows.length - props.limit))
const isClickable = (row: BarRow) => props.clickable && row.clickable !== false
const max = computed(() => Math.max(0, ...props.rows.map((row) => row.value)))

function barPercent(value: number) {
  if (max.value <= 0 || value <= 0) return 0
  return Math.max(2, (value / max.value) * 100)
}
</script>

<style scoped>
.bar-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.bar-list__item {
  box-sizing: border-box;
  display: grid;
  grid-template-columns: minmax(6rem, 34%) 1fr minmax(3rem, auto);
  align-items: center;
  gap: var(--space-3, 0.75rem);
  width: 100%;
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
  border: 0;
  border-radius: var(--radius-sm, 0.25rem);
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: var(--text-sm, 0.875rem);
  text-align: left;
}

.bar-list__item--clickable {
  cursor: pointer;
}

.bar-list__item--clickable:hover {
  background: color-mix(in oklab, var(--color-fg) 5%, transparent);
}

.bar-list__item--clickable:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.bar-list__label {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1, 0.25rem);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bar-list__alert {
  flex: none;
  color: var(--color-danger);
}

.bar-list__track {
  display: block;
  height: 12px;
}

.bar-list__bar {
  display: block;
  height: 100%;
  border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  background: var(--viz-1);
}

.bar-list__bar.muted {
  background: var(--viz-muted);
}

.bar-list__value {
  text-align: right;
  font-variant-numeric: tabular-nums;
  color: var(--color-fg);
}

@media (max-width: 640px) {
  .bar-list__item {
    grid-template-columns: 1fr auto;
    row-gap: 2px;
  }

  .bar-list__label {
    grid-column: 1 / -1;
  }
}

.bar-list__more {
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
}

.bar-list__toggle {
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--color-accent-strong, var(--color-accent));
  font: inherit;
  font-size: var(--text-xs, 0.75rem);
  cursor: pointer;
}

.bar-list__toggle:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}
</style>
