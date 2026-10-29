<template>
  <div class="template-field">
    <div class="template-field__control">
      <textarea
        v-if="multiline"
        ref="inputRef"
        class="input template-field__input"
        :value="modelValue"
        :placeholder="placeholder"
        :aria-label="label"
        rows="3"
        :data-testid="testid"
        @input="onInput"
        @keydown="onKeydown"
        @click="updateSuggestions"
        @blur="closeSoon"
      ></textarea>
      <input
        v-else
        ref="inputRef"
        class="input template-field__input"
        :value="modelValue"
        :placeholder="placeholder"
        :aria-label="label"
        :data-testid="testid"
        @input="onInput"
        @keydown="onKeydown"
        @click="updateSuggestions"
        @blur="closeSoon"
      />
      <button
        type="button"
        class="template-field__insert"
        :aria-expanded="menuOpen"
        aria-haspopup="listbox"
        title="Insert a value from the task"
        :data-testid="testid ? `${testid}-insert` : undefined"
        @mousedown.prevent
        @click="toggleMenu"
      >
        {{ '{ }' }} Insert value
      </button>
    </div>

    <ul v-if="menuOpen && shown.length" class="template-field__menu" role="listbox" :aria-label="`Values for ${label}`">
      <template v-for="(group, groupIndex) in groupedShown" :key="group.name">
        <li class="template-field__group" role="presentation">{{ group.name }}</li>
        <li
          v-for="(variable, index) in group.items"
          :key="variable.key"
          role="option"
          class="template-field__option"
          :class="{ 'is-active': offsetOf(groupIndex) + index === activeIndex }"
          :aria-selected="offsetOf(groupIndex) + index === activeIndex"
          @mousedown.prevent="insert(variable)"
        >
          <span>{{ variable.label }}</span>
          <code>{{ variable.key }}</code>
        </li>
      </template>
    </ul>

    <p v-if="preview" class="template-field__preview" :data-testid="testid ? `${testid}-preview` : undefined">
      <span class="muted">{{ previewTask ? `On ${previewTask.id}:` : 'Preview:' }}</span> {{ preview }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import type { TaskDTO } from '../../api/types'
import { openPlaceholderAt, previewTemplate, type TemplateVariable } from '../../utils/automationText'

/**
 * A text field for automation values that can include `${{...}}`
 * placeholders (DEV-241): an "Insert value" menu lists the values available
 * for the rule's triggers, typing `${{` suggests them, and a preview shows
 * the text filled in for a sample task.
 */
const props = withDefaults(
  defineProps<{
    modelValue: string
    label: string
    variables: TemplateVariable[]
    multiline?: boolean
    placeholder?: string
    previewTask?: TaskDTO | null
    testid?: string
  }>(),
  { multiline: false, placeholder: '', previewTask: null, testid: undefined },
)

const emit = defineEmits<{ (e: 'update:modelValue', value: string): void }>()

const inputRef = ref<HTMLInputElement | HTMLTextAreaElement | null>(null)
const menuOpen = ref(false)
const typed = ref<{ start: number; query: string } | null>(null)
const activeIndex = ref(0)

const shown = computed(() => {
  const query = typed.value?.query.toLowerCase() ?? ''
  if (!query) return props.variables
  const matches = props.variables.filter(
    (variable) => variable.key.toLowerCase().includes(query) || variable.label.toLowerCase().includes(query),
  )
  // Keys that start with what was typed come first.
  const starts = (variable: TemplateVariable) => (variable.key.toLowerCase().startsWith(query) ? 0 : 1)
  return [...matches].sort((a, b) => starts(a) - starts(b))
})

const groupedShown = computed(() => {
  const groups: Array<{ name: string; items: TemplateVariable[] }> = []
  for (const variable of shown.value) {
    let group = groups.find((entry) => entry.name === variable.group)
    if (!group) {
      group = { name: variable.group, items: [] }
      groups.push(group)
    }
    group.items.push(variable)
  }
  return groups
})

function offsetOf(groupIndex: number) {
  return groupedShown.value.slice(0, groupIndex).reduce((sum, group) => sum + group.items.length, 0)
}

const orderedShown = computed(() => groupedShown.value.flatMap((group) => group.items))

const preview = computed(() =>
  props.modelValue.includes('${{') ? previewTemplate(props.modelValue, props.previewTask ?? null) : '',
)

function onInput(event: Event) {
  emit('update:modelValue', (event.target as HTMLInputElement).value)
  void nextTick(updateSuggestions)
}

function updateSuggestions() {
  const element = inputRef.value
  if (!element) return
  typed.value = openPlaceholderAt(element.value, element.selectionStart ?? element.value.length)
  menuOpen.value = Boolean(typed.value)
  activeIndex.value = 0
}

function toggleMenu() {
  typed.value = null
  menuOpen.value = !menuOpen.value
  activeIndex.value = 0
  if (menuOpen.value) inputRef.value?.focus()
}

function closeSoon() {
  setTimeout(() => {
    menuOpen.value = false
  }, 120)
}

function insert(variable: TemplateVariable) {
  const element = inputRef.value
  const text = props.modelValue
  const placeholder = `\${{${variable.key}}}`
  let next: string
  let caret: number
  if (typed.value) {
    const end = element?.selectionStart ?? text.length
    next = text.slice(0, typed.value.start) + placeholder + text.slice(end)
    caret = typed.value.start + placeholder.length
  } else {
    const start = element?.selectionStart ?? text.length
    const end = element?.selectionEnd ?? start
    next = text.slice(0, start) + placeholder + text.slice(end)
    caret = start + placeholder.length
  }
  emit('update:modelValue', next)
  menuOpen.value = false
  typed.value = null
  void nextTick(() => {
    element?.focus()
    element?.setSelectionRange(caret, caret)
  })
}

function onKeydown(event: KeyboardEvent) {
  if (!menuOpen.value || !orderedShown.value.length) return
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    activeIndex.value = (activeIndex.value + 1) % orderedShown.value.length
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    activeIndex.value = (activeIndex.value - 1 + orderedShown.value.length) % orderedShown.value.length
  } else if (event.key === 'Enter' || event.key === 'Tab') {
    const variable = orderedShown.value[activeIndex.value]
    if (variable) {
      event.preventDefault()
      insert(variable)
    }
  } else if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    menuOpen.value = false
  }
}
</script>

