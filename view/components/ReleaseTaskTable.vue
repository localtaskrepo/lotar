<template>
  <div class="release-table-wrapper">
    <table class="release-table">
      <caption class="release-table__caption">
        {{
          caption
        }}
      </caption>
      <thead>
        <tr>
          <th class="release-table__check">
            <input
              type="checkbox"
              :aria-label="`Select all: ${caption}`"
              :checked="allSelected"
              :indeterminate.prop="someSelected && !allSelected"
              @change="emit('toggle-all', taskIds, source, ($event.target as HTMLInputElement).checked)"
            />
          </th>
          <th>ID</th>
          <th>Title</th>
          <th>Status</th>
          <th class="release-table__assignee">Assignee</th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="task in tasks"
          :key="task.id"
          class="release-row"
          :class="{
            'release-row--done': task.task_state?.is_done,
            'release-row--dragging': draggingId === task.id,
            'release-row--selected': isSelected(task.id, source),
          }"
          draggable="true"
          :data-task-id="task.id"
          @dragstart="emit('dragstart', $event, task.id, source)"
          @dragend="emit('dragend')"
          @click="onRowClick($event, task)"
        >
          <td class="release-table__check">
            <input
              type="checkbox"
              :aria-label="`Select ${task.id}`"
              :checked="isSelected(task.id, source)"
              @change="emit('toggle', task.id, source)"
            />
          </td>
          <td class="release-table__id">
            <span class="muted">{{ projectOf(task.id) }}</span
            ><strong>-{{ numericOf(task.id) }}</strong>
          </td>
          <td class="release-table__title">
            <button type="button" class="release-table__open" @click.stop="emit('open', task)">
              {{ task.title || 'Untitled task' }}
            </button>
          </td>
          <td class="release-table__status">{{ task.status }}</td>
          <td class="release-table__assignee">
            <span v-if="task.assignee">{{ formatMember(task.assignee) }}</span>
            <span v-else class="muted">—</span>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { TaskDTO } from '../api/types'
import { formatMember } from '../utils/member'
import { numericOf, projectOf } from '../utils/text'

/** The tickets of one release, or of the backlog, on the Releases page (DEV-393). */
const props = defineProps<{
  tasks: TaskDTO[]
  /** The release these rows belong to; null for the backlog. */
  source: number | null
  caption: string
  isSelected: (task: string, source: number | null) => boolean
  draggingId: string | null
}>()

const emit = defineEmits<{
  (e: 'toggle', task: string, source: number | null): void
  (e: 'toggle-all', tasks: string[], source: number | null, checked: boolean): void
  (e: 'open', task: TaskDTO): void
  (e: 'dragstart', event: DragEvent, task: string, source: number | null): void
  (e: 'dragend'): void
}>()

const taskIds = computed(() => props.tasks.map((task) => task.id))
const selectedCount = computed(() => props.tasks.filter((task) => props.isSelected(task.id, props.source)).length)
const allSelected = computed(() => props.tasks.length > 0 && selectedCount.value === props.tasks.length)
const someSelected = computed(() => selectedCount.value > 0)

function onRowClick(event: MouseEvent, task: TaskDTO) {
  const target = event.target as HTMLElement | null
  if (target?.closest('button, a, input, select, textarea')) return
  emit('open', task)
}
</script>

<style scoped>
.release-table-wrapper {
  width: 100%;
  overflow-x: auto;
}

.release-table {
  width: 100%;
  border-collapse: collapse;
}

.release-table__caption {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}

.release-table th,
.release-table td {
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border-bottom: 1px solid var(--color-border, var(--border));
  vertical-align: middle;
  text-align: left;
}

.release-table th {
  font-weight: 600;
  color: var(--color-muted, var(--muted));
  white-space: nowrap;
}

.release-table__check {
  width: 28px;
}

.release-table__id {
  white-space: nowrap;
  width: 110px;
}

.release-table__title {
  width: 100%;
  min-width: 160px;
}

.release-table__status {
  white-space: nowrap;
}

.release-table__assignee {
  white-space: nowrap;
}

.release-table__open {
  border: 0;
  background: transparent;
  padding: 0;
  font: inherit;
  color: inherit;
  text-align: left;
  cursor: pointer;
  overflow-wrap: anywhere;
}

.release-table__open:hover,
.release-table__open:focus-visible {
  text-decoration: underline;
}

.release-row {
  cursor: grab;
  transition: background var(--duration-med) var(--ease-standard);
}

.release-row:hover {
  background: color-mix(in oklab, var(--color-border) 40%, transparent);
}

.release-row--selected {
  background: color-mix(in oklab, var(--color-accent) 10%, transparent);
}

.release-row--done .release-table__open {
  color: var(--color-muted);
  text-decoration: line-through;
}

.release-row--dragging {
  opacity: 0.6;
}

@media (max-width: 560px) {
  .release-table__assignee {
    display: none;
  }

  .release-table__title {
    min-width: 0;
  }

  .release-table__id {
    width: auto;
  }

  .release-table th,
  .release-table td {
    padding: var(--space-2, 0.5rem);
  }
}
</style>
