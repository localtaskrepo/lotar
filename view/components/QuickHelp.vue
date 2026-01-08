<template>
  <div v-if="open" class="backdrop" @click.self="$emit('close')">
    <UiCard class="quick-help__card" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts">
      <div class="row" style="justify-content: space-between; align-items: center;">
        <h2 style="margin: 0;">Keyboard shortcuts</h2>
        <UiButton
          variant="ghost"
          icon-only
          type="button"
          aria-label="Close dialog"
          title="Close dialog"
          @click="$emit('close')"
        >
          <IconGlyph name="close" />
        </UiButton>
      </div>
      <ul class="list" style="margin-top: 8px;">
        <li><strong>?</strong> — Toggle this help</li>
        <li><strong>/</strong> — Focus search</li>
  <li><strong>g t</strong> — Go to Tasks</li>
  <li><strong>g b</strong> — Go to Boards</li>
  <li><strong>g i</strong> — Go to Insights</li>
      </ul>
      <small class="muted">More shortcuts coming soon (j/k selection, a add, e edit, etc.).</small>
    </UiCard>
  </div>
</template>
<script setup lang="ts">
import IconGlyph from './IconGlyph.vue';
import UiButton from './UiButton.vue';
import UiCard from './UiCard.vue';

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'close'): void }>()
</script>
<style scoped>
.backdrop{
  position: fixed;
  inset: 0;
  background: var(--color-dialog-overlay);
  display:flex;
  align-items: flex-start;
  justify-content: center;
  padding: 32px 16px;
  z-index: var(--z-modal-high);
}
.quick-help__card{
  width: min(720px, 100%);
  margin-top: clamp(48px, 10vh, 120px);
}
</style>
