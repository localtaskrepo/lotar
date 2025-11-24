<template>
  <div class="chip-field" :class="{ 'chip-field--disabled': disabled }">
    <div
      class="chip-field__control"
      :class="{
        'chip-field__control--empty': !values.length,
        'chip-field__control--open': composerOpen,
      }"
    >
      <span
        v-for="(value, index) in values"
        :key="`${value}-${index}`"
        :class="chipWrapperClass"
      >
        <slot name="chip" :value="value" :index="index">
          <span class="chip-field__chip-label">{{ value }}</span>
        </slot>
        <button
          type="button"
          class="chip-field__chip-remove"
          :aria-label="`Remove ${value}`"
          :title="`Remove ${value}`"
          :disabled="disabled"
          v-if="props.removable"
          @click="removeValue(index)"
        >
          <IconGlyph name="close" aria-hidden="true" />
        </button>
      </span>

      <span v-if="!values.length" class="chip-field__empty">{{ emptyLabel }}</span>

      <button
        type="button"
        class="chip-field__add"
        :aria-expanded="composerOpen"
        :aria-label="addAriaLabel"
        :title="addTooltip"
        :disabled="disabled"
        @click.stop="handleAddClick"
      >
        <IconGlyph name="plus" aria-hidden="true" />
        <span class="chip-field__add-label">{{ addLabel }}</span>
      </button>
    </div>

    <slot
      v-if="useCustomComposer"
      name="composer"
      :is-open="composerOpen"
      :close="closeComposer"
      :open="openComposer"
      :add="commit"
      :values="values"
    />

    <Transition name="chip-field-composer">
      <div v-if="!useCustomComposer && composerOpen" class="chip-field__composer" @keyup.esc.stop.prevent="closeComposer">
        <label class="chip-field__composer-label">
          <span class="muted">{{ composerLabel }}</span>
          <UiInput
            ref="inputRef"
            v-model="draft"
            :maxlength="maxLength"
            :placeholder="placeholder"
            @keyup.enter.prevent="commitDraft"
          />
        </label>
        <div class="chip-field__composer-actions">
          <UiButton variant="primary" type="button" :disabled="!canCommit" @click="commitDraft">
            {{ composerConfirmLabel }}
          </UiButton>
          <UiButton variant="ghost" type="button" @click="closeComposer">
            Cancel
          </UiButton>
        </div>
        <div v-if="suggestionsAvailable" class="chip-field__suggestions">
          <p class="chip-field__suggestions-label">Suggestions</p>
          <div class="chip-field__suggestion-list">
            <button
              v-for="suggestion in filteredSuggestions"
              :key="suggestion"
              type="button"
              class="chip-field__suggestion"
              @click="applySuggestion(suggestion)"
            >
              {{ suggestion }}
            </button>
            <span v-if="!filteredSuggestions.length" class="chip-field__suggestion-empty">No matches</span>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, useSlots } from 'vue'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'
import UiInput from './UiInput.vue'

type ChipClassValue = string | Record<string, boolean> | Array<string | Record<string, boolean>>

const props = withDefaults(
  defineProps<{
    modelValue: string[]
    placeholder?: string
    emptyLabel?: string
    addLabel?: string
    composerLabel?: string
    composerConfirmLabel?: string
    suggestions?: string[]
    maxLength?: number
    allowDuplicates?: boolean
    disabled?: boolean
    normalize?: (value: string) => string
    addBehavior?: 'composer' | 'external'
    removable?: boolean
    chipClass?: ChipClassValue | null
  }>(),
  {
    modelValue: () => [],
    placeholder: 'Add value',
    emptyLabel: 'No values yet.',
    addLabel: 'Add',
    composerLabel: 'Value',
    composerConfirmLabel: 'Add value',
    suggestions: () => [],
    maxLength: 100,
    allowDuplicates: false,
    disabled: false,
    addBehavior: 'composer',
    removable: true,
  },
)

const emit = defineEmits<{
  (e: 'update:modelValue', value: string[]): void
  (e: 'add-click'): void
  (e: 'composer-open'): void
  (e: 'composer-close'): void
}>()

const slots = useSlots()
const composerOpen = ref(false)
const draft = ref('')
const inputRef = ref<{ focus: () => void } | null>(null)

const values = computed(() => props.modelValue ?? [])
const addAriaLabel = computed(() => props.addLabel || 'Add value')
const addTooltip = computed(() => props.addLabel || 'Add value')
const useCustomComposer = computed(() => !!slots.composer)
const canCommit = computed(() => !!draft.value.trim())
const chipWrapperClass = computed(() => (props.chipClass === undefined ? 'chip-field__chip' : props.chipClass))

function normalize(raw: string): string {
  const trimmed = raw.trim()
  if (!trimmed) return ''
  return props.normalize ? props.normalize(trimmed) : trimmed
}

function openComposer() {
  if (props.disabled) return
  composerOpen.value = true
  emit('composer-open')
  if (!useCustomComposer.value) {
    nextTick(() => inputRef.value?.focus())
  }
}

function closeComposer() {
  const wasOpen = composerOpen.value
  composerOpen.value = false
  draft.value = ''
  if (wasOpen) {
    emit('composer-close')
  }
}

