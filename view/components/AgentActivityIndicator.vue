<template>
  <span
    v-if="activities.length"
    :class="['agent-activity', { 'agent-activity--queued': !running }]"
    role="img"
    :aria-label="description"
    :title="description"
    data-testid="agent-activity"
  >
    <IconGlyph name="agent" class="agent-activity__icon" aria-hidden="true" />
    <span v-if="label" class="agent-activity__label">{{ running ? 'Working' : 'Queued' }}</span>
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { describeAgentActivity, useAgentActivity } from '../composables/useAgentActivity'
import IconGlyph from './IconGlyph.vue'

/**
 * Shows that an agent is working on (or queued for) a task (DEV-249).
 * Hovering says which agent and what it reported last.
 */
const props = withDefaults(defineProps<{ taskId: string | null | undefined; label?: boolean }>(), {
  label: false,
})

const { activityFor } = useAgentActivity()
const activities = activityFor(() => props.taskId)
const running = computed(() => activities.value.some((activity) => activity.status === 'running'))
const description = computed(() => describeAgentActivity(activities.value))
</script>

<style scoped>
.agent-activity {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  vertical-align: middle;
  margin-left: 6px;
  padding: 1px 6px;
  border-radius: var(--radius-pill, 999px);
  color: var(--color-accent-strong, var(--color-accent));
  background: color-mix(in oklab, var(--color-accent) 14%, transparent);
  font-size: var(--text-xs, 0.75rem);
  font-weight: 600;
  line-height: 1.4;
  cursor: help;
  white-space: nowrap;
}

.agent-activity__icon {
  width: 14px;
  height: 14px;
  animation: agent-activity-pulse 1.6s ease-in-out infinite;
}

.agent-activity--queued {
  color: var(--color-muted);
  background: color-mix(in oklab, var(--color-muted) 14%, transparent);
}

.agent-activity--queued .agent-activity__icon {
  animation: none;
}

@keyframes agent-activity-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.45;
  }
}

@media (prefers-reduced-motion: reduce) {
  .agent-activity__icon {
    animation: none;
  }
}
</style>
