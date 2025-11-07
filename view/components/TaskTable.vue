<template>
  <div class="table-wrap" ref="rootRef">
    <div class="table-toolbar row" style="justify-content: space-between; align-items: center; margin-bottom: 8px; position: relative;">
      <div class="row columns-control" style="gap:8px; align-items:center; position: relative;">
        <strong>Columns</strong>
        <button class="btn" @click="toggleColumnMenu">Configure</button>
        <div v-if="showColumnMenu" class="columns-popover card" @click.self="showColumnMenu=false">
          <div class="col" style="gap:6px;">
            <label v-for="col in allColumns" :key="col" class="row" style="gap:6px; align-items:center;">
              <input type="checkbox" :checked="columnsSet.has(col)" @change="toggleColumn(col, $event)" />
              <span>{{ headerLabel(col) }}</span>
            </label>
            <div class="row" style="gap:6px; margin-top: 6px;">
              <button class="btn" @click="showColumnMenu=false">Close</button>
              <button class="btn" @click="resetColumns">Reset</button>
            </div>
          </div>
        </div>
      </div>
      <div class="row controls" style="gap:8px; align-items:center; flex-wrap: wrap;">
        <!-- Bulk controls and Add placed in same toolbar row -->
        <label class="row" style="gap:6px; align-items:center;">
          <input type="checkbox" :checked="bulk" @change="onToggleBulk($event)" /> Bulk select
        </label>
        <span v-if="bulk" class="muted">Selected: {{ selected.length }}</span>
        <div v-if="bulk" class="bulk-menu-wrapper">
          <button class="btn" type="button" title="Bulk actions" :disabled="disableBulkActions" ref="bulkMenuButton" @click.stop="toggleBulkMenu">⋯</button>
          <div v-if="showBulkMenu" class="menu-popover card bulk-actions-menu" ref="bulkMenuPopover">
            <button class="menu-item" type="button" :disabled="disableBulkActions" @click="handleBulkAction('assign')">Assign…</button>
            <button class="menu-item" type="button" :disabled="disableBulkActions" @click="handleBulkAction('unassign')">Clear assignee</button>
            <button class="menu-item" type="button" :disabled="disableSprintActions" @click="handleBulkAction('sprint-add')">Add to sprint…</button>
            <button class="menu-item" type="button" :disabled="disableSprintActions" @click="handleBulkAction('sprint-remove')">Remove from sprint…</button>
            <button class="menu-item danger" type="button" :disabled="disableBulkActions" @click="handleBulkAction('delete')">Delete tasks…</button>
            <div class="menu-separator" role="separator"></div>
            <button class="menu-item" type="button" @click="openBacklog">Open sprint backlog</button>
          </div>
        </div>
  <button class="btn primary" @click="$emit('add')">Add</button>
      </div>
    </div>

    <UiCard>
      <div class="table-scroll">
        <table class="table">
        <thead>
          <tr>
            <th v-if="selectable" style="width:28px;">
              <input aria-label="Select all" ref="selectAllRef" type="checkbox" :checked="allSelected" @change="toggleAll($event)" />
            </th>
            <th
              v-for="col in visibleColumns"
              :key="col"
              :class="['sortable', { active: sort.key === col }]"
              :aria-sort="sort.key === col ? (sort.dir === 'asc' ? 'ascending' : 'descending') : 'none'"
            >
              <button
                class="header-button"
                type="button"
                @click="onSort(col)"
                @keydown.enter.prevent="onSort(col)"
                @keydown.space.prevent="onSort(col)"
              >
                {{ headerLabel(col) }}
              </button>
              <div class="sort-controls" role="group" aria-label="Sorting">
                <button
                  class="sort-control"
                  type="button"
                  aria-label="Sort ascending"
                  :aria-pressed="sort.key === col && sort.dir === 'asc'"
                  :class="{ 'sort-control--active': sort.key === col && sort.dir === 'asc' }"
                  :disabled="sort.key === col && sort.dir === 'asc'"
                  @click.stop="setSort(col, 'asc')"
                >▲</button>
                <button
                  class="sort-control"
                  type="button"
                  aria-label="Sort descending"
                  :aria-pressed="sort.key === col && sort.dir === 'desc'"
                  :class="{ 'sort-control--active': sort.key === col && sort.dir === 'desc' }"
                  :disabled="sort.key === col && sort.dir === 'desc'"
                  @click.stop="setSort(col, 'desc')"
                >▼</button>
              </div>
            </th>
            <th style="width: 1%; white-space: nowrap;">Actions</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="t in sorted" :key="t.id" :class="{ 'is-recent': !!touchesMap[t.id] }" @click="$emit('open', t.id)">
            <td v-if="selectable" @click.stop style="text-align:center;">
              <input type="checkbox" :checked="isSelected(t.id)" @change="toggleOne(t.id, $event)" />
            </td>
            <td v-for="col in visibleColumns" :key="col" :class="['task-table__cell', `task-table__cell--${col}`]">
              <template v-if="col === 'id'">
                <span class="muted">{{ projectOf(t.id) }}</span>
                <strong style="margin-left: 6px;">{{ numericOf(t.id) }}</strong>
              </template>
              <template v-else-if="col === 'title'">
                <div class="task-table__title-wrapper">
                  <span class="task-table__title-text">{{ t.title }}</span>
                  <div v-if="touchesMap[t.id]" class="session-touch">
                    <span class="session-touch__badge" :class="touchesMap[t.id].kind">{{ touchBadge(touchesMap[t.id]) }}</span>
                    <span>{{ relativeTime(touchesMap[t.id].time) }}</span>
                    <span v-if="touchesMap[t.id].actor">• {{ touchesMap[t.id].actor }}</span>
                  </div>
                </div>
              </template>
              <template v-else-if="col === 'status'">
                <span class="status" :data-status="t.status">{{ t.status }}</span>
              </template>
              <template v-else-if="col === 'priority'">
                <span>{{ t.priority }}</span>
              </template>
              <template v-else-if="col === 'task_type'">
                <span>{{ t.task_type }}</span>
              </template>
              <template v-else-if="col === 'reporter'">
                <span v-if="t.reporter">@{{ t.reporter }}</span>
                <span v-else class="muted">—</span>
              </template>
              <template v-else-if="col === 'assignee'">
                <span v-if="t.assignee">@{{ t.assignee }}</span>
                <span v-else class="muted">—</span>
              </template>
              <template v-else-if="col === 'effort'">
                <span v-if="(t as any).effort">{{ (t as any).effort }}</span>
                <span v-else class="muted">—</span>
              </template>
              <template v-else-if="col === 'tags'">
                <div class="row" style="gap:6px; flex-wrap: wrap; align-items:center;">
                  <span v-for="tag in (t.tags || [])" :key="tag" class="chip small">{{ tag }}</span>
                  <input v-if="isEditingTags(t.id)" class="input" v-model="tagsDrafts[t.id]" placeholder="tag1, tag2" style="max-width: 240px;" @click.stop @keyup.enter.prevent="saveTags(t)" />
                </div>
              </template>
              <template v-else-if="col === 'sprints'">
                <div class="row" style="gap:6px; flex-wrap: wrap; align-items:center;">
                  <span
                    v-for="sprintId in (t.sprints || [])"
                    :key="`${t.id}-sprint-${sprintId}`"
                    class="chip small"
                    :class="sprintStateClass(sprintId)"
                  >
                    {{ sprintLabel(sprintId) }}
                  </span>
                  <span v-if="!(t.sprints && t.sprints.length)" class="muted">—</span>
                </div>
              </template>
              <template v-else-if="col === 'due_date'">
                <span v-if="t.due_date" :class="{ overdue: isOverdue(t) }">{{ fmtDate(t.due_date) }}</span>
                <span v-else class="muted">—</span>
              </template>
              <template v-else-if="col === 'modified'">
                <span :title="fmtDateTime(t.modified)">{{ relativeTime(t.modified) }}</span>
                <span v-if="touchesMap[t.id]?.actor" class="muted"> • {{ touchesMap[t.id]?.actor }}</span>
              </template>
              <template v-else>
                <span>{{ (t as any)[col] }}</span>
              </template>
            </td>
            <td class="actions-cell" @click.stop>
              <button class="btn" aria-label="Row actions" title="Actions" @click.stop="toggleRowMenu(t.id)">⋯</button>
              <div v-if="isRowMenuOpen(t.id)" class="menu-popover card">
                <button class="menu-item" v-if="!isEditingTags(t.id)" @click="toggleTagsEdit(t.id); closeRowMenu(t.id)">Edit tags</button>
                <button class="menu-item" v-else @click="saveTags(t); closeRowMenu(t.id)">Save tags</button>
                <button class="menu-item" @click="$emit('assign', t.id); closeRowMenu(t.id)">Assign…</button>
                <button class="menu-item" @click="$emit('unassign', t.id); closeRowMenu(t.id)">Clear assignee</button>
                <button class="menu-item" @click="$emit('sprint-add', t.id); closeRowMenu(t.id)">Add to sprint…</button>
                <button class="menu-item" @click="$emit('sprint-remove', t.id); closeRowMenu(t.id)">Remove from sprint…</button>
                <button class="menu-item danger" @click="$emit('delete', t.id); closeRowMenu(t.id)">Delete…</button>
              </div>
            </td>
          </tr>
          <tr v-if="!loading && (!tasks || tasks.length === 0)"><td :colspan="visibleColumns.length + (selectable ? 2 : 1)" class="muted">No tasks</td></tr>
        </tbody>
        </table>
      </div>
    </UiCard>
  </div>
  
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useTaskTableState, type TaskTableEmit, type TaskTableProps } from '../composables/useTaskTableState'
import UiCard from './UiCard.vue'

