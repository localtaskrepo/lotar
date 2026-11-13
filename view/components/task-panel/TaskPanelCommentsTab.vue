<template>
  <div class="task-panel__tab-panel">
    <div class="task-panel__tab-actions">
      <ReloadButton
        class="task-panel__tab-action"
        variant="ghost"
        :disabled="mode !== 'edit'"
        label="Refresh comments"
        title="Refresh comments"
        @click="$emit('reload')"
      />
    </div>
    <template v-if="mode === 'edit'">
      <TaskPanelActivityFilter
        v-if="filterGroups.length"
        label="Filter comments"
        :groups="filterGroups"
        :hidden-text="hiddenText"
        @toggle="(group, value) => (group === 'type' ? filter.toggleField(value) : filter.toggleAuthor(value))"
        @show-all="filter.showAll"
      >
        <label class="task-panel__open-question-filter">
          <input
            type="checkbox"
            aria-label="Always show open questions"
            :checked="filter.alwaysShowOpenQuestions.value"
            @change="filter.toggleOpenQuestions"
          />
          Always show open questions
        </label>
      </TaskPanelActivityFilter>
      <ul class="task-panel__comments">
        <li
          v-for="{ comment, index: commentIndex } in shownComments"
          :key="`${comment.date}-${commentIndex}`"
          class="task-panel__comment"
          :class="{
            'task-panel__comment--automation': comment.author === 'automation' && !comment.question,
            'task-panel__comment--question': comment.question,
            [`task-panel__comment--review-${comment.review?.verdict}`]: comment.review,
          }"
          :data-testid="comment.question ? 'comment-question' : comment.review ? 'comment-review' : undefined"
        >
          <div class="task-panel__comment-header">
            <div class="task-panel__comment-meta">
              <span v-if="comment.author" class="task-panel__comment-author">{{ comment.author }} · </span
              >{{ formatDate(comment.date) }}
              <ReviewBadge v-if="comment.review" :review="comment.review" class="task-panel__comment-review-badge" />
            </div>
            <div class="task-panel__comment-actions">
              <template v-if="editingCommentIndex !== commentIndex">
                <UiButton
                  variant="ghost"
                  type="button"
                  class="task-panel__comment-action"
                  @click="$emit('startEdit', commentIndex)"
                  >Edit</UiButton
                >
              </template>
              <template v-else>
                <UiButton
                  variant="primary"
                  type="button"
                  class="task-panel__comment-action"
                  :disabled="editingCommentSubmitting || !editingCommentText.trim()"
                  @click="$emit('saveEdit', commentIndex)"
                  >Save</UiButton
                >
                <UiButton
                  variant="ghost"
                  type="button"
                  class="task-panel__comment-action"
                  :disabled="editingCommentSubmitting"
                  @click="$emit('cancelEdit')"
                  >Cancel</UiButton
                >
              </template>
            </div>
          </div>
          <template v-if="editingCommentIndex !== commentIndex && comment.question">
            <MarkdownContent :source="questionPrompt(comment)" />
            <div
              v-if="answerOf(comment) && isAnswerVisible(comment)"
              class="task-panel__answer"
              data-testid="comment-answer"
            >
              <div class="task-panel__comment-meta">
                <span v-if="answerOf(comment)?.author" class="task-panel__comment-author"
                  >{{ answerOf(comment)?.author }} · </span
                >answered {{ formatDate(answerOf(comment)?.date || '') }}
              </div>
              <ul v-if="answerChoice(comment).length" class="task-panel__answer-choice">
                <li v-for="label in answerChoice(comment)" :key="label">{{ label }}</li>
              </ul>
              <MarkdownContent v-if="answerOf(comment)?.answer?.note" :source="answerOf(comment)?.answer?.note || ''" />
            </div>
            <p v-else-if="answerOf(comment)" class="muted">Answer hidden by filter</p>
            <QuestionPrompt
              v-else
              :spec="comment.question"
              :prompt="questionPrompt(comment)"
              :busy="answeringQuestion === comment.date"
              @answer="$emit('answer', comment, $event)"
            />
          </template>
          <template v-else-if="editingCommentIndex !== commentIndex && comment.review">
            <MarkdownContent v-if="reviewSummary(comment)" :source="reviewSummary(comment)" />
            <ul v-if="comment.review.findings?.length" class="task-panel__review-findings">
              <li
                v-for="(finding, findingIndex) in sortedFindings(comment.review)"
                :key="findingIndex"
                :class="{ 'task-panel__review-finding--blocking': finding.blocking }"
                data-testid="review-finding"
              >
                <span v-if="finding.blocking" class="task-panel__review-finding-mark">Blocking</span>
                <code v-if="finding.file" class="task-panel__review-finding-file"
                  >{{ finding.file }}{{ finding.line ? `:${finding.line}` : '' }}</code
                >
                <span>{{ finding.summary }}</span>
              </li>
            </ul>
          </template>
          <MarkdownContent v-else-if="editingCommentIndex !== commentIndex" :source="comment.text" />
          <CommentReport
            v-if="comment.report && editingCommentIndex !== commentIndex"
            :report="comment.report"
            :commit-url="branding.commitUrl"
          />
          <textarea
            v-if="editingCommentIndex === commentIndex"
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
        <li v-if="!comments.length" class="muted">No comments yet</li>
        <li v-else-if="!visibleComments.length" class="muted">All comments are hidden by the filter</li>
      </ul>
      <div class="task-panel__comment-editor">
        <textarea :value="newComment" rows="3" placeholder="Add a comment…" @input="onNewCommentInput"></textarea>
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
import { computed, type ComponentPublicInstance } from 'vue'
import { COMMENT_FILTER_KEY, useActivityFilter } from '../../composables/task-panel/useActivityFilter'
import type { CommentReview, ReviewFinding, TaskComment } from '../../api/types'
import { useBranding } from '../../composables/useBranding'
import { answerLabels, answerTo, questionPrompt } from '../../utils/questions'
import { reviewSummary } from '../../utils/reviews'
import IconGlyph from '../IconGlyph.vue'
import MarkdownContent from '../MarkdownContent.vue'
import QuestionPrompt from '../QuestionPrompt.vue'
import ReloadButton from '../ReloadButton.vue'
import ReviewBadge from '../ReviewBadge.vue'
import UiButton from '../UiButton.vue'
import CommentReport from './CommentReport.vue'
import TaskPanelActivityFilter, { type ActivityFilterGroup } from './TaskPanelActivityFilter.vue'

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
  /** DEV-488: the `date` of the question an answer is being sent for. */
  answeringQuestion?: string | null
}>()

