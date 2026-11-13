<template>
  <div v-if="question" class="open-question" data-testid="open-question" @click.stop @dblclick.stop @mousedown.stop>
    <UiButton
      variant="ghost"
      type="button"
      class="open-question__toggle"
      :aria-expanded="expanded"
      :title="expanded ? 'Hide the question' : 'Answer the open question'"
      data-testid="open-question-toggle"
      @click="expanded = !expanded"
    >
      <IconGlyph name="help" />
      <span class="open-question__summary">{{ summary }}</span>
    </UiButton>
    <div v-if="expanded" class="open-question__body">
      <MarkdownContent :source="prompt" class="open-question__prompt" />
      <QuestionPrompt
        v-if="question.question"
        :spec="question.question"
        :prompt="prompt"
        :busy="busy"
        @answer="answer"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { api } from '../api/client'
import type { TaskDTO } from '../api/types'
import { useTaskStore } from '../composables/useTaskStore'
import { errorMessage } from '../utils/errors'
import { lastOpenQuestion, questionPrompt } from '../utils/questions'
import IconGlyph from './IconGlyph.vue'
import MarkdownContent from './MarkdownContent.vue'
import QuestionPrompt from './QuestionPrompt.vue'
import UiButton from './UiButton.vue'
import { showToast } from './toast'

/** DEV-488: a task's newest open question on a board card or list row, answerable in place. */
const props = defineProps<{ task: Pick<TaskDTO, 'id' | 'comments'> }>()

const expanded = ref(false)
const busy = ref(false)

const question = computed(() => lastOpenQuestion(props.task.comments))
const prompt = computed(() => (question.value ? questionPrompt(question.value) : ''))
const summary = computed(() => {
  const first = prompt.value.split('\n')[0]?.trim() || 'Open question'
  return first.length > 80 ? `${first.slice(0, 79)}…` : first
})

async function answer(value: { choice: string[]; note: string }) {
  const target = question.value
  if (!target || busy.value) return
  busy.value = true
  try {
    const updated = await api.answerQuestion(props.task.id, {
      to: target.date,
      choice: value.choice,
      note: value.note || undefined,
    })
    useTaskStore().upsert(updated)
    expanded.value = false
  } catch (error: unknown) {
    showToast(errorMessage(error) || 'Failed to answer the question')
  } finally {
    busy.value = false
  }
}
</script>

<style scoped>
.open-question {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
  min-width: 0;
}

.open-question__toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  padding: 2px 6px;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-accent, var(--accent));
  justify-content: flex-start;
}

.open-question__toggle .icon-glyph {
  flex: 0 0 auto;
  width: 0.9rem;
  height: 0.9rem;
}

.open-question__summary {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.open-question__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
  padding: var(--space-2, 0.5rem);
  border: 1px solid var(--border);
  border-radius: var(--radius-base);
  background: var(--color-surface, var(--bg));
}

.open-question__prompt {
  font-size: var(--text-sm, 0.875rem);
}
</style>
