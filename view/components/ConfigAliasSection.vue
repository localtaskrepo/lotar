<template>
  <section class="alias-section" :aria-label="title">
    <div class="alias-header">
      <div class="alias-title">
        <h4>{{ title }}</h4>
        <span v-if="sourceLabel" class="provenance" :class="sourceClass">{{ sourceLabel }}</span>
      </div>
      <div class="alias-actions">
        <UiButton v-if="showClear" variant="ghost" type="button" class="alias-action" @click="$emit('clear')">{{
          clearLabel
        }}</UiButton>
        <UiButton variant="ghost" type="button" class="alias-action" @click="$emit('add')">
          <IconGlyph name="plus" aria-hidden="true" />
          <span>{{ addLabel }}</span>
        </UiButton>
      </div>
    </div>
    <p v-if="!entries.length" class="alias-empty">No aliases</p>
    <div v-else class="alias-rows">
      <div class="alias-row alias-row--head" aria-hidden="true">
        <span>Branch token</span>
        <span>Maps to</span>
        <span />
      </div>
      <div v-for="(entry, index) in entries" :key="`alias-${index}`" class="alias-row">
        <UiInput
          v-model="entries[index]!.key"
          :placeholder="keyPlaceholder"
          :aria-label="`${title}: branch token ${index + 1}`"
          @blur="onFieldBlur"
        />
        <UiSelect
          v-if="valueOptions.length"
          v-model="entries[index]!.value"
          :aria-label="`${title}: maps to ${index + 1}`"
          @change="onFieldBlur"
        >
          <option value="" disabled>Choose…</option>
          <option v-for="option in optionsFor(entry.value)" :key="option" :value="option">{{ option }}</option>
        </UiSelect>
        <UiInput
          v-else
          v-model="entries[index]!.value"
          :placeholder="valuePlaceholder"
          :aria-label="`${title}: maps to ${index + 1}`"
          @blur="onFieldBlur"
        />
        <UiButton
          icon-only
          variant="ghost"
          type="button"
          aria-label="Remove alias"
          title="Remove alias"
          @click="$emit('remove', index)"
        >
          <IconGlyph name="close" />
        </UiButton>
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
import UiSelect from './UiSelect.vue'

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
    /** Configured targets (types/statuses/priorities); empty falls back to free text. */
    valueOptions?: string[]
    showClear: boolean
    sourceLabel?: string
    sourceClass?: string
  }>(),
  {
    addLabel: 'Add alias',
    clearLabel: 'Clear overrides',
    valueOptions: () => [],
  },
)

const {
  title,
  error,
  addLabel,
  clearLabel,
  keyPlaceholder,
  valuePlaceholder,
  valueOptions,
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

/** Keep a saved value selectable even if it is no longer configured. */
function optionsFor(current: string) {
  const options = valueOptions.value
  return current && !options.some((option) => option.toLowerCase() === current.toLowerCase())
    ? [current, ...options]
    : options
}

function onFieldBlur() {
  emit('field-blur')
}
</script>

<style scoped>
.alias-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 14px 0;
  border-top: 1px solid var(--color-border);
}

.alias-section:first-of-type {
  border-top: none;
  padding-top: 0;
}

.alias-section:last-of-type {
  padding-bottom: 0;
}

.alias-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 32px;
}

.alias-title {
  display: flex;
  align-items: center;
  gap: 8px;
}

.alias-title h4 {
  margin: 0;
  font-size: var(--text-sm);
  font-weight: 600;
}

.alias-empty {
  margin: 0;
  font-size: 12px;
  color: var(--color-muted);
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

.alias-row--head {
  font-size: 12px;
  font-weight: 600;
  color: var(--color-muted);
}

.alias-row--head span:last-child {
  width: var(--control-height);
}

.alias-row :deep(.input) {
  width: 100%;
  height: var(--control-height);
  box-sizing: border-box;
}

.alias-actions {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}

.alias-action {
  min-height: 32px;
  padding: 0 10px;
  font-size: var(--text-sm);
}
</style>
