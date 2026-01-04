<template>
  <div class="task-panel__tab-panel">
    <header class="task-panel__group-header">
      <h3>Comments</h3>
      <ReloadButton
        variant="ghost"
        :disabled="mode !== 'edit'"
        label="Refresh comments"
        title="Refresh comments"
        @click="$emit('reload')"
      />
    </header>
    <template v-if="mode === 'edit'">
      <ul class="task-panel__comments">
        <li
          v-for="(comment, commentIndex) in task.comments || []"
          :key="`${comment.date}-${commentIndex}`"
          class="task-panel__comment"
        >
          <div class="task-panel__comment-header">
            <div class="task-panel__comment-meta">{{ formatDate(comment.date) }}</div>
            <div class="task-panel__comment-actions">
              <template v-if="editingCommentIndex !== commentIndex">
                <UiButton
                  variant="ghost"
                  type="button"
                  class="task-panel__comment-action"
                  @click="$emit('startEdit', commentIndex)"
                >Edit</UiButton>
              </template>
              <template v-else>
                <UiButton
                  variant="primary"
                  type="button"
                  class="task-panel__comment-action"
                  :disabled="editingCommentSubmitting || !editingCommentText.trim()"
                  @click="$emit('saveEdit', commentIndex)"
                >Save</UiButton>
                <UiButton
                  variant="ghost"
                  type="button"
                  class="task-panel__comment-action"
                  :disabled="editingCommentSubmitting"
                  @click="$emit('cancelEdit')"
                >Cancel</UiButton>
              </template>
            </div>
          </div>
          <MarkdownContent v-if="editingCommentIndex !== commentIndex" :source="comment.text" />
          <textarea
            v-else
            :ref="onEditingTextareaRef"
            :value="editingCommentText"
            class="task-panel__comment-edit"
            rows="3"
            :disabled="editingCommentSubmitting"
            @input="onEditingTextInput"
            @keydown.meta.enter.prevent="$emit('saveEdit', commentIndex)"
            @keydown.ctrl.enter.prevent="$emit('saveEdit', commentIndex)"
          ></textarea>
        </li>
        <li v-if="!(task.comments && task.comments.length)" class="muted">No comments yet</li>
      </ul>
      <div class="task-panel__comment-editor">
        <textarea
          :value="newComment"
          rows="3"
          placeholder="Add a comment…"
          @input="onNewCommentInput"
        ></textarea>
        <UiButton
          class="task-panel__comment-submit"
          variant="primary"
          icon-only
          type="button"
          aria-label="Add comment"
          :disabled="!newComment.trim() || submitting"
          @click="$emit('addComment')"
        >
          <IconGlyph name="send" />
        </UiButton>
      </div>
    </template>
    <p v-else class="task-panel__empty-hint">Comments become available after the task is created.</p>
  </div>
</template>

<script setup lang="ts">
import type { ComponentPublicInstance } from 'vue';
import IconGlyph from '../IconGlyph.vue';
import MarkdownContent from '../MarkdownContent.vue';
import ReloadButton from '../ReloadButton.vue';
import UiButton from '../UiButton.vue';

interface TaskComment {
  date: string
  text: string
}

const props = defineProps<{
  mode: 'create' | 'edit'
  task: { comments?: TaskComment[] | null }
  newComment: string
  submitting: boolean
  editingCommentIndex: number | null
  editingCommentText: string
  editingCommentSubmitting: boolean
  formatDate: (input: string) => string
  setEditingTextarea: (el: HTMLTextAreaElement | null) => void
}>()

const emit = defineEmits<{ (e: 'reload'): void; (e: 'startEdit', index: number): void; (e: 'saveEdit', index: number): void; (e: 'cancelEdit'): void; (e: 'addComment'): void; (e: 'update:newComment', value: string): void; (e: 'update:editingCommentText', value: string): void }>()

function onEditingTextInput(event: Event) {
  emit('update:editingCommentText', (event.target as HTMLTextAreaElement).value)
}

function onNewCommentInput(event: Event) {
  emit('update:newComment', (event.target as HTMLTextAreaElement).value)
}

function onEditingTextareaRef(ref: Element | ComponentPublicInstance | null) {
  props.setEditingTextarea((ref as HTMLTextAreaElement) || null)
}
</script>

<style scoped>
.task-panel__comments {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__comment {
  padding: var(--space-2, 0.5rem);
  border-radius: var(--radius-md, 0.375rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 94%, transparent);
}

.task-panel__comment-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: var(--space-2, 0.5rem);
  flex-wrap: wrap;
}

.task-panel__comment-meta {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__comment-actions {
  display: inline-flex;
  flex-wrap: wrap;
  gap: var(--space-2, 0.5rem);
}

.task-panel__comment-action {
  padding: 0 var(--space-2, 0.5rem);
  font-size: var(--text-xs, 0.75rem);
  line-height: 1.4;
}

.task-panel__comment-edit {
  width: 100%;
  resize: vertical;
  font-size: var(--text-sm, 0.875rem);
  padding: var(--space-2, 0.5rem);
  border-radius: var(--radius-md, 0.375rem);
  border: 1px solid var(--color-border, var(--border));
  font-family: inherit;
  background: var(--color-surface, var(--bg));
}

.task-panel__comment-editor {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2, 0.5rem);
}

.task-panel__comment-editor textarea {
  flex: 1;
  min-height: 96px;
  resize: vertical;
}

.task-panel__comment-submit {
  width: 40px;
  height: 40px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 1.1rem;
}

.task-panel__comment-submit .icon-glyph {
  width: 1.1rem;
  height: 1.1rem;
}
</style>
