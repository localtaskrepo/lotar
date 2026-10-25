<template>
  <div class="task-table">
    <p v-if="!rows.length" class="task-table__empty muted">{{ emptyText }}</p>
    <table v-else>
      <thead>
        <tr>
          <th scope="col">Task</th>
          <th v-for="column in columns" :key="column.key" scope="col" :class="{ numeric: column.numeric }">
            {{ column.label }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in expanded ? rows : rows.slice(0, limit)" :key="row.id">
          <td class="task-table__task">
            <button type="button" class="task-table__link" @click="emit('open', row.id)">
              <span class="task-table__id">{{ row.id }}</span>
              <span class="task-table__title">{{ row.title }}</span>
            </button>
          </td>
          <td
            v-for="column in columns"
            :key="column.key"
            :class="{ numeric: column.numeric, alert: row.alert?.includes(column.key) }"
          >
            {{ row.cells[column.key] ?? '—' }}
          </td>
        </tr>
      </tbody>
    </table>
    <p v-if="rows.length > limit" class="task-table__more muted">
      {{ expanded ? `Showing all ${rows.length}` : `Showing ${limit} of ${rows.length}` }}
      <button type="button" class="task-table__toggle" :aria-expanded="expanded" @click="expanded = !expanded">
        {{ expanded ? 'Show fewer' : 'Show all' }}
      </button>
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

export interface TaskTableRow {
  id: string
  title: string
  cells: Record<string, string>
  /** Column keys whose value needs attention (shown in the danger ink). */
  alert?: string[]
}

withDefaults(
  defineProps<{
    rows: TaskTableRow[]
    columns: Array<{ key: string; label: string; numeric?: boolean }>
    limit?: number
    emptyText?: string
  }>(),
  { limit: 8, emptyText: 'Nothing here.' },
)

const emit = defineEmits<{ (e: 'open', id: string): void }>()
const expanded = ref(false)
</script>

<style scoped>
.task-table {
  min-width: 0;
  overflow-x: auto;
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--text-sm, 0.875rem);
}

th {
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
  text-align: left;
  font-weight: 500;
  color: var(--color-muted);
  border-bottom: 1px solid var(--color-border);
  white-space: nowrap;
}

td {
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
  border-bottom: 1px solid color-mix(in oklab, var(--color-border) 60%, transparent);
  white-space: nowrap;
}

.numeric {
  text-align: right;
  font-variant-numeric: tabular-nums;
}

td.alert {
  color: var(--color-danger-strong);
  font-weight: 600;
}

.task-table__task {
  width: 100%;
  max-width: 0;
}

.task-table__link {
  display: flex;
  gap: var(--space-2, 0.5rem);
  width: 100%;
  min-width: 0;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.task-table__link:hover .task-table__title {
  text-decoration: underline;
}

.task-table__link:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.task-table__id {
  flex: none;
  color: var(--color-muted);
}

.task-table__title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.task-table__toggle {
  margin-left: var(--space-2, 0.5rem);
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--color-accent-strong, var(--color-accent));
  font: inherit;
  cursor: pointer;
}

.task-table__toggle:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.task-table__empty,
.task-table__more {
  margin: var(--space-2, 0.5rem) 0 0;
  font-size: var(--text-sm, 0.875rem);
}
</style>
