<template>
  <div class="table-wrap" ref="rootRef">
    <div class="table-toolbar row" style="justify-content: space-between; align-items: center; margin-bottom: 8px; position: relative;">
      <div class="row columns-control" style="gap:8px; align-items:center; position: relative;">
        <label class="row" style="gap:6px; align-items:center;">
          <input type="checkbox" :checked="bulk" @change="onToggleBulk($event)" /> Bulk select
        </label>
        <span v-if="bulk" class="muted columns-control__selected">Selected: {{ selected.length }}</span>
        <div v-if="bulk" class="bulk-menu-wrapper">
          <UiButton
            icon-only
            type="button"
            aria-label="Bulk actions"
            title="Bulk actions"
            :disabled="disableBulkActions"
            ref="bulkMenuButton"
            @click.stop="toggleBulkMenu"
          >
            <IconGlyph name="dots-horizontal" />
          </UiButton>
          <div v-if="showBulkMenu" class="menu-popover card bulk-actions-menu" ref="bulkMenuPopover">
            <button class="menu-item" type="button" :disabled="disableBulkActions" @click="handleBulkAction('assign')">
              <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="user-add" /></span>
              <span class="menu-item__label">Assign…</span>
            </button>
            <button class="menu-item" type="button" :disabled="disableBulkActions" @click="handleBulkAction('unassign')">
              <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="user-remove" /></span>
              <span class="menu-item__label">Clear assignee</span>
            </button>
            <button class="menu-item" type="button" :disabled="disableSprintActions" @click="handleBulkAction('sprint-add')">
              <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="flag" /></span>
              <span class="menu-item__label">Add to sprint…</span>
            </button>
            <button class="menu-item" type="button" :disabled="disableSprintActions" @click="handleBulkAction('sprint-remove')">
              <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="flag-remove" /></span>
              <span class="menu-item__label">Remove from sprint…</span>
            </button>
            <button class="menu-item danger" type="button" :disabled="disableBulkActions" @click="handleBulkAction('delete')">
              <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="trash" /></span>
              <span class="menu-item__label">Delete tasks…</span>
            </button>
          </div>
        </div>
      </div>
      <div class="row controls" style="gap:8px; align-items:center; flex-wrap: wrap;">
        <div class="columns-button-wrapper">
          <UiButton class="columns-button" type="button" title="Configure columns" @click="toggleColumnMenu">
            <IconGlyph name="columns" aria-hidden="true" />
            <span>Columns</span>
          </UiButton>
          <div v-if="showColumnMenu" class="columns-popover card" @click.self="showColumnMenu=false">
            <div class="col" style="gap:6px;">
              <label v-for="col in allColumns" :key="col" class="row" style="gap:6px; align-items:center;">
                <input type="checkbox" :checked="columnsSet.has(col)" @change="toggleColumn(col, $event)" />
                <span>{{ headerLabel(col) }}</span>
              </label>
              <div class="row" style="gap:6px; margin-top: 6px;">
                <UiButton type="button" @click="showColumnMenu=false">Close</UiButton>
                <UiButton type="button" @click="resetColumns">Reset</UiButton>
              </div>
            </div>
          </div>
        </div>
        <UiButton class="add-button" type="button" aria-label="Add task" title="Add task" @click="$emit('add')">
          <IconGlyph name="plus" aria-hidden="true" />
          <span>Task</span>
        </UiButton>
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
                <span class="header-button__label">{{ headerLabel(col) }}</span>
                <span class="header-button__sort" aria-hidden="true">
                  <template v-if="sort.key === col">
                    {{ sort.dir === 'asc' ? '▲' : '▼' }}
                  </template>
                  <template v-else>
                    ⇅
                  </template>
                </span>
              </button>
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
                  <span v-for="tag in (t.tags || [])" :key="tag" class="tag">{{ tag }}</span>
                  <input v-if="isEditingTags(t.id)" class="input" v-model="tagsDrafts[t.id]" placeholder="tag1, tag2" style="max-width: 240px;" @click.stop @keyup.enter.prevent="saveTags(t)" />
                </div>
              </template>
              <template v-else-if="col === 'sprints'">
                <div class="row" style="gap:6px; flex-wrap: wrap; align-items:center;">
                  <span
                    v-for="sprintId in (t.sprints || [])"
                    :key="`${t.id}-sprint-${sprintId}`"
                    class="chip small sprint-chip"
                    :class="sprintStateClass(sprintId)"
                    :title="sprintTooltip(sprintId)"
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
              <UiButton icon-only aria-label="Row actions" title="Actions" @click.stop="toggleRowMenu(t.id)">
                <IconGlyph name="dots-horizontal" />
              </UiButton>
              <div v-if="isRowMenuOpen(t.id)" class="menu-popover card">
                <button class="menu-item" v-if="!isEditingTags(t.id)" @click="toggleTagsEdit(t.id); closeRowMenu(t.id)">
                  <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="tag" /></span>
                  <span class="menu-item__label">Edit tags</span>
                </button>
                <button class="menu-item" v-else @click="saveTags(t); closeRowMenu(t.id)">
                  <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="check" /></span>
                  <span class="menu-item__label">Save tags</span>
                </button>
                <button class="menu-item" @click="$emit('assign', t.id); closeRowMenu(t.id)">
                  <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="user-add" /></span>
                  <span class="menu-item__label">Assign…</span>
                </button>
                <button class="menu-item" @click="$emit('unassign', t.id); closeRowMenu(t.id)">
                  <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="user-remove" /></span>
                  <span class="menu-item__label">Clear assignee</span>
                </button>
                <button class="menu-item" @click="$emit('sprint-add', t.id); closeRowMenu(t.id)">
                  <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="flag" /></span>
                  <span class="menu-item__label">Add to sprint…</span>
                </button>
                <button class="menu-item" @click="$emit('sprint-remove', t.id); closeRowMenu(t.id)">
                  <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="flag-remove" /></span>
                  <span class="menu-item__label">Remove from sprint…</span>
                </button>
                <button class="menu-item danger" @click="$emit('delete', t.id); closeRowMenu(t.id)">
                  <span class="menu-item__icon" aria-hidden="true"><IconGlyph name="trash" /></span>
                  <span class="menu-item__label">Delete…</span>
                </button>
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
import { computed, onMounted, onUnmounted, ref, watch, type ComponentPublicInstance } from 'vue';
import { useTaskTableState, type TaskTableEmit, type TaskTableProps } from '../composables/useTaskTableState';
import IconGlyph from './IconGlyph.vue';
import UiButton from './UiButton.vue';
import UiCard from './UiCard.vue';

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
} = useTaskTableState(props, emit)

