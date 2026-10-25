<template>
  <section class="chart-card" :aria-labelledby="titleId">
    <header class="chart-card__header">
      <div class="chart-card__heading">
        <h3 :id="titleId" class="chart-card__title">{{ title }}</h3>
        <p v-if="subtitle" class="chart-card__subtitle">{{ subtitle }}</p>
      </div>
      <div v-if="$slots.table && !empty" class="chart-card__views" role="group" :aria-label="`${title} view`">
        <button
          type="button"
          class="chart-card__view"
          :class="{ active: view === 'chart' }"
          :aria-pressed="view === 'chart'"
          @click="view = 'chart'"
        >
          Chart
        </button>
        <button
          type="button"
          class="chart-card__view"
          :class="{ active: view === 'table' }"
          :aria-pressed="view === 'table'"
          @click="view = 'table'"
        >
          Table
        </button>
      </div>
    </header>
    <p v-if="empty" class="chart-card__empty">{{ emptyText }}</p>
    <div v-else-if="view === 'table' && $slots.table" class="chart-card__table"><slot name="table" /></div>
    <div v-else class="chart-card__body"><slot /></div>
    <footer v-if="$slots.footer" class="chart-card__footer"><slot name="footer" /></footer>
  </section>
</template>

<script setup lang="ts">
import { ref, useId } from 'vue'

withDefaults(
  defineProps<{
    title: string
    subtitle?: string
    empty?: boolean
    emptyText?: string
  }>(),
  { subtitle: '', empty: false, emptyText: 'No data for this selection.' },
)

const titleId = useId()
const view = ref<'chart' | 'table'>('chart')
</script>

<style scoped>
.chart-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
  min-width: 0;
  padding: var(--space-4, 1rem);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg, 0.5rem);
  background: var(--color-surface);
}

.chart-card__header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: var(--space-2, 0.5rem);
  flex-wrap: wrap;
}

.chart-card__heading {
  min-width: 0;
}

.chart-card__title {
  margin: 0;
  font-size: var(--text-md, 1rem);
}

.chart-card__subtitle {
  margin: var(--space-1, 0.25rem) 0 0;
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted);
}

.chart-card__views {
  display: inline-flex;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md, 0.375rem);
  overflow: hidden;
}

.chart-card__view {
  border: 0;
  background: transparent;
  color: var(--color-muted);
  font: inherit;
  font-size: var(--text-xs, 0.75rem);
  padding: 2px var(--space-2, 0.5rem);
  cursor: pointer;
}

.chart-card__view.active {
  background: color-mix(in oklab, var(--color-accent) 14%, transparent);
  color: var(--color-fg);
}

.chart-card__view:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.chart-card__empty {
  margin: 0;
  padding: var(--space-4, 1rem) 0;
  color: var(--color-muted);
  font-size: var(--text-sm, 0.875rem);
}

.chart-card__body,
.chart-card__table {
  min-width: 0;
}

.chart-card__table {
  overflow-x: auto;
}

.chart-card__footer {
  font-size: var(--text-sm, 0.875rem);
}
</style>
