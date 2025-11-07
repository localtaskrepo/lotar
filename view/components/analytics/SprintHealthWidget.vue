<template>
  <div v-if="loading" class="sprint-health__loading">
    <UiLoader size="sm" />
    <span>Loading sprint health…</span>
  </div>
  <p v-else-if="error" class="sprint-health__error">{{ error }}</p>
  <div class="sprint-health" v-else-if="summary">
    <section class="sprint-health__section">
      <header>
        <h3>Overview</h3>
        <span class="badge" :class="statusClass(summary.lifecycle.state)">{{ summary.lifecycle.state }}</span>
      </header>
      <dl class="sprint-health__grid">
        <div>
          <dt>Status</dt>
          <dd>{{ summary.lifecycle.status }}</dd>
        </div>
        <div>
          <dt>Goal</dt>
          <dd>{{ summary.sprint.goal || '—' }}</dd>
        </div>
        <div>
          <dt>Start</dt>
          <dd>{{ formatDate(summary.lifecycle.actual_start || summary.lifecycle.planned_start) }}</dd>
        </div>
        <div>
          <dt>End (planned)</dt>
          <dd>{{ formatDate(summary.lifecycle.planned_end) }}</dd>
        </div>
        <div>
          <dt>End (actual)</dt>
          <dd>{{ formatDate(summary.lifecycle.actual_end || summary.lifecycle.computed_end) }}</dd>
        </div>
        <div>
          <dt>Blocked tasks</dt>
          <dd>{{ summary.blocked_tasks.length }}</dd>
        </div>
      </dl>
      <div v-if="summary.sprint.has_warnings && summary.sprint.status_warnings.length" class="sprint-health__warnings">
        <h4>Warnings</h4>
        <ul>
          <li v-for="warning in summary.sprint.status_warnings" :key="warning.code">
            <strong>{{ warning.code }}</strong>: {{ warning.message }}
          </li>
        </ul>
      </div>
    </section>

    <section class="sprint-health__section">
      <header>
        <h3>Throughput</h3>
      </header>
      <div class="sprint-health__metrics">
        <article class="metric">
          <h4>Tasks</h4>
          <p class="metric__value">{{ formatNumber(summary.metrics.tasks.done) }} / {{ formatNumber(summary.metrics.tasks.committed) }}</p>
          <p class="metric__meta">{{ formatPercent(summary.metrics.tasks.completion_ratio) }} complete</p>
        </article>
        <article v-if="summary.metrics.points" class="metric">
          <h4>Points</h4>
          <p class="metric__value">{{ formatNumber(summary.metrics.points.done) }} / {{ formatNumber(summary.metrics.points.committed) }}</p>
          <p class="metric__meta">
            {{ formatPercent(summary.metrics.points.completion_ratio) }} complete
            <span v-if="summary.metrics.points.capacity">(capacity {{ formatNumber(summary.metrics.points.capacity) }})</span>
          </p>
        </article>
        <article v-if="summary.metrics.hours" class="metric">
          <h4>Hours</h4>
          <p class="metric__value">{{ formatNumber(summary.metrics.hours.done) }} / {{ formatNumber(summary.metrics.hours.committed) }}</p>
          <p class="metric__meta">
            {{ formatPercent(summary.metrics.hours.completion_ratio) }} complete
            <span v-if="summary.metrics.hours.capacity">(capacity {{ formatNumber(summary.metrics.hours.capacity) }})</span>
          </p>
        </article>
      </div>
      <p v-if="summary.metrics.blocked" class="sprint-health__blocked-count">
        {{ summary.metrics.blocked }} task{{ summary.metrics.blocked === 1 ? '' : 's' }} currently blocked.
      </p>
    </section>

    <section class="sprint-health__section">
      <header>
        <h3>Timeline</h3>
      </header>
      <dl class="sprint-health__grid">
        <div>
          <dt>Planned duration</dt>
          <dd>{{ formatDuration(summary.timeline.planned_duration_days) }}</dd>
        </div>
        <div>
          <dt>Actual duration</dt>
          <dd>{{ formatDuration(summary.timeline.actual_duration_days) }}</dd>
        </div>
        <div>
          <dt>Elapsed</dt>
          <dd>{{ formatDuration(summary.timeline.elapsed_days) }}</dd>
        </div>
        <div>
          <dt>Remaining</dt>
          <dd>{{ formatDuration(summary.timeline.remaining_days) }}</dd>
        </div>
        <div>
          <dt>Overdue</dt>
          <dd>{{ formatDuration(summary.timeline.overdue_days) }}</dd>
        </div>
      </dl>
    </section>

    <section v-if="summary.blocked_tasks.length" class="sprint-health__section">
      <header>
        <h3>Blocked tasks</h3>
      </header>
      <ul class="sprint-health__blocked">
        <li v-for="task in summary.blocked_tasks" :key="task.id">
          <span>{{ task.title }}</span>
          <span class="muted">{{ task.status }}</span>
        </li>
      </ul>
    </section>
  </div>
  <p v-else class="sprint-health__empty">Select a sprint to view health metrics.</p>
