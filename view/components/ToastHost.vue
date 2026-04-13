<template>
  <TransitionGroup name="toast" tag="div" class="toast-host">
    <div v-for="t in toasts" :key="t.id" class="card toast-card">
      <strong v-if="t.title">{{ t.title }}</strong>
      <div>{{ t.message }}</div>
    </div>
  </TransitionGroup>
</template>
<script setup lang="ts">
import { toasts } from './toast';
</script>
<style scoped>
.toast-host {
  position: fixed;
  right: 12px;
  bottom: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  z-index: 9999;
}
.toast-card {
  background: var(--surface);
}
.toast-enter-active,
.toast-leave-active {
  transition: all 0.3s ease;
}
.toast-enter-from {
  opacity: 0;
  transform: translateX(40px);
}
.toast-leave-to {
  opacity: 0;
  transform: translateX(40px);
}
</style>
