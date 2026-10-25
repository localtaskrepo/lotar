<template>
  <table class="series-table">
    <thead>
      <tr>
        <th scope="col"></th>
        <th v-for="entry in series" :key="entry.label" scope="col" class="numeric">{{ entry.label }}</th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="(label, index) in labels" :key="`${index}-${label}`">
        <th scope="row">{{ label }}</th>
        <td v-for="entry in series" :key="entry.label" class="numeric">{{ entry.values[index] ?? 0 }}</td>
      </tr>
    </tbody>
  </table>
</template>

<script setup lang="ts">
/** Plain table view for a chart, so every value is readable without hover. */
defineProps<{
  labels: string[]
  series: Array<{ label: string; values: number[] }>
}>()
</script>

<style scoped>
.series-table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--text-sm, 0.875rem);
}

.series-table th,
.series-table td {
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
  border-bottom: 1px solid color-mix(in oklab, var(--color-border) 60%, transparent);
  text-align: left;
  white-space: nowrap;
}

.series-table thead th {
  font-weight: 500;
  color: var(--color-muted);
}

.series-table .numeric {
  text-align: right;
  font-variant-numeric: tabular-nums;
}
</style>
