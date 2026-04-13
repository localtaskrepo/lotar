<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="ui-modal__overlay"
      role="dialog"
      aria-modal="true"
      :aria-label="ariaLabel"
      @click.self="$emit('close')"
    >
      <UiCard class="ui-modal__card" :class="sizeClass">
        <slot />
      </UiCard>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import UiCard from './UiCard.vue';

const props = defineProps<{
  open: boolean
  ariaLabel?: string
  size?: 'sm' | 'md' | 'lg'
}>()

defineEmits<{
  close: []
}>()

const sizeClass = computed(() => {
  switch (props.size) {
    case 'sm': return 'ui-modal__card--sm'
    case 'lg': return 'ui-modal__card--lg'
    default: return 'ui-modal__card--md'
  }
})
</script>

<style scoped>
.ui-modal__overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: var(--color-dialog-overlay);
  z-index: var(--z-modal);
}

.ui-modal__card {
  max-height: calc(100vh - 48px);
  overflow-y: auto;
  box-shadow: var(--shadow-dialog);
}

.ui-modal__card--sm {
  width: min(400px, 100%);
}

.ui-modal__card--md {
  width: min(520px, 100%);
}

.ui-modal__card--lg {
  width: min(720px, 100%);
  padding: var(--space-5, 2rem);
}
</style>
