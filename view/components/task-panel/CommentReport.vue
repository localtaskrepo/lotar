<template>
  <div class="comment-report" data-testid="comment-report">
    <ul v-if="report.commits?.length" class="comment-report__list" aria-label="Commits">
      <li v-for="commit in report.commits" :key="commit.hash" class="comment-report__commit">
        <a
          v-if="commitUrl"
          :href="`${commitUrl}${commit.hash}`"
          target="_blank"
          rel="noopener noreferrer"
          class="comment-report__hash"
          >{{ shortHash(commit.hash) }}</a
        >
        <code v-else class="comment-report__hash">{{ shortHash(commit.hash) }}</code>
        <span v-if="commit.what">{{ commit.what }}</span>
      </li>
    </ul>
    <ul v-if="report.checks?.length" class="comment-report__list" aria-label="Checks">
      <li
        v-for="(check, index) in report.checks"
        :key="`${check.command}-${index}`"
        class="comment-report__check"
        :data-result="check.result"
      >
        <span class="comment-report__mark" :aria-label="resultLabel(check.result)">{{ resultMark(check.result) }}</span>
        <code>{{ check.command }}</code>
        <span class="comment-report__result">{{ resultLabel(check.result) }}</span>
        <span v-if="check.reason" class="comment-report__reason">{{ check.reason }}</span>
      </li>
    </ul>
    <p class="comment-report__missing" data-testid="comment-report-missing" v-if="missing">{{ missing }}</p>
    <p v-if="report.next" class="comment-report__next">
      <span class="comment-report__label">Next:</span> {{ report.next }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { CommentReport } from '../../api/types'

const props = defineProps<{
  report: CommentReport
  /** A commit's web page without the hash; commits are links with it. */
  commitUrl?: string | null
}>()

const RESULTS: Record<string, { mark: string; label: string }> = {
  passed: { mark: '✓', label: 'passed' },
  failed: { mark: '✗', label: 'failed' },
  not_run: { mark: '–', label: 'not run' },
}

function resultMark(result: string): string {
  return RESULTS[result]?.mark ?? '?'
}

function resultLabel(result: string): string {
  return RESULTS[result]?.label ?? result
}

function shortHash(hash: string): string {
  return hash.slice(0, 8)
}

// The owner's rule is "say which checks didn't run": a report that lists
// none says so, so a forgotten "not run" is visible.
const missing = computed(() => {
  const checks = props.report.checks ?? []
  if (!checks.length) return 'No checks listed'
  if (!checks.some((check) => check.result === 'not_run')) return 'No check marked not run'
  return ''
})
</script>

<style scoped>
.comment-report {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  margin-top: var(--space-2, 0.5rem);
  padding-top: var(--space-2, 0.5rem);
  border-top: 1px solid var(--color-border, var(--border));
  font-size: var(--text-sm, 0.875rem);
}

.comment-report__list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
}

.comment-report__commit,
.comment-report__check {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--space-2, 0.5rem);
  min-width: 0;
}

.comment-report__check code,
.comment-report__hash {
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: var(--text-xs, 0.75rem);
  overflow-wrap: anywhere;
}

.comment-report__mark {
  width: 1em;
  text-align: center;
  font-weight: 700;
}

.comment-report__check[data-result='passed'] .comment-report__mark {
  color: var(--color-success);
}

.comment-report__check[data-result='failed'] .comment-report__mark,
.comment-report__check[data-result='failed'] .comment-report__result {
  color: var(--color-danger);
}

.comment-report__check[data-result='not_run'] .comment-report__mark,
.comment-report__check[data-result='not_run'] .comment-report__result {
  color: var(--color-warning);
}

.comment-report__result {
  font-size: var(--text-xs, 0.75rem);
}

.comment-report__reason,
.comment-report__missing {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.comment-report__missing {
  margin: 0;
  font-style: italic;
}

.comment-report__next {
  margin: 0;
}

.comment-report__label {
  font-weight: 600;
}
</style>
