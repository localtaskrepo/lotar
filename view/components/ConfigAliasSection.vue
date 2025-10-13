<template>
  <section class="alias-section">
    <div class="alias-header">
      <h4>{{ title }}</h4>
      <span v-if="sourceLabel" class="provenance" :class="sourceClass">{{ sourceLabel }}</span>
    </div>
    <div class="alias-rows">
      <div v-for="(entry, index) in entries" :key="`alias-${index}`" class="alias-row">
        <UiInput v-model="entries[index].key" :placeholder="keyPlaceholder" @blur="onFieldBlur" />
        <UiInput v-model="entries[index].value" :placeholder="valuePlaceholder" @blur="onFieldBlur" />
        <button class="btn icon-only" type="button" aria-label="Remove alias" @click="$emit('remove', index)">
          ✕
        </button>
      </div>
      <div class="alias-actions">
        <button class="btn secondary" type="button" @click="$emit('add')">{{ addLabel }}</button>
        <button v-if="showClear" class="btn link" type="button" @click="$emit('clear')">{{ clearLabel }}</button>
      </div>
    </div>
    <p v-if="error" class="field-error">{{ error }}</p>
  </section>
</template>

<script setup lang="ts">
import { toRefs } from 'vue'
import UiInput from './UiInput.vue'

export interface AliasEntry {
  key: string
  value: string
}

const entries = defineModel<AliasEntry[]>('entries', { required: true })

const props = withDefaults(
  defineProps<{
    title: string
    error?: string | null
    addLabel?: string
    clearLabel?: string
    keyPlaceholder: string
    valuePlaceholder: string
    showClear: boolean
    sourceLabel?: string
    sourceClass?: string
  }>(),
  {
    addLabel: 'Add alias',
    clearLabel: 'Clear overrides',
  },
)

const {
  title,
  error,
  addLabel,
  clearLabel,
  keyPlaceholder,
  valuePlaceholder,
  showClear,
  sourceLabel,
  sourceClass,
} = toRefs(props)

const emit = defineEmits<{
  (e: 'add'): void
  (e: 'clear'): void
  (e: 'remove', index: number): void
  (e: 'field-blur'): void
}>()

function onFieldBlur() {
  emit('field-blur')
}
</script>

<style scoped>
.alias-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 0;
  border-top: 1px solid var(--color-border);
}

.alias-section:first-of-type {
  border-top: none;
  padding-top: 0;
}

.alias-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.alias-header h4 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
}

.alias-rows {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.alias-row {
  display: grid;
  grid-template-columns: 1fr 1fr auto;
  gap: 8px;
  align-items: center;
}

.alias-actions {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}

.btn.icon-only {
  padding: 0;
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.provenance {
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.source-project {
  background: rgba(0, 180, 120, 0.25);
  color: #9ef0d0;
}

.source-global {
  background: rgba(0, 120, 255, 0.2);
  color: #8bc0ff;
}

.source-built_in {
  background: rgba(255, 255, 255, 0.15);
  color: rgba(255, 255, 255, 0.85);
}
</style>
