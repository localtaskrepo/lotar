<template>
  <div class="question-prompt" data-testid="question-prompt" @click.stop @dblclick.stop @keydown.stop>
    <div class="question-prompt__options" role="group" :aria-label="`Answers to: ${promptLabel}`">
      <UiButton
        v-for="option in spec.options"
        :key="option.key"
        type="button"
        :variant="isChosen(option.key) || option.key === spec.recommended ? 'primary' : ''"
        class="question-prompt__option"
        :class="{ 'question-prompt__option--chosen': isChosen(option.key) }"
        :aria-pressed="spec.multiple ? isChosen(option.key) : undefined"
        :title="optionTitle(option)"
        :disabled="busy"
        :data-option="option.key"
        @click="pick(option.key)"
        >{{ option.label
        }}<span v-if="option.key === spec.recommended" class="question-prompt__recommended">
          · recommended</span
        ></UiButton
      >
    </div>
    <div v-if="spec.multiple" class="question-prompt__row">
      <UiButton
        type="button"
        variant="primary"
        :disabled="busy || !chosen.length"
        data-testid="question-submit-choice"
        @click="submit(chosen, '')"
        >Answer with {{ chosen.length || 'the chosen' }} option{{ chosen.length === 1 ? '' : 's' }}</UiButton
      >
    </div>
    <form v-if="spec.other !== false" class="question-prompt__row question-prompt__other" @submit.prevent="submitOther">
      <input
        v-model="other"
        type="text"
        class="input question-prompt__other-input"
        placeholder="Other answer…"
        aria-label="Other answer"
        :disabled="busy"
        data-testid="question-other-input"
      />
      <UiButton type="submit" :disabled="busy || !other.trim()" data-testid="question-other-submit">Answer</UiButton>
    </form>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { CommentQuestion, QuestionOption } from '../api/types'
import UiButton from './UiButton.vue'

/** DEV-488: the options of an open question as buttons, plus an "Other" field. */
const props = defineProps<{
  spec: CommentQuestion
  prompt: string
  busy?: boolean
}>()

const emit = defineEmits<{
  (event: 'answer', value: { choice: string[]; note: string }): void
}>()

const chosen = ref<string[]>([])
const other = ref('')

const promptLabel = computed(() => props.prompt.split('\n')[0] || 'question')

function isChosen(key: string): boolean {
  return chosen.value.includes(key)
}

function optionTitle(option: QuestionOption): string {
  const set = option.set
  if (!set) return option.label
  const parts = Object.entries(set).map(
    ([field, value]) => `${field}: ${Array.isArray(value) ? value.join(', ') : value}`,
  )
  return `${option.label} (sets ${parts.join('; ')})`
}

function pick(key: string) {
  if (!props.spec.multiple) {
    submit([key], '')
    return
  }
  chosen.value = isChosen(key) ? chosen.value.filter((value) => value !== key) : [...chosen.value, key]
}

function submitOther() {
  const note = other.value.trim()
  if (!note) return
  submit(props.spec.multiple ? chosen.value : [], note)
}

function submit(choice: string[], note: string) {
  emit('answer', { choice, note })
}
</script>

<style scoped>
.question-prompt {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
  cursor: default;
}

.question-prompt__options {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2, 0.5rem);
}

.question-prompt__option {
  max-width: 100%;
  white-space: normal;
  text-align: left;
}

.question-prompt__recommended {
  font-size: var(--text-xs, 0.75rem);
  opacity: 0.85;
}

.question-prompt__row {
  display: flex;
  gap: var(--space-2, 0.5rem);
  align-items: center;
}

.question-prompt__other-input {
  flex: 1;
  min-width: 0;
}
</style>
