<template>
  <UiButton
    class="reload-button"
    icon-only
    :variant="variant"
    :aria-label="label"
    :title="title || label"
    :disabled="isDisabled"
    v-bind="forwardedAttrs"
    @click="handleClick"
  >
    <span :class="['reload-button__icon', { 'reload-button__icon--spinning': loading }]"><IconGlyph name="refresh" /></span>
  </UiButton>
</template>

<script setup lang="ts">
import { computed, useAttrs } from 'vue';
import IconGlyph from './IconGlyph.vue';
import UiButton from './UiButton.vue';

defineOptions({ inheritAttrs: false })

const props = withDefaults(defineProps<{
  label: string
  title?: string
  loading?: boolean
  disabled?: boolean
  variant?: 'primary' | 'danger' | 'ghost' | ''
}>(), {
  loading: false,
  disabled: false,
  title: undefined,
  variant: '',
})

const emit = defineEmits<{ (e: 'click', event: MouseEvent): void }>()
const attrs = useAttrs()

const forwardedAttrs = computed(() => {
  const rest: Record<string, unknown> = { ...attrs }
  delete rest.title
  delete rest['aria-label']
  return rest
})

const isDisabled = computed(() => props.disabled || props.loading)

function handleClick(event: MouseEvent) {
  if (isDisabled.value) {
    event.preventDefault()
    event.stopPropagation()
    return
  }
  emit('click', event)
}
</script>

<style scoped>
.reload-button__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: transform 0.2s ease;
}

.reload-button__icon--spinning {
  animation: reload-button-spin 0.9s linear infinite;
}

@keyframes reload-button-spin {
  0% {
    transform: rotate(0deg);
  }
  100% {
    transform: rotate(360deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  .reload-button__icon--spinning {
    animation-duration: 1.5s;
  }
}
</style>
