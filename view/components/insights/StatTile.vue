<template>
  <component
    :is="clickable ? 'button' : 'div'"
    :type="clickable ? 'button' : undefined"
    class="stat-tile"
    :class="{ 'stat-tile--clickable': clickable, [`stat-tile--${tone}`]: tone !== 'neutral' }"
    :title="hint || undefined"
    :aria-describedby="hint && (delta || caption) ? hintId : undefined"
    @click="clickable && emit('select')"
  >
    <span class="stat-tile__label">
      <IconGlyph v-if="tone === 'alert'" name="flag" class="stat-tile__icon" aria-hidden="true" />
      {{ label }}
    </span>
    <span class="stat-tile__value">{{ value }}</span>
    <span v-if="delta" class="stat-tile__delta" :class="`stat-tile__delta--${delta.tone}`">
      <span aria-hidden="true">{{ delta.direction === 'up' ? '▲' : delta.direction === 'down' ? '▼' : '•' }}</span>
      {{ delta.text }}
    </span>
    <span v-else-if="caption || hint" class="stat-tile__caption">{{ caption || hint }}</span>
    <span v-if="hint && (delta || caption)" :id="hintId" class="sr-only">{{ hint }}</span>
  </component>
</template>

<script setup lang="ts">
import { useId } from 'vue'
import IconGlyph from '../IconGlyph.vue'

export interface StatDelta {
  /** e.g. "12% vs previous 30 days" */
  text: string
  direction: 'up' | 'down' | 'flat'
  /** Whether the change is good news, bad news or neither. */
  tone: 'good' | 'bad' | 'neutral'
}

withDefaults(
  defineProps<{
    label: string
    value: string
    caption?: string
    hint?: string
    delta?: StatDelta | null
    /** `alert` marks a value that needs attention (with an icon, never colour alone). */
    tone?: 'neutral' | 'alert'
    clickable?: boolean
  }>(),
  { caption: '', hint: '', delta: null, tone: 'neutral', clickable: false },
)

const emit = defineEmits<{ (e: 'select'): void }>()
const hintId = useId()
</script>

<style scoped>
.stat-tile {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-1, 0.25rem);
  min-width: 0;
  padding: var(--space-3, 0.75rem) var(--space-4, 1rem);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md, 0.375rem);
  background: var(--color-surface);
  color: inherit;
  font: inherit;
  text-align: left;
}

.stat-tile--clickable {
  cursor: pointer;
}

.stat-tile--clickable:hover {
  border-color: var(--color-border-strong);
}

.stat-tile--clickable:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.stat-tile--alert {
  border-color: color-mix(in oklab, var(--color-danger) 55%, var(--color-border));
}

.stat-tile__label {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1, 0.25rem);
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted);
}

.stat-tile__icon {
  color: var(--color-danger);
}

.stat-tile__value {
  font-size: 1.75rem;
  font-weight: 600;
  line-height: 1.1;
  color: var(--color-fg);
}

.stat-tile__delta,
.stat-tile__caption {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}

.stat-tile__delta--good {
  color: var(--color-success-strong);
}

.stat-tile__delta--bad {
  color: var(--color-danger-strong);
}
</style>
