<template>
  <div class="token-input" @click="focusInput">
    <div class="tokens">
      <span v-for="(token, index) in internalValue" :key="`${token}-${index}`" class="token">
        <span class="token__label">{{ token }}</span>
        <button type="button" class="token__remove" @click.stop="removeToken(index)" aria-label="Remove token">×</button>
      </span>
      <input
        ref="inputRef"
        v-model="draft"
        type="text"
        :placeholder="placeholder"
        @keydown.enter.prevent="commitDraft"
        @keydown.tab="onTab"
        @keydown.delete="onDelete"
        @blur="commitDraft"
      />
    </div>
    <div v-if="showSuggestions" class="suggestions">
      <button
        v-for="suggestion in filteredSuggestions"
        :key="suggestion"
        type="button"
        class="suggestions__item"
        @click="applySuggestion(suggestion)"
      >
        {{ suggestion }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, ref } from 'vue';

const props = defineProps<{
  modelValue: string[]
  placeholder?: string
  suggestions?: string[]
  normalize?: (token: string) => string
  allowDuplicates?: boolean
}>()
const emit = defineEmits<{
  (e: 'update:modelValue', value: string[]): void
  (e: 'invalid', value: string): void
}>()

const draft = ref('')
const inputRef = ref<HTMLInputElement | null>(null)

const internalValue = computed(() => props.modelValue ?? [])

function focusInput() {
  nextTick(() => {
    inputRef.value?.focus()
  })
}

function normalizeToken(raw: string): string {
  const trimmed = raw.trim()
  if (!trimmed) return ''
  return props.normalize ? props.normalize(trimmed) : trimmed
}

function commit(token: string) {
  const normalized = normalizeToken(token)
  if (!normalized) return
  if (!props.allowDuplicates && internalValue.value.includes(normalized)) {
    draft.value = ''
    return
  }
  emit('update:modelValue', [...internalValue.value, normalized])
  draft.value = ''
}

function commitDraft() {
  if (draft.value.trim()) {
    commit(draft.value)
  } else {
    draft.value = ''
  }
}

function removeToken(index: number) {
  const next = [...internalValue.value]
  next.splice(index, 1)
  emit('update:modelValue', next)
}

function applySuggestion(value: string) {
  commit(value)
}

function onTab(event: KeyboardEvent) {
  if (!draft.value.trim()) return
  event.preventDefault()
  commitDraft()
}

function onDelete(event: KeyboardEvent) {
  if (draft.value === '' && internalValue.value.length) {
    event.preventDefault()
    const last = internalValue.value[internalValue.value.length - 1]
    emit('update:modelValue', internalValue.value.slice(0, -1))
    draft.value = last
    nextTick(() => inputRef.value?.setSelectionRange(last.length, last.length))
  }
}

const showSuggestions = computed(() => {
  return !!props.suggestions?.length
})

const filteredSuggestions = computed(() => {
  if (!props.suggestions) return []
  const lowerDraft = draft.value.toLowerCase()
  return props.suggestions.filter((s) => {
    if (!props.allowDuplicates && internalValue.value.includes(s)) return false
    return !lowerDraft || s.toLowerCase().includes(lowerDraft)
  })
})
</script>

<style scoped>
.token-input {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.tokens {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  padding: 6px 8px;
  background: var(--surface-2, #1f232a);
  border: 1px solid var(--border-muted, rgba(255, 255, 255, 0.1));
  border-radius: 6px;
}

.tokens input {
  flex: 1;
  min-width: 120px;
  background: transparent;
  border: none;
  outline: none;
  color: inherit;
  font: inherit;
  padding: 4px 0;
}

.token {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  border-radius: 999px;
  background: var(--accent-muted, rgba(0, 150, 255, 0.15));
  color: var(--accent-strong, #76c7ff);
  font-size: 12px;
}

.token__remove {
  background: none;
  border: none;
  color: currentColor;
  cursor: pointer;
  font-size: 12px;
  line-height: 1;
}

.token__remove:focus-visible {
  outline: 1px solid currentColor;
  outline-offset: 2px;
}

.suggestions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.suggestions__item {
  border: 1px solid var(--border-muted, rgba(255, 255, 255, 0.1));
  background: transparent;
  color: inherit;
  padding: 3px 8px;
  border-radius: 999px;
  font-size: 12px;
  cursor: pointer;
}

.suggestions__item:hover {
  background: var(--surface-hover, rgba(255, 255, 255, 0.1));
}
</style>