const emit = defineEmits<{
  (e: 'reload'): void
  (e: 'startEdit', index: number): void
  (e: 'saveEdit', index: number): void
  (e: 'cancelEdit'): void
  (e: 'addComment'): void
  (e: 'answer', question: TaskComment, value: { choice: string[]; note: string }): void
  (e: 'update:newComment', value: string): void
  (e: 'update:editingCommentText', value: string): void
}>()

const filter = useActivityFilter(COMMENT_FILTER_KEY)
const branding = useBranding()

const comments = computed(() => (props.task.comments || []).map((comment, index) => ({ comment, index })))
function commentType(comment: TaskComment): string {
  return comment.question
    ? 'question'
    : comment.answer
      ? 'answer'
      : comment.review
        ? 'review'
        : comment.report
          ? 'handoff'
          : 'comment'
}

const TYPE_LABELS: Record<string, string> = {
  question: 'Question',
  answer: 'Answer',
  review: 'Review',
  handoff: 'Hand-off',
  comment: 'Comment',
}

/** Blocking findings first, each group in the reviewer's order. */
function sortedFindings(review: CommentReview): ReviewFinding[] {
  const findings = review.findings ?? []
  return [...findings.filter((finding) => finding.blocking), ...findings.filter((finding) => !finding.blocking)]
}

function isVisible(comment: TaskComment): boolean {
  if (filter.alwaysShowOpenQuestions.value && comment.question && !answerOf(comment)) return true
  return !filter.hiddenAuthors.value.has(comment.author || '') && !filter.hiddenFields.value.has(commentType(comment))
}