const props = defineProps<TaskTableProps>()
const emit = defineEmits<TaskTableEmit>()

const {
  allColumns,
  columns,
  columnsSet,
  visibleColumns,
  headerLabel,
  showColumnMenu,
  toggleColumnMenu,
  toggleColumn,
  resetColumns,
  rootRef,
  sort,
  onSort,
  rowMenu,
  toggleRowMenu,
  closeRowMenu,
  isRowMenuOpen,
  sorted,
  touchesMap,
  selected,
  allSelected,
  selectAllRef,
  indeterminate,
  isSelected,
  toggleOne,
  toggleAll,
  onToggleBulk,
  tagsEditing,
  tagsDrafts,
  isEditingTags,
  toggleTagsEdit,
  saveTags,
  projectOf,
  numericOf,
  fmtDate,
  fmtDateTime,
  relativeTime,
  touchBadge,
  isOverdue,
  setSort,
} = useTaskTableState(props, emit)

const sprintLookup = computed(() => props.sprintLookup ?? {})
const hasSprintsLocal = computed(() => props.hasSprints ?? false)
const sprintsLoadingLocal = computed(() => props.sprintsLoading ?? false)

const showBulkMenu = ref(false)
const bulkMenuButton = ref<HTMLElement | null>(null)
const bulkMenuPopover = ref<HTMLElement | null>(null)
const disableBulkActions = computed(() => !selected.value.length)
const disableSprintActions = computed(() => disableBulkActions.value || sprintsLoadingLocal.value || !hasSprintsLocal.value)

