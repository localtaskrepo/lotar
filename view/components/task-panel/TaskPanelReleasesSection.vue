<template>
  <fieldset class="task-panel__group task-panel-releases" data-testid="task-panel-releases">
    <legend>Releases</legend>
    <div v-if="loading && !releaseList.length" class="task-panel-releases__loading">
      <UiLoader size="sm">Loading releases…</UiLoader>
    </div>
    <template v-else>
      <ul v-if="assigned.length" class="task-panel-releases__chips" aria-label="Releases">
        <li
          v-for="entry in assigned"
          :key="entry.id"
          class="task-panel-releases__chip"
          :class="{
            'task-panel-releases__chip--released': entry.state === 'released',
            'task-panel-releases__chip--missing': entry.missing,
          }"
        >
          <span>{{ entry.label }}</span>
          <button
            type="button"
            class="task-panel-releases__remove"
            :aria-label="`Remove from ${entry.label}`"
            :title="`Remove from ${entry.label}`"
            :disabled="disabled"
            @click="remove(entry.id)"
          >
            <IconGlyph name="close" aria-hidden="true" />
          </button>
        </li>
      </ul>
      <p v-else class="muted task-panel-releases__empty">Not in a release.</p>
      <div v-if="choices.length" class="task-panel-releases__picker">
        <select
          v-model="selection"
          class="input task-panel-releases__select"
          aria-label="Release to add"
          :disabled="disabled"
        >
          <option value="">Add to release…</option>
          <option v-for="choice in choices" :key="choice.id" :value="String(choice.id)">{{ choice.label }}</option>
        </select>
        <UiButton type="button" :disabled="disabled || !selection" @click="add">
          <IconGlyph name="plus" aria-hidden="true" />
          <span>Add</span>
        </UiButton>
      </div>
      <p v-else-if="!releaseList.length" class="muted task-panel-releases__empty">
        No releases yet. Create one on the Releases page.
      </p>
    </template>
  </fieldset>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { ReleaseListItem } from '../../api/types'
import { releaseLabel } from '../../composables/useReleases'
import IconGlyph from '../IconGlyph.vue'
import UiButton from '../UiButton.vue'
import UiLoader from '../UiLoader.vue'

const props = withDefaults(
  defineProps<{
    releases: number[]
    releaseList: ReleaseListItem[]
    loading?: boolean
    disabled?: boolean
  }>(),
  { loading: false, disabled: false },
)

const emit = defineEmits<{ (e: 'update:releases', value: number[]): void }>()

const selection = ref('')

const lookup = computed(() => new Map(props.releaseList.map((release) => [release.id, release])))

// Version order, as the server lists releases; unknown ids last.
const assigned = computed(() => {
  const order = new Map(props.releaseList.map((release, index) => [release.id, index]))
  return [...new Set(props.releases)]
    .sort((a, b) => (order.get(a) ?? Number.MAX_SAFE_INTEGER) - (order.get(b) ?? Number.MAX_SAFE_INTEGER) || a - b)
    .map((id) => {
      const release = lookup.value.get(id)
      return {
        id,
        label: release ? releaseLabel(release) : `#${id} (missing)`,
        state: release?.state ?? 'missing',
        missing: !release,
      }
    })
})

const choices = computed(() =>
  props.releaseList
    .filter((release) => !props.releases.includes(release.id))
    .map((release) => ({
      id: release.id,
      label: release.state === 'released' ? `${releaseLabel(release)} (released)` : releaseLabel(release),
    })),
)

function add() {
  const id = Number(selection.value)
  selection.value = ''
  if (!Number.isInteger(id) || id <= 0 || props.releases.includes(id)) return
  emit('update:releases', [...props.releases, id])
}

function remove(id: number) {
  emit(
    'update:releases',
    props.releases.filter((value) => value !== id),
  )
}
</script>

<style scoped>
.task-panel-releases__loading {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  color: var(--color-muted);
}

.task-panel-releases__chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2, 0.5rem);
  margin: 0 0 var(--space-2, 0.5rem);
  padding: 0;
  list-style: none;
}

.task-panel-releases__chip {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1, 0.25rem);
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
  border-radius: var(--radius-pill);
  font-size: var(--text-xs, 0.75rem);
  background: color-mix(in oklab, var(--color-accent) 14%, transparent);
  color: var(--color-accent-strong);
  border: 1px solid color-mix(in oklab, var(--color-accent) 35%, transparent);
}

.task-panel-releases__chip--released {
  background: color-mix(in oklab, var(--color-success) 14%, transparent);
  color: var(--color-success-strong);
  border-color: color-mix(in oklab, var(--color-success) 35%, transparent);
}

.task-panel-releases__chip--missing {
  background: color-mix(in oklab, var(--color-danger) 12%, transparent);
  color: var(--color-danger);
  border-color: color-mix(in oklab, var(--color-danger) 35%, transparent);
}

.task-panel-releases__remove {
  border: 0;
  background: transparent;
  color: inherit;
  padding: 0;
  display: inline-flex;
  align-items: center;
  cursor: pointer;
}

.task-panel-releases__remove .icon-glyph {
  width: 0.75rem;
  height: 0.75rem;
}

.task-panel-releases__remove:hover:not(:disabled),
.task-panel-releases__remove:focus-visible {
  color: var(--color-danger);
}

.task-panel-releases__remove:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.task-panel-releases__empty {
  margin: 0 0 var(--space-2, 0.5rem);
}

.task-panel-releases__picker {
  display: flex;
  gap: var(--space-2, 0.5rem);
  align-items: center;
  flex-wrap: wrap;
}

.task-panel-releases__select {
  flex: 1 1 160px;
  min-width: 0;
}
</style>
