<template>
  <input
    ref="inputRef"
    class="input"
    v-bind="$attrs"
    :value="modelValue"
    @input="$emit('update:modelValue', ($event.target as HTMLInputElement).value)"
  />
</template>
<script setup lang="ts">
import { ref } from 'vue';

defineProps<{ modelValue?: string }>()
defineEmits<{ (e: 'update:modelValue', v: string): void }>()

const inputRef = ref<HTMLInputElement | null>(null)

function focus() {
  inputRef.value?.focus()
}

defineExpose({ focus })
</script>
<style scoped>
</style>