</template>

<script setup lang="ts">
import { toRefs } from 'vue';
import type { SprintSummaryReportResponse } from '../../api/types';

import UiLoader from '../UiLoader.vue';

const props = withDefaults(
  defineProps<{ summary?: SprintSummaryReportResponse; loading?: boolean; error?: string | null }>(),
  {
    summary: undefined,
    loading: false,
    error: null,
  },
)

const { summary, loading, error } = toRefs(props)

function statusClass(state: string) {
  const lowered = state?.toLowerCase()
  if (lowered === 'active') return 'badge--info'
  if (lowered === 'overdue') return 'badge--danger'
  if (lowered === 'complete') return 'badge--success'
  return 'badge--muted'
}

function formatDate(value?: string | null) {
  if (!value) return '—'
  try {
    return new Date(value).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
  } catch {
    return value
  }
}

function formatNumber(value: number | null | undefined) {
  if (value === null || value === undefined || Number.isNaN(value)) return '0'
  return value.toLocaleString(undefined, { maximumFractionDigits: 1 })
}

function formatPercent(ratio: number | null | undefined) {
  if (ratio === null || ratio === undefined || Number.isNaN(ratio)) return '0%'
  return `${Math.round(ratio * 100)}%`
}

function formatDuration(days: number | null | undefined) {
  if (days === null || days === undefined || Number.isNaN(days)) return '—'
  if (Math.abs(days) < 0.1) return '< 1 day'
  return `${days.toFixed(1)} days`
}
</script>

<style scoped>
.sprint-health {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.sprint-health__loading {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--color-muted, #64748b);
  font-size: 0.95rem;
}

.sprint-health__error {
  color: var(--color-danger, #ef4444);
  font-size: 0.95rem;
}

.sprint-health__section {
  border: 1px solid var(--color-border, #e2e8f0);
  border-radius: var(--radius-md, 6px);
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.sprint-health__section > header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sprint-health__grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 12px;
}

.sprint-health__grid dt {
  font-size: 0.75rem;
  text-transform: uppercase;
  color: var(--color-muted, #64748b);
}

.sprint-health__grid dd {
  margin: 0;
  font-weight: 500;
}

.sprint-health__warnings ul,
.sprint-health__blocked {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.sprint-health__blocked li {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}

.sprint-health__metrics {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}

.metric {
  flex: 1 1 160px;
  border: 1px solid var(--color-border, #e2e8f0);
  border-radius: var(--radius-sm, 4px);
  padding: 8px 12px;
  display: flex;
  align-items: flex-start;
  flex-direction: column;
  gap: 4px;
}

.metric__value {
  font-size: 1.1rem;
  margin: 0;
}

.metric__meta {
  margin: 0;
  font-size: 0.85rem;
  color: var(--color-muted, #64748b);
}

.sprint-health__blocked-count {
  font-size: 0.9rem;
  color: var(--color-muted, #64748b);
}

.sprint-health__empty {
  color: var(--color-muted, #64748b);
  font-size: 0.95rem;
}

.badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 0.75rem;
  text-transform: uppercase;
}

.badge--info { background: color-mix(in oklab, var(--color-accent, #6366f1) 15%, transparent); color: var(--color-accent, #6366f1); }
.badge--danger { background: color-mix(in oklab, var(--color-danger, #ef4444) 15%, transparent); color: var(--color-danger, #ef4444); }
.badge--success { background: color-mix(in oklab, var(--color-success, #10b981) 15%, transparent); color: var(--color-success, #10b981); }
.badge--muted { background: color-mix(in oklab, var(--color-muted, #64748b) 15%, transparent); color: var(--color-muted, #64748b); }
</style>
