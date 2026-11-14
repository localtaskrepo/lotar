<template>
  <span
    class="review-badge"
    :class="`review-badge--${review.verdict}`"
    :data-verdict="review.verdict"
    :title="review.commit ? `${verdictLabel(review)} at ${review.commit}` : verdictLabel(review)"
  >
    {{ verdictLabel(review) }}
    <code v-if="review.commit" class="review-badge__commit">{{ shortCommit(review.commit) }}</code>
  </span>
</template>

<script setup lang="ts">
import type { CommentReview } from '../api/types'
import { shortCommit, verdictLabel } from '../utils/reviews'

defineProps<{ review: CommentReview }>()
</script>

<style scoped>
.review-badge {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1, 0.25rem);
  padding: 0.1rem 0.5rem;
  border-radius: var(--radius-pill);
  font-size: var(--text-xs, 0.75rem);
  font-weight: 600;
  white-space: nowrap;
  background: color-mix(in oklab, var(--color-muted, var(--muted)) 14%, transparent);
}

.review-badge--pass {
  background: color-mix(in oklab, var(--color-success) 14%, transparent);
  color: var(--color-success-strong);
}

.review-badge--block {
  background: color-mix(in oklab, var(--color-danger) 14%, transparent);
  color: var(--color-danger-strong);
}

.review-badge__commit {
  font-family: var(--font-mono);
  font-weight: 400;
}
</style>
