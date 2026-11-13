<template>
  <div class="task-panel__activity-filter" role="group" :aria-label="label">
    <div v-for="group in groups" :key="group.id" class="task-panel__activity-filter-group">
      <span class="task-panel__activity-filter-name">{{ group.name }}</span>
      <button
        v-for="item in group.items"
        :key="item.value"
        type="button"
        class="chip task-panel__activity-filter-chip"
        :class="{ 'is-hidden': item.hidden }"
        :aria-pressed="!item.hidden"
        :title="item.hidden ? `Hidden: click to show ${item.label}` : `Shown: click to hide ${item.label}`"
        @click="$emit('toggle', group.id, item.value)"
      >
        {{ item.label }}
      </button>
    </div>
    <p v-if="hiddenText" class="task-panel__activity-filter-hidden" role="status">
      {{ hiddenText }}
      <button type="button" class="task-panel__activity-filter-show-all" @click="$emit('showAll')">Show all</button>
    </p>
  </div>
</template>

<script setup lang="ts">
export interface ActivityFilterGroup {
  id: string
  name: string
  items: Array<{ value: string; label: string; hidden: boolean }>
}

defineProps<{
  label: string
  groups: ActivityFilterGroup[]
  hiddenText: string
}>()

defineEmits<{
  (e: 'toggle', group: string, value: string): void
  (e: 'showAll'): void
}>()
</script>

<style scoped>
.task-panel__activity-filter {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
}

.task-panel__activity-filter-group {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1, 0.25rem);
}

.task-panel__activity-filter-name {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
  min-width: 3.5rem;
}

.task-panel__activity-filter-chip {
  color: var(--color-fg);
  background: color-mix(in oklab, var(--color-accent) 14%, transparent);
  border-color: color-mix(in oklab, var(--color-accent) 40%, var(--color-border));
}

.task-panel__activity-filter-chip.is-hidden {
  color: var(--color-muted, var(--muted));
  background: transparent;
  border-color: var(--color-border);
  border-style: dashed;
  text-decoration: line-through;
}

.task-panel__activity-filter-hidden {
  margin: 0;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__activity-filter-show-all {
  border: 0;
  padding: 0;
  background: none;
  font: inherit;
  color: var(--color-accent);
  cursor: pointer;
  text-decoration: underline;
}
</style>
