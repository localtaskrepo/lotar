<template>
  <div class="ui-loader" role="status" aria-live="polite">
    <span class="ui-loader__spinner" :class="props.size" aria-hidden="true"></span>
    <span class="ui-loader__label"><slot>Loading…</slot></span>
  </div>
</template>
<script setup lang="ts">
const props = withDefaults(defineProps<{ size?: 'sm' | 'md' | 'lg' }>(), {
  size: 'md',
})
</script>
<style scoped>
.ui-loader {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  color: var(--color-muted);
  font-size: var(--text-sm, 0.875rem);
}

.ui-loader__spinner {
  display: inline-block;
  width: 1rem;
  height: 1rem;
  border-radius: var(--radius-pill);
  border: 2px solid color-mix(in oklab, var(--color-border) 70%, transparent);
  border-top-color: var(--color-fg);
  animation: ui-loader-spin 0.9s linear infinite;
}

.ui-loader__spinner.sm { width: 0.75rem; height: 0.75rem; }
.ui-loader__spinner.lg { width: 1.25rem; height: 1.25rem; }

.ui-loader__label { display: inline-block; }

@keyframes ui-loader-spin {
  to { transform: rotate(360deg); }
}
</style>
