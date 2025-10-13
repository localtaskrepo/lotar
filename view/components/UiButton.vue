<template>
  <button class="btn" :class="classes" type="button" v-bind="$attrs"><slot /></button>
</template>
<script setup lang="ts">
import { computed } from 'vue';

const props = withDefaults(defineProps<{ variant?: 'primary' | 'danger' | 'ghost' | '' }>(), {
  variant: '',
})

const classes = computed(() => ({
  primary: props.variant === 'primary',
  danger: props.variant === 'danger',
  ghost: props.variant === 'ghost',
}))
</script>
<style scoped>
</style>