function handleAddClick() {
  if (props.addBehavior === 'external') {
    emit('add-click')
    return
  }
  if (useCustomComposer.value) {
    if (composerOpen.value) {
      closeComposer()
    } else {
      openComposer()
    }
    return
  }
  if (composerOpen.value) {
    closeComposer()
  } else {
    openComposer()
  }
}

function commit(value: string) {
  const normalized = normalize(value)
  if (!normalized) return
  if (!props.allowDuplicates && values.value.includes(normalized)) {
    draft.value = ''
    return
  }
  emit('update:modelValue', [...values.value, normalized])
  draft.value = ''
}

function commitDraft() {
  if (!draft.value.trim()) return
  commit(draft.value)
  if (!useCustomComposer.value) {
    nextTick(() => inputRef.value?.focus())
  }
}

function removeValue(index: number) {
  if (props.disabled) return
  const next = [...values.value]
  next.splice(index, 1)
  emit('update:modelValue', next)
}

function applySuggestion(suggestion: string) {
  commit(suggestion)
  if (!useCustomComposer.value) {
    nextTick(() => inputRef.value?.focus())
  }
}

const suggestionsAvailable = computed(() => !!props.suggestions?.length)
const filteredSuggestions = computed(() => {
  if (!props.suggestions?.length) return []
  const needle = draft.value.trim().toLowerCase()
  return props.suggestions.filter((suggestion) => {
    if (!props.allowDuplicates && values.value.includes(suggestion)) return false
    return !needle || suggestion.toLowerCase().includes(needle)
  })
})
</script>

<style scoped>
.chip-field {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
}

.chip-field__control {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  min-height: var(--config-control-height, var(--field-height, 40px));
  padding: calc(var(--space-2, 8px) - 4px) var(--space-3, 12px);
  border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
  border-radius: var(--radius-md, 8px);
  background: color-mix(in oklab, var(--color-surface, #111827) 96%, transparent);
  transition: border-color 120ms ease, box-shadow 120ms ease;
}

.chip-field__control--open {
  border-color: var(--color-accent, #0ea5e9);
  box-shadow: var(--focus-ring, 0 0 0 2px color-mix(in srgb, var(--color-accent, #0ea5e9) 40%, transparent));
}

.chip-field__control:focus-within {
  border-color: var(--color-accent, #0ea5e9);
  box-shadow: var(--focus-ring, 0 0 0 2px color-mix(in srgb, var(--color-accent, #0ea5e9) 40%, transparent));
}

.chip-field__chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 10px;
  border-radius: 999px;
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 18%, transparent);
  color: var(--color-accent, #0ea5e9);
  font-size: 12px;
  line-height: 1.2;
}

.chip-field__chip-remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: none;
  background: none;
  color: inherit;
  cursor: pointer;
}

.chip-field__chip-remove:focus-visible {
  outline: 1px solid currentColor;
  outline-offset: 1px;
}

.chip-field__empty {
  font-size: 13px;
  color: var(--color-muted, rgba(255, 255, 255, 0.7));
}

.chip-field__add {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px dashed var(--color-border, rgba(255, 255, 255, 0.3));
  border-radius: 999px;
  padding: 4px 10px;
  background: transparent;
  color: var(--color-muted, rgba(255, 255, 255, 0.8));
  cursor: pointer;
  transition: border-color 120ms ease, color 120ms ease, background 120ms ease;
}

.chip-field__add:hover:not(:disabled) {
  border-color: var(--color-accent, #0ea5e9);
  color: var(--color-accent, #0ea5e9);
}

.chip-field__add:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.chip-field__add-label {
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.chip-field__composer {
  border: 1px solid var(--color-border, rgba(255, 255, 255, 0.12));
  border-radius: var(--radius-md, 8px);
  background: color-mix(in oklab, var(--color-surface, #111827) 94%, transparent);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  box-shadow: var(--shadow-md, 0 8px 30px rgba(0, 0, 0, 0.35));
}

.chip-field__composer-label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
}

.chip-field__composer-actions {
  display: flex;
  gap: 8px;
}

.chip-field__suggestions {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.chip-field__suggestions-label {
  font-size: 12px;
  color: var(--color-muted, rgba(255, 255, 255, 0.7));
}

.chip-field__suggestion-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.chip-field__suggestion {
  border: 1px solid var(--color-border, rgba(255, 255, 255, 0.2));
  border-radius: 999px;
  background: transparent;
  color: inherit;
  padding: 4px 10px;
  font-size: 12px;
  cursor: pointer;
}

.chip-field__suggestion:hover {
  border-color: var(--color-accent, #0ea5e9);
  color: var(--color-accent, #0ea5e9);
}

.chip-field__suggestion-empty {
  font-size: 12px;
  color: var(--color-muted, rgba(255, 255, 255, 0.6));
}

.chip-field--disabled .chip-field__control {
  opacity: 0.6;
  pointer-events: none;
}

.chip-field-composer-enter-active,
.chip-field-composer-leave-active {
  transition: opacity 120ms ease;
}

.chip-field-composer-enter-from,
.chip-field-composer-leave-to {
  opacity: 0;
}
</style>