function sprintLabel(id: number) {
  const entry = sprintLookup.value[id]
  if (entry?.label) return entry.label
  return `#${id}`
}

function sprintStateClass(id: number) {
  const state = sprintLookup.value[id]?.state?.toLowerCase()
  if (!state) return 'sprint--unknown'
  return `sprint--${state}`
}

function toggleBulkMenu() {
  if (disableBulkActions.value) return
  showBulkMenu.value = !showBulkMenu.value
}

function closeBulkMenu() {
  showBulkMenu.value = false
}

type BulkMenuAction = 'assign' | 'unassign' | 'sprint-add' | 'sprint-remove' | 'delete'

function handleBulkAction(action: BulkMenuAction) {
  if (disableBulkActions.value) {
    closeBulkMenu()
    return
  }
  if ((action === 'sprint-add' || action === 'sprint-remove') && disableSprintActions.value) {
    closeBulkMenu()
    return
  }
  switch (action) {
    case 'assign':
      emit('bulk-assign')
      break
    case 'unassign':
      emit('bulk-unassign')
      break
    case 'sprint-add':
      emit('bulk-sprint-add')
      break
    case 'sprint-remove':
      emit('bulk-sprint-remove')
      break
    case 'delete':
      emit('bulk-delete')
      break
  }
  closeBulkMenu()
}

