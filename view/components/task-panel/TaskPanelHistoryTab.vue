<template>
  <div class="task-panel__tab-panel">
    <div class="task-panel__tab-actions">
      <ReloadButton
        class="task-panel__tab-action"
        variant="ghost"
        :disabled="mode !== 'edit'"
        label="Reload history"
        title="Reload history"
        @click="$emit('reload')"
      />
    </div>
    <template v-if="mode === 'edit'">
      <TaskPanelActivityFilter
        v-if="filterGroups.length"
        label="Filter history"
        :groups="filterGroups"
        :hidden-text="hiddenText"
        @toggle="onToggle"
        @show-all="filter.showAll"
      />
      <div class="task-panel__history-scroll" role="region" aria-label="Task history">
        <div class="task-panel__history">
          <h4>Recent updates</h4>
          <ul class="task-panel__history-list">
            <li
              v-for="entry in visibleLog"
              :key="`${entry.at}-${entry.actor || 'unknown'}`"
              class="task-panel__history-item"
            >
              <div class="task-panel__history-meta">
                <time>{{ formatDate(entry.at) }}</time>
                <span v-if="entry.actor" class="task-panel__history-actor">{{ entry.actor }}</span>
              </div>
              <ul class="task-panel__history-changes">
                <li v-for="(change, idx) in entry.changes" :key="idx" class="task-panel__history-change">
                  <strong>{{ formatFieldName(change.field) }}</strong>
                  <span class="task-panel__history-change-values">
                    <span v-if="formatChangeValue(change.old)" class="task-panel__history-old">{{
                      formatChangeValue(change.old)
                    }}</span>
                    <span v-if="formatChangeValue(change.old) && formatChangeValue(change.new)" aria-hidden="true"
                      >→</span
                    >
                    <span v-if="formatChangeValue(change.new)" class="task-panel__history-new">{{
                      formatChangeValue(change.new)
                    }}</span>
                    <span v-else-if="!formatChangeValue(change.old)" class="task-panel__history-new">Set</span>
                  </span>
                </li>
              </ul>
            </li>
            <li v-if="!changeLog.length" class="muted">No updates yet</li>
            <li v-else-if="!visibleLog.length" class="muted">All updates are hidden by the filter</li>
          </ul>
        </div>
      </div>
    </template>
    <p v-else class="task-panel__empty-hint">History will appear after the task is created.</p>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { TaskHistoryChange, TaskHistoryEntry } from '../../api/types'
import { HISTORY_FILTER_KEY, useActivityFilter } from '../../composables/task-panel/useActivityFilter'
import ReloadButton from '../ReloadButton.vue'
import TaskPanelActivityFilter, { type ActivityFilterGroup } from './TaskPanelActivityFilter.vue'

const props = defineProps<{
  mode: 'create' | 'edit'
  changeLog: TaskHistoryEntry[]
  formatDate: (input: string) => string
  formatFieldName: (field: string) => string
  formatChangeValue: (value?: string | null) => string
}>()

defineEmits<{ (e: 'reload'): void }>()

/** Field groups the History tab filters by; every other field counts as "other". */
const FIELD_GROUPS = [
  { value: 'status', label: 'Status' },
  { value: 'assignee', label: 'Assignee' },
  { value: 'tags', label: 'Tags' },
  { value: 'other', label: 'Other' },
]

const fieldGroup = (change: TaskHistoryChange) =>
  FIELD_GROUPS.some((group) => group.value === change.field) ? change.field : 'other'

const filter = useActivityFilter(HISTORY_FILTER_KEY)

/** Entries by shown authors, each with only its changes to shown fields; entries left empty drop out. */
const visibleLog = computed(() =>
  props.changeLog
    .filter((entry) => !filter.hiddenAuthors.value.has(entry.actor || ''))
    .map((entry) => ({
      ...entry,
      changes: entry.changes.filter((change) => !filter.hiddenFields.value.has(fieldGroup(change))),
    }))
    .filter((entry) => entry.changes.length),
)

/** A group of chips for each kind of entry the task has, even a single one, so an automation-only history can hide it. */
const filterGroups = computed<ActivityFilterGroup[]>(() => {
  const groups: ActivityFilterGroup[] = []
  const fields = new Set(props.changeLog.flatMap((entry) => entry.changes.map(fieldGroup)))
  const fieldItems = FIELD_GROUPS.filter((group) => fields.has(group.value)).map((group) => ({
    ...group,
    hidden: filter.hiddenFields.value.has(group.value),
  }))
  if (fieldItems.length) {
    groups.push({ id: 'field', name: 'Field', items: fieldItems })
  }
  const authors = [...new Set(props.changeLog.map((entry) => entry.actor || ''))]
  const authorItems = authors.map((author) => ({
    value: author,
    label: author || 'Unknown',
    hidden: filter.hiddenAuthors.value.has(author),
  }))
  if (authorItems.length) {
    groups.push({ id: 'author', name: 'Author', items: authorItems })
  }
  return groups
})

/** Counts changes, not updates: hiding a field can leave an update shown with some of its changes hidden. */
const hiddenText = computed(() => {
  const countChanges = (log: TaskHistoryEntry[]) => log.reduce((sum, entry) => sum + entry.changes.length, 0)
  const total = countChanges(props.changeLog)
  const hidden = total - countChanges(visibleLog.value)
  if (!hidden) return ''
  return `${hidden} of ${total} ${total === 1 ? 'change' : 'changes'} hidden.`
})

function onToggle(group: string, value: string) {
  if (group === 'field') filter.toggleField(value)
  else filter.toggleAuthor(value)
}
</script>
