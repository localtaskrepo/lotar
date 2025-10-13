<template>
  <section class="config-group card">
    <header class="config-group__header">
      <div class="config-group__titles">
        <h3>{{ title }}</h3>
        <p v-if="description" class="config-group__description">{{ description }}</p>
      </div>
      <span v-if="source" class="config-group__source" :class="`source-${source}`">{{ sourceLabel }}</span>
    </header>
    <div class="config-group__content">
      <slot />
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { ConfigSource } from '../api/types';

const props = defineProps<{
  title: string
  description?: string
  source?: ConfigSource
}>()

const sourceLabel = computed(() => {
  switch (props.source) {
    case 'project':
      return 'Project override'
    case 'global':
      return 'Global default'
    case 'built_in':
      return 'Built-in'
    default:
      return ''
  }
})
</script>

<style scoped>
.config-group {
  padding: 16px 20px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.config-group__header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
}

.config-group__titles h3 {
  margin: 0;
  font-size: 16px;
}

.config-group__description {
  margin: 4px 0 0 0;
  color: var(--muted-foreground, rgba(255, 255, 255, 0.6));
  font-size: 13px;
  line-height: 1.4;
}

.config-group__source {
  align-self: center;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: rgba(255, 255, 255, 0.06);
}

.config-group__source.source-project {
  background: rgba(0, 180, 120, 0.25);
  border-color: rgba(0, 180, 120, 0.45);
  color: #9ef0d0;
}

.config-group__source.source-global {
  background: rgba(0, 120, 255, 0.2);
  border-color: rgba(0, 120, 255, 0.4);
  color: #8bc0ff;
}

.config-group__source.source-built_in {
  background: rgba(255, 255, 255, 0.12);
  border-color: rgba(255, 255, 255, 0.2);
  color: rgba(255, 255, 255, 0.8);
}

.config-group__content {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
</style>