function isAnswerVisible(question: TaskComment): boolean {
  const answer = answerOf(question)
  return !!answer && isVisible(answer)
}

const visibleComments = computed(() => comments.value.filter(({ comment }) => isVisible(comment)))

/** One chip per author on this task, even a single one, so a task with only automation's comments can hide them. */
const filterGroups = computed<ActivityFilterGroup[]>(() => {
  const authors = [...new Set(comments.value.map(({ comment }) => comment.author || ''))]
  if (!authors.length) return []
  return [
    {
      id: 'author',
      name: 'Author',
      items: authors.map((author) => ({
        value: author,
        label: author || 'No author',
        hidden: filter.hiddenAuthors.value.has(author),
      })),
    },
    ...(comments.value.some(({ comment }) => comment.question || comment.answer || comment.review || comment.report)
      ? [
          {
            id: 'type',
            name: 'Type',
            items: [...new Set(comments.value.map(({ comment }) => commentType(comment)))].map((type) => ({
              value: type,
              label: TYPE_LABELS[type] ?? type,
              hidden: filter.hiddenFields.value.has(type),
            })),
          },
        ]
      : []),
  ]
})

const hiddenText = computed(() => {
  const hidden = comments.value.length - visibleComments.value.length
  if (!hidden) return ''
  return `${hidden} of ${comments.value.length} ${comments.value.length === 1 ? 'comment' : 'comments'} hidden.`
})

const allComments = computed(() => props.task.comments || [])

/** Answers show under their question (DEV-488), so the list leaves them out. */
const shownComments = computed(() => {
  const visibleQuestions = new Set(
    visibleComments.value.filter(({ comment }) => comment.question).map(({ comment }) => comment.date),
  )
  return visibleComments.value.filter(({ comment }) => !(comment.answer && visibleQuestions.has(comment.answer.to)))
})

function answerOf(question: TaskComment): TaskComment | null {
  return answerTo(allComments.value, question)
}

function answerChoice(question: TaskComment): string[] {
  const answer = answerOf(question)?.answer
  return answer ? answerLabels(question, answer) : []
}

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
.task-panel__open-question-filter {
  display: flex;
  align-items: center;
  gap: var(--space-1, 0.25rem);
  font-size: var(--text-xs, 0.75rem);
}

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

/* Automation's notes (job failed, retry started) step back so the agents' and the owner's comments stand out. */
.task-panel__comment--automation {
  padding-block: var(--space-1, 0.25rem);
  background: transparent;
  color: var(--color-muted, var(--muted));
  font-size: var(--text-xs, 0.75rem);
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

/* A review badge wraps inside the meta line; Edit stays beside it on narrow panels. */
.task-panel__comment-header > .task-panel__comment-meta {
  flex: 1 1 0;
  min-width: 0;
}

.task-panel__comment-author {
  font-weight: 600;
}

.task-panel__comment--question {
  border-left: 3px solid var(--color-accent, var(--accent));
}

.task-panel__comment--question > :deep(.markdown-content),
.task-panel__comment--question > .question-prompt {
  margin-bottom: var(--space-2, 0.5rem);
}

.task-panel__comment--review-pass {
  border-left: 3px solid var(--color-success);
}

.task-panel__comment--review-block {
  border-left: 3px solid var(--color-danger);
}

.task-panel__comment-review-badge {
  margin-left: var(--space-2, 0.5rem);
}

.task-panel__review-findings {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  margin: var(--space-2, 0.5rem) 0 0;
  padding-left: 1.2rem;
}

.task-panel__review-findings > li {
  overflow-wrap: anywhere;
}

.task-panel__review-finding-mark {
  margin-right: var(--space-1, 0.25rem);
  font-weight: 600;
  color: var(--color-danger-strong);
}

.task-panel__review-finding-file {
  margin-right: var(--space-1, 0.25rem);
  font-family: var(--font-mono);
}

.task-panel__answer {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  margin-top: var(--space-2, 0.5rem);
  padding-left: var(--space-2, 0.5rem);
  border-left: 2px solid var(--color-border, var(--border));
}

.task-panel__answer-choice {
  margin: 0;
  padding-left: 1.2rem;
  font-weight: 600;
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