function openBacklog() {
  emit('open-sprint-backlog')
  closeBulkMenu()
}

function handleOutsideClick(event: MouseEvent) {
  if (!showBulkMenu.value) return
  const target = event.target as Node | null
  if (!target) return
  if (bulkMenuButton.value && bulkMenuButton.value.contains(target)) return
  if (bulkMenuPopover.value && bulkMenuPopover.value.contains(target)) return
  closeBulkMenu()
}

function handleEscape(event: KeyboardEvent) {
  if (event.key === 'Escape') closeBulkMenu()
}

watch(selected, (value) => {
  if (!value.length) closeBulkMenu()
})

watch(
  () => props.bulk,
  (value) => {
    if (!value) closeBulkMenu()
  },
)

watch(sprintsLoadingLocal, (loadingNow) => {
  if (loadingNow) closeBulkMenu()
})

watch(hasSprintsLocal, (hasSprints) => {
  if (!hasSprints) closeBulkMenu()
})

watch(showBulkMenu, (open) => {
  if (open && disableBulkActions.value) {
    showBulkMenu.value = false
  }
})

onMounted(() => {
  if (typeof window !== 'undefined') {
    window.addEventListener('click', handleOutsideClick)
    window.addEventListener('keydown', handleEscape)
  }
})

onUnmounted(() => {
  if (typeof window !== 'undefined') {
    window.removeEventListener('click', handleOutsideClick)
    window.removeEventListener('keydown', handleEscape)
  }
})
</script>

<style scoped>
.table-wrap { width: 100%; }
.table-scroll {
  width: 100%;
  overflow-x: auto;
}
.table-scroll::-webkit-scrollbar {
  height: 8px;
}
.table-scroll::-webkit-scrollbar-track {
  background: transparent;
}
.table-scroll::-webkit-scrollbar-thumb {
  background: color-mix(in oklab, var(--color-border, #e2e8f0) 70%, transparent);
  border-radius: 999px;
}
.table {
  width: 100%;
  border-collapse: collapse;
  min-width: 720px;
}
th,
td {
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border-bottom: 1px solid var(--color-border, var(--border));
  vertical-align: middle;
}

thead th {
  text-align: left;
  font-weight: 600;
  color: var(--color-muted, var(--muted));
  user-select: none;
  cursor: default;
}

th.sortable {
  cursor: default;
}

th.active {
  color: var(--color-fg, var(--fg));
}

.header-button {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1, 0.25rem);
  background: transparent;
  border: none;
  padding: 0;
  font: inherit;
  color: inherit;
  cursor: pointer;
}

.header-button:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring, 0 0 0 1px rgba(14,165,233,0.35), 0 0 0 4px rgba(14,165,233,0.2));
  border-radius: var(--radius-sm, 0.25rem);
}

.sort-controls {
  display: inline-flex;
  margin-left: var(--space-1, 0.25rem);
  border-radius: var(--radius-sm, 0.25rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 85%, transparent);
  overflow: hidden;
}

.sort-control {
  border: none;
  background: transparent;
  font-size: 0.75rem;
  line-height: 1;
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
  cursor: pointer;
  color: var(--color-muted, #6b7280);
}

.sort-control:hover:not(:disabled) {
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 75%, transparent);
}

.sort-control--active {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 18%, transparent);
  color: var(--color-accent, #0ea5e9);
  font-weight: 600;
}

.sort-control:disabled {
  cursor: default;
  opacity: 0.65;
}

.task-table__cell {
  position: relative;
}

.task-table__cell--title {
  width: 40%;
  max-width: 520px;
  min-width: 240px;
}

@media (max-width: 1024px) {
  .task-table__cell--title {
    width: auto;
    max-width: none;
  }
}

.task-table__title-wrapper {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-1, 0.25rem);
  width: 100%;
  min-width: 0;
}

tbody tr {
  cursor: pointer;
  transition: background 120ms ease;
}