const sprintLookup = computed(() => props.sprintLookup ?? {})
const hasSprintsLocal = computed(() => props.hasSprints ?? false)
const sprintsLoadingLocal = computed(() => props.sprintsLoading ?? false)

type BulkMenuButtonRef = HTMLElement | (ComponentPublicInstance & { $el: HTMLElement })

const showBulkMenu = ref(false)
const bulkMenuButton = ref<BulkMenuButtonRef | null>(null)
const bulkMenuPopover = ref<HTMLElement | null>(null)
const disableBulkActions = computed(() => !selected.value.length)
const disableSprintActions = computed(() => disableBulkActions.value || sprintsLoadingLocal.value || !hasSprintsLocal.value)

function sprintLabel(id: number) {
  const entry = sprintLookup.value[id]
  const raw = (entry?.label || `#${id}`).trim()
  const firstSpace = raw.indexOf(' ')
  if (firstSpace === -1) return raw
  const head = raw.slice(0, firstSpace)
  const tail = raw.slice(firstSpace + 1).trim()
  if (!tail) return head
  return `${head}\u00a0${tail}`
}

function sprintStateClass(id: number) {
  const state = sprintLookup.value[id]?.state?.toLowerCase()
  if (!state) return 'sprint--unknown'
  return `sprint--${state}`
}

function sprintTooltip(id: number) {
  return sprintLabel(id).replace(/\u00a0/g, ' ')
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

function handleOutsideClick(event: MouseEvent) {
  if (!showBulkMenu.value) return
  const target = event.target as Node | null
  if (!target) return
  const bulkButtonEl = bulkMenuButton.value
    ? bulkMenuButton.value instanceof HTMLElement
      ? bulkMenuButton.value
      : bulkMenuButton.value.$el
    : null
  if (bulkButtonEl && bulkButtonEl.contains(target)) return
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
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2, 0.5rem);
  width: 100%;
  background: transparent;
  border: none;
  padding: 0;
  font: inherit;
  color: inherit;
  cursor: pointer;
  white-space: nowrap;
}

.header-button:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring, 0 0 0 1px rgba(14,165,233,0.35), 0 0 0 4px rgba(14,165,233,0.2));
  border-radius: var(--radius-sm, 0.25rem);
}

.header-button__label {
  overflow: hidden;
  text-overflow: ellipsis;
}

.header-button__sort {
  font-size: 0.8rem;
  line-height: 1;
  min-width: 1.5em;
  text-align: center;
  color: var(--color-muted, #6b7280);
}

th.active .header-button__sort {
  color: var(--color-fg, var(--fg));
}

.task-table__cell {
  position: relative;
}

.task-table__cell--title {
  width: 38%;
  max-width: 480px;
  min-width: 200px;
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

.chip.sprint-chip {
  max-width: 200px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.btn.icon-only {
  width: 36px;
  height: 36px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  font-size: 1.25rem;
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

.columns-button-wrapper {
  position: relative;
}

.columns-button {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  height: 2.25rem;
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

.columns-control__selected {
  font-size: var(--text-sm, 0.875rem);
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
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.menu-item:hover {
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 75%, transparent);
}

.menu-item.danger {
  color: var(--color-danger, #ef4444);
}

.menu-item__icon {
  width: 1.75rem;
  height: 1.75rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--color-muted, #6b7280);
  font-size: 1.15rem;
}

.menu-item.danger .menu-item__icon {
  color: inherit;
}

.menu-item__label {
  flex: 1;
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

.controls .add-button {
  font-weight: 600;
  height: 2.25rem;
  padding: 0 var(--space-4, 1rem);
  gap: var(--space-2, 0.5rem);
}

.controls .add-button .icon-glyph {
  font-size: 1rem;
}

.add-button:hover {
  background: var(--color-accent, #0ea5e9);
  color: var(--color-accent-contrast, #ffffff);
  border-color: transparent;
}

.add-button:hover .icon-glyph {
  color: inherit;
}
</style>
