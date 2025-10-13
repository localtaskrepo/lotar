<template>
  <div v-if="open" class="backdrop" @click.self="$emit('close')">
    <div class="card" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts">
      <div class="row" style="justify-content: space-between; align-items: center;">
        <h2 style="margin: 0;">Keyboard shortcuts</h2>
        <button class="btn" @click="$emit('close')">Close</button>
      </div>
      <ul class="list" style="margin-top: 8px;">
        <li><strong>?</strong> — Toggle this help</li>
        <li><strong>/</strong> — Focus search</li>
  <li><strong>g t</strong> — Go to Tasks</li>
  <li><strong>g b</strong> — Go to Boards</li>
  <li><strong>g i</strong> — Go to Insights</li>
      </ul>
      <small class="muted">More shortcuts coming soon (j/k selection, a add, e edit, etc.).</small>
    </div>
  </div>
</template>
<script setup lang="ts">
const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e:'close'): void }>()
</script>
<style scoped>
.backdrop{
  position: fixed;
  inset: 0;
  background: color-mix(in oklab, black 40%, transparent);
  display:flex;
  align-items: flex-start;
  justify-content: center;
  padding: 32px 16px;
  z-index: 1200;
}
.card{
  width: min(720px, 100%);
  margin-top: clamp(48px, 10vh, 120px);
  padding: 16px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 12px;
  box-shadow: 0 8px 24px color-mix(in oklab, black 18%, transparent);
}
</style>
