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
        <UiButton icon-only type="button" aria-label="Remove alias" @click="$emit('remove', index)">
          <IconGlyph name="close" />
        </UiButton>
      </div>
      <div class="alias-actions">
        <UiButton type="button" @click="$emit('add')">{{ addLabel }}</UiButton>
        <UiButton v-if="showClear" variant="ghost" type="button" @click="$emit('clear')">{{ clearLabel }}</UiButton>
      </div>
    </div>
    <p v-if="error" class="field-error">{{ error }}</p>
  </section>
</template>

<script setup lang="ts">
import { toRefs } from 'vue'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'
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

</style>
