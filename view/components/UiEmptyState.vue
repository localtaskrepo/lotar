<template>
  <div class="empty card col" role="status" aria-live="polite">
    <div class="col" style="gap: 4px;">
      <h3 style="margin:0;">{{ title }}</h3>
      <p v-if="description" class="muted" style="margin:0;">{{ description }}</p>
    </div>
    <div v-if="$slots.actions || primaryLabel || secondaryLabel" class="row" style="gap:8px; margin-top: 8px;">
      <slot name="actions">
        <UiButton v-if="primaryLabel" variant="primary" type="button" @click="$emit('primary')">
          {{ primaryLabel }}
        </UiButton>
        <UiButton v-if="secondaryLabel" type="button" @click="$emit('secondary')">
          {{ secondaryLabel }}
        </UiButton>
      </slot>
    </div>
  </div>
</template>

<script setup lang="ts">
import UiButton from './UiButton.vue';

const props = defineProps<{ title: string; description?: string; primaryLabel?: string; secondaryLabel?: string }>()
const emit = defineEmits<{ (e: 'primary'): void; (e: 'secondary'): void }>()
</script>

<style scoped>
.empty { align-items: flex-start; }
</style>
