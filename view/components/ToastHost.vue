<template>
  <div style="position: fixed; right: 12px; bottom: 12px; display:flex; flex-direction:column; gap:8px;">
    <div v-for="t in toasts" :key="t.id" class="card" :style="{ background: 'var(--surface)' }">
      <strong v-if="t.title">{{ t.title }}</strong>
      <div>{{ t.message }}</div>
    </div>
  </div>
</template>
<script setup lang="ts">
import { toasts } from './toast';
</script>
<style scoped></style>
