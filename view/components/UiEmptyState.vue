<template>
  <div class="empty card col" role="status" aria-live="polite">
    <div class="col" style="gap: 4px;">
      <h3 style="margin:0;">{{ title }}</h3>
      <p v-if="description" class="muted" style="margin:0;">{{ description }}</p>
    </div>
    <div v-if="$slots.actions || primaryLabel || secondaryLabel" class="row" style="gap:8px; margin-top: 8px;">
      <slot name="actions">
        <button v-if="primaryLabel" class="btn primary" @click="$emit('primary')">{{ primaryLabel }}</button>
        <button v-if="secondaryLabel" class="btn" @click="$emit('secondary')">{{ secondaryLabel }}</button>
      </slot>
    </div>
  </div>
</template>

<script setup lang="ts">
const props = defineProps<{ title: string; description?: string; primaryLabel?: string; secondaryLabel?: string }>()
</script>

<style scoped>
.empty { align-items: flex-start; }
</style>