tbody tr:hover,
tbody tr:focus-within {
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 82%, transparent);
}

tbody tr.is-recent {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 12%, transparent);
}

.status { color: var(--color-muted, #6b7280); font-weight: 600; }

.chip.small {
  font-size: var(--text-xs, 0.75rem);
  padding: calc(var(--space-1, 0.25rem)) var(--space-2, 0.5rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 85%, transparent);
  border-radius: 999px;
}

.chip.sprint--active {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 18%, transparent);
  color: var(--color-accent, #0ea5e9);
}

.chip.sprint--overdue {
  background: color-mix(in oklab, var(--color-danger, #ef4444) 18%, transparent);
  color: var(--color-danger, #ef4444);
}

.chip.sprint--complete {
  background: color-mix(in oklab, var(--color-success, #16a34a) 18%, transparent);
  color: var(--color-success, #166534);
}

.chip.sprint--pending,
.chip.sprint--unknown {
  background: color-mix(in oklab, var(--color-muted, #6b7280) 18%, transparent);
  color: var(--color-muted, #6b7280);
}

.columns-popover {
  position: absolute;
  margin-top: var(--space-2, 0.5rem);
  padding: var(--space-3, 0.75rem);
  border: 1px solid var(--color-border, var(--border));
  border-radius: var(--radius-lg, 0.75rem);
  background: var(--color-bg, var(--bg));
  box-shadow: var(--shadow-md, 0 4px 16px rgba(15,23,42,0.1));
}

.table-toolbar {
  position: relative;
}

.table-toolbar .controls {
  margin-left: auto;
}

.overdue {
  color: var(--color-danger, #ef4444);
  font-weight: 600;
}

/* Popover placement: under toolbar (top-right) */
.columns-popover {
  top: calc(100% + var(--space-2, 0.5rem));
  right: 0;
  z-index: 10;
}

/* Row actions menu */
.actions-cell {
  position: relative;
  white-space: nowrap;
}

.menu-popover {
  position: absolute;
  top: calc(100% + var(--space-1, 0.25rem));
  right: 0;
  padding: var(--space-2, 0.5rem);
  border: 1px solid var(--color-border, var(--border));
  border-radius: var(--radius-md, 0.375rem);
  background: var(--color-bg, var(--bg));
  box-shadow: var(--shadow-md, 0 4px 16px rgba(15,23,42,0.1));
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  z-index: 10;
}

.bulk-menu-wrapper {
  position: relative;
}

.bulk-actions-menu {
  left: 0;
  right: auto;
  min-width: 220px;
}

.menu-item {
  background: transparent;
  border: none;
  text-align: left;
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border-radius: var(--radius-md, 0.375rem);
  cursor: pointer;
  transition: background 120ms ease;
}

.menu-item:hover {
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 75%, transparent);
}

.menu-item.danger {
  color: var(--color-danger, #ef4444);
}

.menu-separator {
  height: 1px;
  margin: var(--space-2, 0.5rem) 0;
  background: color-mix(in oklab, var(--color-border, #e2e8f0) 80%, transparent);
}

.task-table__title-text {
  display: block;
  width: 100%;
  font-weight: 600;
  color: var(--color-fg, var(--fg));
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}



.session-touch {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, #64748b);
  margin-top: 0;
  max-width: 100%;
}

.session-touch__badge {
  display: inline-flex;
  align-items: center;
  padding: 0 var(--space-1, 0.25rem);
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--color-border, #e2e8f0) 90%, transparent);
  background: color-mix(in oklab, var(--color-surface, #f8fafc) 85%, transparent);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.session-touch__badge.created {
  border-color: color-mix(in oklab, var(--color-success, #16a34a) 40%, transparent);
  background: color-mix(in oklab, var(--color-success, #16a34a) 10%, transparent);
}

.session-touch__badge.updated {
  border-color: color-mix(in oklab, var(--color-accent, #0ea5e9) 40%, transparent);
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 12%, transparent);
}

.session-touch__badge.deleted {
  border-color: color-mix(in oklab, var(--color-danger, #ef4444) 40%, transparent);
  background: color-mix(in oklab, var(--color-danger, #ef4444) 12%, transparent);
}
</style>
