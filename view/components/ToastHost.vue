<template>
  <TransitionGroup name="toast" tag="div" class="toast-host">
    <div v-for="t in toasts" :key="t.id" class="card toast-card" role="status">
      <strong v-if="t.title">{{ t.title }}</strong>
      <div class="toast-card__body">
        <span>{{ t.message }}</span>
        <button v-if="t.action" type="button" class="toast-card__action" @click="runAction(t.id, t.action.run)">
          {{ t.action.label }}
        </button>
      </div>
    </div>
  </TransitionGroup>
</template>
<script setup lang="ts">
import { dismissToast, toasts } from './toast'

function runAction(id: number, run: () => void) {
  dismissToast(id)
  run()
}
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
.toast-card__body {
  display: flex;
  align-items: center;
  gap: 12px;
}
.toast-card__action {
  margin-left: auto;
  padding: 2px 8px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md, 6px);
  background: transparent;
  color: var(--color-accent-strong);
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}
.toast-card__action:hover,
.toast-card__action:focus-visible {
  border-color: var(--color-accent);
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