<style scoped>
.template-field {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  flex: 1;
}

.template-field__control {
  display: flex;
  gap: 6px;
  align-items: flex-start;
}

.template-field__input {
  flex: 1;
  min-width: 0;
}

textarea.template-field__input {
  resize: vertical;
  font: inherit;
}

.template-field__insert {
  flex: 0 0 auto;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-surface);
  color: var(--color-muted);
  font-size: var(--text-xs, 0.75rem);
  padding: 6px 8px;
  cursor: pointer;
  white-space: nowrap;
}

.template-field__insert:hover {
  color: var(--color-fg);
}

.template-field__menu {
  position: absolute;
  top: 100%;
  left: 0;
  z-index: var(--z-popover, 50);
  width: min(360px, 100%);
  max-height: 260px;
  overflow-y: auto;
  margin: 4px 0 0;
  padding: 4px;
  list-style: none;
  background: var(--color-bg);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-popover);
}

.template-field__group {
  padding: 6px 8px 2px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--color-muted);
}

.template-field__option {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 5px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: var(--text-sm, 0.875rem);
}

.template-field__option code {
  color: var(--color-muted);
  font-size: 11px;
}

.template-field__option.is-active,
.template-field__option:hover {
  background: color-mix(in oklab, var(--color-accent) 14%, transparent);
}

.template-field__preview {
  margin: 0;
  font-size: var(--text-xs, 0.75rem);
  overflow-wrap: anywhere;
}
</style>
