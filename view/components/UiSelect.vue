<template>
  <select class="input ui-select" :value="modelValue" @change="$emit('update:modelValue', ($event.target as HTMLSelectElement).value)">
    <slot />
  </select>
</template>
<script setup lang="ts">
defineProps<{ modelValue?: string }>()
defineEmits<{ (e: 'update:modelValue', v: string): void }>()
</script>
<style scoped>
.ui-select {
  position: relative;
  appearance: none;
  min-height: 32px;
  padding: calc(var(--space-2, 0.5rem) - 4px) calc(var(--space-3, 0.75rem) + 16px) calc(var(--space-2, 0.5rem) - 4px) var(--space-3, 0.75rem);
  background: inherit;
  color: inherit;
  line-height: var(--line-tight, 1.25);
}

.ui-select:focus-visible {
  outline: none;
}

.ui-select option {
  color: var(--color-fg, #0f172a);
  background: var(--color-bg, #ffffff);
}

@media (prefers-color-scheme: dark) {
  .ui-select option {
    color: var(--color-fg, #e5e7eb);
    background: var(--color-bg, #111315);
  }
}
</style>
