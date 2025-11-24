<template>
  <section class="config-group card">
    <header class="config-group__header">
      <div class="config-group__titles">
        <h3>{{ title }}</h3>
        <p v-if="description" class="config-group__description">{{ description }}</p>
      </div>
      <span v-if="source" class="config-group__source provenance" :class="sourceClass">{{ sourceLabel }}</span>
    </header>
    <div class="config-group__content">
      <slot />
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { ConfigSource } from '../api/types';
import { provenanceClass, provenanceLabel } from '../utils/provenance';

const props = defineProps<{
  title: string
  description?: string
  source?: ConfigSource
}>()

const sourceLabel = computed(() => provenanceLabel(props.source))
const sourceClass = computed(() => provenanceClass(props.source))
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
  white-space: nowrap;
}

.config-group__content {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
</style>
