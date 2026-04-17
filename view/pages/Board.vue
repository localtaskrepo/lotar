<template>
  <section class="col" style="gap:16px;">
    <div class="row" style="justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px;">
      <h1>Boards <span v-if="project" class="muted">— {{ project }}</span></h1>
      <div class="row board-controls">
        <details ref="wipEditorRef" class="wip-editor" @toggle="handleWipToggle">
          <summary class="btn">WIP limits</summary>
          <div class="card col" style="gap:8px; min-width: 260px;">
            <div class="row" v-for="st in columns" :key="st" style="gap:6px; align-items:center;">
              <label style="min-width: 120px;">{{ st }}</label>
              <input class="input" type="number" min="0" :value="wipLimits[st] ?? ''" @input="onWipInput(st, $event)" placeholder="—" style="max-width: 100px;" />
            </div>
            <small class="muted">Leave empty or 0 for no limit. Limits are saved locally per project.</small>
          </div>
        </details>
        <UiButton
          icon-only
          type="button"
          aria-label="Clear filters"
          title="Clear filters"
          :disabled="!hasAnyFilters"
          @click="clearFilters"
        >
          <IconGlyph name="close" />
        </UiButton>
        <ReloadButton
          :disabled="loadingConfig || loadingTasks"
          :loading="loadingConfig || loadingTasks"
          label="Refresh board"
          title="Refresh board"
          @click="refreshAll"
        />
      </div>
    </div>

    <div class="filter-card">
      <div class="row board-chips-row">
        <SmartListChips
          :statuses="statuses"
          :priorities="priorities"
          :value="filter"
          :custom-presets="customFilterPresets"
          @update:value="boardOnChipsUpdate"
          @preset="handleCustomPreset"
        />
        <div class="row board-chips-row__right">
          <select v-model="groupBy" class="input board-chips-row__groupby" aria-label="Group cards by" data-testid="board-groupby">
            <option value="none">No grouping</option>
            <option value="assignee">Group by assignee</option>
            <option value="priority">Group by priority</option>
            <option value="type">Group by type</option>
          </select>
          <details ref="filtersEditorRef" class="done-filter" @toggle="handleFiltersToggle">
            <summary class="btn">Filters</summary>
            <div class="card col done-filter__card">
              <div class="col" style="gap:4px;">
                <span class="muted">Statuses</span>
                <div class="col done-filter__statuses">
                  <label v-for="label in columns" :key="`done-${label}`" class="row" style="gap:6px; align-items:center;">
                    <input type="checkbox" :checked="doneStatusSelected(label)" @change="toggleDoneStatus(label)" />
                    <span>{{ label }}</span>
                  </label>
                  <p v-if="!columns.length" class="muted">No statuses available yet.</p>
                </div>
              </div>
              <label class="col" style="gap:4px;">
                <span class="muted">Hide cards older than (days)</span>
                <input
                  class="input"
                  type="number"
                  min="0"
                  step="1"
                  :value="doneFilters.maxAgeDays ?? ''"
                  placeholder="e.g. 14"
                  @input="onDoneMaxAgeInput"
                />
              </label>
              <label class="col" style="gap:4px;">
                <span class="muted">Limit visible cards</span>
                <input
                  class="input"
                  type="number"
                  min="0"
                  step="1"
                  :value="doneFilters.maxVisible ?? ''"
                  placeholder="e.g. 20"
                  @input="onDoneMaxVisibleInput"
                />
              </label>
              <div class="row" style="justify-content:flex-end; gap:8px;">
                <UiButton variant="ghost" type="button" @click="resetDoneFilters">Reset</UiButton>
              </div>
            </div>
          </details>
        </div>
      </div>
      <div class="row board-filter-row">
        <FilterBar
          ref="filterBarRef"
          class="board-filter-row__bar"
          :statuses="statuses"
          :priorities="priorities"
          :types="types"
          :sprint-options="sprintFilterOptions"
          :value="filterPayload"
          :show-status="false"
          emit-project-key
          storage-key="lotar.boards.filter"
          @update:value="boardOnFilterUpdate"
        />

        <details ref="fieldsEditorRef" class="board-fields" @toggle="handleFieldsToggle">
          <summary class="btn">Fields</summary>
          <div class="card col board-fields__card">
            <div class="col" style="gap:4px;">
              <span class="muted">Card fields</span>
              <div class="col board-fields__items">
                <label v-for="opt in boardFieldOptions" :key="`field-${opt.key}`" class="row" style="gap:6px; align-items:center;">
                  <input type="checkbox" :checked="isBoardFieldVisible(opt.key)" @change="setBoardFieldVisible(opt.key, $event)" />
                  <span>{{ opt.label }}</span>
                </label>
              </div>
            </div>
            <small class="muted">Saved locally per project.</small>
            <div class="row" style="justify-content:flex-end; gap:8px;">
              <UiButton variant="ghost" type="button" @click="resetBoardFields">Reset</UiButton>
              <UiButton type="button" @click="closeBoardFields">Close</UiButton>
            </div>
          </div>
        </details>
      </div>
    </div>

    <div v-if="initialLoading" style="margin: 12px 0;"><UiLoader>Loading board…</UiLoader></div>

    <div v-else-if="!project">
      <UiEmptyState title="Pick a project" description="Boards are per-project. Choose a project to view its board." />
    </div>

    <div v-else class="board grid" :style="gridStyle">
      <template v-if="groupBy !== 'none'">
        <template v-for="group in allGroupLabels" :key="group">
          <!-- Swimlane header spans all columns -->
          <div class="swimlane-row" :style="{ gridColumn: `1 / -1` }">
            <button type="button" class="swimlane-header" :class="{ collapsed: collapsedGroups.has(group) }" @click="toggleGroup(group)">
              <span class="swimlane-chevron" :class="{ open: !collapsedGroups.has(group) }">▶</span>
              <span v-if="groupBy === 'assignee'" class="member-badge" :style="{ background: memberColor(group === '(none)' ? '' : group) }" :title="group">{{ memberInitials(group === '(none)' ? '' : group) }}</span>
              <span class="swimlane-label">{{ group }}</span>
              <span class="muted swimlane-count">{{ groupTotalCount(group) }}</span>
            </button>
          </div>
          <!-- Per-column tasks for this group -->
          <template v-if="!collapsedGroups.has(group)">
            <div v-for="st in columns" :key="`${group}::${st}`" class="col column-group-cell"
                 :data-status="st"
                 :class="{ 'over-limit': overLimit(st) }"
                 @dragover.prevent="onDragOver"
                 @drop.prevent="onDrop(st)"
            >
              <TransitionGroup name="task-list" tag="div" class="col-cards">
                <article v-for="task in groupedColumnTasks(st, group)" :key="task.id"
                         class="card task"
                         :class="[priorityClass(task.priority), { 'task--selected': selectedTaskId === task.id }]"
                         draggable="true"
                         @dragstart="onDragStart(task)"
                         @dblclick="openTask(task.id)"
                         @click.exact="selectTask(task.id)"
                         @keydown.enter.prevent="openTask(task.id)"
                         tabindex="0">
                  <header v-if="hasTaskHeader(task)" class="row task-header">
                    <template v-if="hasTaskIdentity(task)">
                      <div class="row task-header__left">
                        <span v-if="isBoardFieldVisible('id') && (task.id || '').trim()" class="muted id">{{ task.id }}</span>
                        <strong v-if="isBoardFieldVisible('title') && (task.title || '').trim()" class="title">{{ task.title }}</strong>
                      </div>
                      <span v-if="isBoardFieldVisible('priority') && (task.priority || '').trim()" class="priority" :class="priorityClass(task.priority)">{{ task.priority }}</span>
                    </template>
                    <template v-else>
                      <span v-if="isBoardFieldVisible('priority') && (task.priority || '').trim()" class="priority" :class="priorityClass(task.priority)">{{ task.priority }}</span>
                    </template>
                  </header>
                  <footer v-if="hasTaskMeta(task)" class="task-meta" :class="{ 'task-meta--no-header': !hasTaskHeader(task) }">
                    <div v-if="hasPrimaryMeta(task)" class="row task-meta__tags">
                      <span v-if="isBoardFieldVisible('status') && (task.status || '').trim()" class="muted">{{ task.status }}</span>
                      <span v-if="isBoardFieldVisible('task_type') && (task.task_type || '').trim()" class="muted">{{ task.task_type }}</span>
                      <span v-if="isBoardFieldVisible('effort') && (task.effort || '').trim()" class="muted">{{ task.effort }}</span>
                      <span v-if="isBoardFieldVisible('reporter') && (task.reporter || '').trim()" class="muted">by {{ formatMember(task.reporter) }}</span>
                      <span v-if="isBoardFieldVisible('assignee') && (task.assignee || '').trim()" class="member-inline">
                        <span class="member-badge small" :style="{ background: memberColor(task.assignee) }" :title="task.assignee || ''">{{ memberInitials(task.assignee) }}</span>
                        {{ formatMember(task.assignee) }}
                      </span>
                      <span v-if="isBoardFieldVisible('due_date') && taskDueInfo(task).label" class="task-meta__due" :class="{ 'is-overdue': taskDueInfo(task).overdue }">{{ taskDueInfo(task).label }}</span>
                      <span v-if="isBoardFieldVisible('modified') && taskModifiedInfo(task)" class="muted">{{ taskModifiedInfo(task) }}</span>
                      <span v-if="isBoardFieldVisible('tags')" v-for="tag in (task.tags || [])" :key="tag" class="tag">{{ tag }}</span>
                    </div>
                    <div v-if="isBoardFieldVisible('sprints') && task.sprints?.length" class="row task-meta__sprints">
                      <span v-for="sprintId in task.sprints" :key="`${task.id}-sprint-${sprintId}`" class="chip small sprint-chip" :class="sprintStateClass(sprintId)" :title="sprintTooltip(sprintId)">{{ sprintLabel(sprintId) }}</span>
                    </div>
                  </footer>
                </article>
                <div v-if="!groupedColumnTasks(st, group).length" key="__group-empty__" class="muted" style="padding: 4px 0; font-size: var(--text-xs, 0.75rem);">—</div>
              </TransitionGroup>
            </div>
          </template>
        </template>
        <!-- Column headers at the very top (rendered first via CSS order) -->
        <div v-for="st in columns" :key="`hdr-${st}`" class="col-header row board-col-header"
             :data-status="st"
             style="justify-content: space-between; align-items:center; gap:8px;">
          <strong>{{ st }}</strong>
          <span class="muted" :class="{ warn: overLimit(st) }">
            <template v-if="limitOf(st) > 0">{{ countOf(st) }} / {{ limitOf(st) }}</template>
            <template v-else>{{ countOf(st) }}</template>
          </span>
        </div>
      </template>
      <template v-else>
        <div v-for="st in columns" :key="st" class="col column"
             :data-status="st"
             :class="{ 'over-limit': overLimit(st) }"
             tabindex="0"
             @dragover.prevent="onDragOver"
             @drop.prevent="onDrop(st)"
             @keydown.enter.prevent="onDrop(st)"
        >
          <div class="col-header row" style="justify-content: space-between; align-items:center; gap:8px;">
            <strong>{{ st }}</strong>
            <span class="muted" :class="{ warn: overLimit(st) }">
              <template v-if="limitOf(st) > 0">{{ countOf(st) }} / {{ limitOf(st) }}</template>
              <template v-else>{{ countOf(st) }}</template>
            </span>
          </div>
          <TransitionGroup name="task-list" tag="div" class="col-cards">
            <article v-for="task in visibleFlatTasks(st)" :key="task.id"
                     class="card task"
                     :class="[priorityClass(task.priority), { 'task--selected': selectedTaskId === task.id }]"
                     draggable="true"
                     @dragstart="onDragStart(task)"
                     @dblclick="openTask(task.id)"
                     @click.exact="selectTask(task.id)"
                     @keydown.enter.prevent="openTask(task.id)"
                     tabindex="0">
              <header v-if="hasTaskHeader(task)" class="row task-header">
                <template v-if="hasTaskIdentity(task)">
                  <div class="row task-header__left">
                    <span v-if="isBoardFieldVisible('id') && (task.id || '').trim()" class="muted id">{{ task.id }}</span>
                    <strong v-if="isBoardFieldVisible('title') && (task.title || '').trim()" class="title">{{ task.title }}</strong>
                  </div>
                  <span v-if="isBoardFieldVisible('priority') && (task.priority || '').trim()" class="priority" :class="priorityClass(task.priority)">{{ task.priority }}</span>
                </template>
                <template v-else>
                  <span v-if="isBoardFieldVisible('priority') && (task.priority || '').trim()" class="priority" :class="priorityClass(task.priority)">{{ task.priority }}</span>
                </template>
              </header>
              <footer v-if="hasTaskMeta(task)" class="task-meta" :class="{ 'task-meta--no-header': !hasTaskHeader(task) }">
                <div v-if="hasPrimaryMeta(task)" class="row task-meta__tags">
                  <span v-if="isBoardFieldVisible('status') && (task.status || '').trim()" class="muted">{{ task.status }}</span>
                  <span v-if="isBoardFieldVisible('task_type') && (task.task_type || '').trim()" class="muted">{{ task.task_type }}</span>
                  <span v-if="isBoardFieldVisible('effort') && (task.effort || '').trim()" class="muted">{{ task.effort }}</span>
                  <span v-if="isBoardFieldVisible('reporter') && (task.reporter || '').trim()" class="muted">by {{ formatMember(task.reporter) }}</span>
                  <span v-if="isBoardFieldVisible('assignee') && (task.assignee || '').trim()" class="member-inline">
                    <span class="member-badge small" :style="{ background: memberColor(task.assignee) }" :title="task.assignee || ''">{{ memberInitials(task.assignee) }}</span>
                    {{ formatMember(task.assignee) }}
                  </span>
                  <span v-if="isBoardFieldVisible('due_date') && taskDueInfo(task).label" class="task-meta__due" :class="{ 'is-overdue': taskDueInfo(task).overdue }">{{ taskDueInfo(task).label }}</span>
                  <span v-if="isBoardFieldVisible('modified') && taskModifiedInfo(task)" class="muted">{{ taskModifiedInfo(task) }}</span>
                  <span v-if="isBoardFieldVisible('tags')" v-for="tag in (task.tags || [])" :key="tag" class="tag">{{ tag }}</span>
                </div>
                <div v-if="isBoardFieldVisible('sprints') && task.sprints?.length" class="row task-meta__sprints">
                  <span
                    v-for="sprintId in task.sprints"
                    :key="`${task.id}-sprint-${sprintId}`"
                    class="chip small sprint-chip"
                    :class="sprintStateClass(sprintId)"
                    :title="sprintTooltip(sprintId)"
                  >
                    {{ sprintLabel(sprintId) }}
                  </span>
                </div>
              </footer>
            </article>
            <div v-if="!grouped[st]?.length" key="__empty__" class="muted" style="padding: 8px;">No tasks</div>
          </TransitionGroup>
          <button v-if="hiddenFlatCount(st) > 0" type="button" class="show-more-btn" @click="showMore(st)">
            Show {{ hiddenFlatCount(st) }} more…
          </button>
        </div>
        <div v-if="other.length" class="col column" data-status="__other__">
          <div class="col-header row" style="justify-content: space-between; align-items:center; gap:8px;">
            <strong>Other</strong>
            <span class="muted">{{ other.length }}</span>
          </div>
          <TransitionGroup name="task-list" tag="div" class="col-cards">
            <article v-for="task in other" :key="task.id"
                     class="card task"
                     :class="[priorityClass(task.priority), { 'task--selected': selectedTaskId === task.id }]"
                     draggable="true"
                     @dragstart="onDragStart(task)"
                     @dblclick="openTask(task.id)"
                     @click.exact="selectTask(task.id)"
                     tabindex="0">
              <header v-if="hasTaskHeader(task)" class="row task-header">
                <template v-if="hasTaskIdentity(task)">
                  <div class="row task-header__left">
                    <span v-if="isBoardFieldVisible('id') && (task.id || '').trim()" class="muted id">{{ task.id }}</span>
                    <strong v-if="isBoardFieldVisible('title') && (task.title || '').trim()" class="title">{{ task.title }}</strong>
                  </div>
                  <span v-if="isBoardFieldVisible('priority') && (task.priority || '').trim()" class="priority" :class="priorityClass(task.priority)">{{ task.priority }}</span>
                </template>
                <template v-else>
                  <span v-if="isBoardFieldVisible('priority') && (task.priority || '').trim()" class="priority" :class="priorityClass(task.priority)">{{ task.priority }}</span>
                </template>
              </header>
              <footer v-if="hasTaskMeta(task)" class="task-meta" :class="{ 'task-meta--no-header': !hasTaskHeader(task) }">
                <div v-if="hasPrimaryMeta(task)" class="row task-meta__tags">
                  <span v-if="isBoardFieldVisible('status') && (task.status || '').trim()" class="muted">{{ task.status }}</span>
                  <span v-if="isBoardFieldVisible('task_type') && (task.task_type || '').trim()" class="muted">{{ task.task_type }}</span>
                  <span v-if="isBoardFieldVisible('effort') && (task.effort || '').trim()" class="muted">{{ task.effort }}</span>
                  <span v-if="isBoardFieldVisible('reporter') && (task.reporter || '').trim()" class="muted">by {{ formatMember(task.reporter) }}</span>
                  <span v-if="isBoardFieldVisible('assignee') && (task.assignee || '').trim()" class="member-inline">
                    <span class="member-badge small" :style="{ background: memberColor(task.assignee) }" :title="task.assignee || ''">{{ memberInitials(task.assignee) }}</span>
                    {{ formatMember(task.assignee) }}
                  </span>
                  <span v-if="isBoardFieldVisible('due_date') && taskDueInfo(task).label" class="task-meta__due" :class="{ 'is-overdue': taskDueInfo(task).overdue }">{{ taskDueInfo(task).label }}</span>
                  <span v-if="isBoardFieldVisible('modified') && taskModifiedInfo(task)" class="muted">{{ taskModifiedInfo(task) }}</span>
                  <span v-if="isBoardFieldVisible('tags')" v-for="tag in (task.tags || [])" :key="tag" class="tag">{{ tag }}</span>
                </div>
                <div v-if="isBoardFieldVisible('sprints') && task.sprints?.length" class="row task-meta__sprints">
                  <span
                    v-for="sprintId in task.sprints"
                    :key="`${task.id}-other-sprint-${sprintId}`"
                    class="chip small sprint-chip"
                    :class="sprintStateClass(sprintId)"
                    :title="sprintTooltip(sprintId)"
                  >
                    {{ sprintLabel(sprintId) }}
                  </span>
                </div>
              </footer>
            </article>
          </TransitionGroup>
        </div>
      </template>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '../api/client'
import type { TaskDTO } from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import IconGlyph from '../components/IconGlyph.vue'
import ReloadButton from '../components/ReloadButton.vue'
import SmartListChips from '../components/SmartListChips.vue'
import { showToast } from '../components/toast'
import UiButton from '../components/UiButton.vue'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiLoader from '../components/UiLoader.vue'
import { useConfig } from '../composables/useConfig'
import { useFieldVisibility } from '../composables/useFieldVisibility'
import { applySmartFilters, buildServerFilter, useCustomFilterPresets, useProjectFilterSync } from '../composables/useFilterBuilder'
import { useProjects } from '../composables/useProjects'
import { useSprintFormatting } from '../composables/useSprintFormatting'
import { useSprints } from '../composables/useSprints'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { useTaskStore } from '../composables/useTaskStore'
import { parseTaskDate, startOfLocalDay } from '../utils/date'
import { formatMember, memberColor, memberInitials } from '../utils/member'
import { findLastStatusChangeAt } from '../utils/taskHistory'

const router = useRouter()
const route = useRoute()
const { projects, refresh: refreshProjects } = useProjects()
const { statuses, priorities, types, customFields: availableCustomFields, refresh: refreshConfig, loading: loadingConfig } = useConfig()
const { sprints, refresh: refreshSprints } = useSprints()
const store = useTaskStore()
const loadingTasks = computed(() => store.status.value === 'loading')
const items = computed(() => store.items.value)
const { openTaskPanel } = useTaskPanelController()

const project = ref<string>(route.query.project ? String(route.query.project) : '')
const draggingId = ref<string>('')
const filter = ref<Record<string, string>>({})
const filterBarRef = ref<{ appendCustomFilter: (expr: string) => void; clear?: () => void } | null>(null)
const wipEditorRef = ref<HTMLDetailsElement | null>(null)
const filtersEditorRef = ref<HTMLDetailsElement | null>(null)
const fieldsEditorRef = ref<HTMLDetailsElement | null>(null)
const filterPayload = computed(() => ({
  ...filter.value,
  project: project.value || '',
}))
const { hasFilters, sanitizeFilterInput, onFilterUpdate, onChipsUpdate, clearFilters: clearFiltersAction } = useProjectFilterSync(project, filter)
const customFilterPresets = useCustomFilterPresets(availableCustomFields)

const { sprintLookup, sprintLabel, sprintStateClass, sprintTooltip } = useSprintFormatting(sprints)

const sprintFilterOptions = computed(() =>
  (sprints.value || []).map((s) => ({ id: s.id, label: s.display_name || `Sprint ${s.id}` })),
)

// -- Swimlane group-by (persisted per project) ----------------------------
type GroupByMode = 'none' | 'assignee' | 'priority' | 'type'
function groupByKey() { return project.value ? `lotar.boardGroupBy::${project.value}` : 'lotar.boardGroupBy' }
function loadGroupBy(): GroupByMode {
  try {
    const v = localStorage.getItem(groupByKey())
    if (v === 'assignee' || v === 'priority' || v === 'type') return v
  } catch {}
  return 'none'
}
const groupBy = ref<GroupByMode>(loadGroupBy())
function saveGroupBy() { try { localStorage.setItem(groupByKey(), groupBy.value) } catch {} }

// -- Initial loading (only shows spinner before first data arrives) --------
const hasEverLoaded = ref(false)
const initialLoading = computed(() => (loadingConfig.value || loadingTasks.value) && !hasEverLoaded.value)

// -- Ticket highlight on single click -------------------------------------
const selectedTaskId = ref('')
function selectTask(id: string) { selectedTaskId.value = selectedTaskId.value === id ? '' : id }

// -- Collapsible groups ---------------------------------------------------
const collapsedGroups = ref<Set<string>>(new Set())
function toggleGroup(label: string) {
  const next = new Set(collapsedGroups.value)
  if (next.has(label)) next.delete(label); else next.add(label)
  collapsedGroups.value = next
}

// -- Progressive disclosure (virtual scrolling) ---------------------------
const COLUMN_PAGE_SIZE = 30
const columnExpansion = ref<Record<string, number>>({})
function visibleLimit(st: string): number { return columnExpansion.value[st] || COLUMN_PAGE_SIZE }
function showMore(st: string) { columnExpansion.value = { ...columnExpansion.value, [st]: visibleLimit(st) + COLUMN_PAGE_SIZE } }
function resetExpansion() { columnExpansion.value = {} }

const MS_PER_DAY = 24 * 60 * 60 * 1000

function startOfDay(date: Date) {
  return startOfLocalDay(date)
}

function parseDateLike(value?: string | null) {
  return parseTaskDate(value)
}

function handleWipToggle() {
  if (wipEditorRef.value?.open) {
    if (filtersEditorRef.value?.open) {
      filtersEditorRef.value.open = false
    }
    if (fieldsEditorRef.value?.open) {
      fieldsEditorRef.value.open = false
    }
  }
}

function handleFiltersToggle() {
  if (filtersEditorRef.value?.open) {
    if (wipEditorRef.value?.open) {
      wipEditorRef.value.open = false
    }
    if (fieldsEditorRef.value?.open) {
      fieldsEditorRef.value.open = false
    }
  }
}

function handleFieldsToggle() {
  if (fieldsEditorRef.value?.open) {
    if (wipEditorRef.value?.open) {
      wipEditorRef.value.open = false
    }
    if (filtersEditorRef.value?.open) {
      filtersEditorRef.value.open = false
    }
  }
}

function handleBoardPopoverClick(event: MouseEvent) {
  const target = event.target as Node | null
  if (!target) return

  if (wipEditorRef.value?.open && !wipEditorRef.value.contains(target)) {
    wipEditorRef.value.open = false
  }
  if (filtersEditorRef.value?.open && !filtersEditorRef.value.contains(target)) {
    filtersEditorRef.value.open = false
  }
  if (fieldsEditorRef.value?.open && !fieldsEditorRef.value.contains(target)) {
    fieldsEditorRef.value.open = false
  }
}

function priorityClass(value: string | undefined): string {
  const v = (value || '').trim().toLowerCase()
  if (v === 'critical') return 'priority--critical'
  if (v === 'high') return 'priority--high'
  if (v === 'low') return 'priority--low'
  return ''
}

const doneStatus = computed(() => {
  const s = statuses.value
  return s.length ? normalizeStatusKey(s[s.length - 1]) : ''
})

function taskDueInfo(task: TaskDTO): { label: string; overdue: boolean } {
  const raw = (task.due_date || '').trim()
  if (!raw) return { label: '', overdue: false }
  const parsed = parseDateLike(raw)
  if (!parsed) return { label: raw, overdue: false }

  const today = startOfDay(new Date())
  const due = startOfDay(parsed)
  const diffDays = Math.round((due.getTime() - today.getTime()) / MS_PER_DAY)
  const sameYear = parsed.getFullYear() === today.getFullYear()
  const dateLabel = parsed.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' })
  if (diffDays < 0) {
    // Tasks in the final (done) status are not overdue
    const isDone = doneStatus.value && normalizeStatusKey(task.status) === doneStatus.value
    return { label: isDone ? `Due ${dateLabel}` : `Overdue ${dateLabel}`, overdue: !isDone }
  }
  return { label: `Due ${dateLabel}`, overdue: false }
}

function taskModifiedInfo(task: TaskDTO): string {
  const raw = (task.modified || '').trim()
  if (!raw) return ''
  let parsed: Date | null = null
  try {
    const d = new Date(raw)
    parsed = Number.isFinite(d.getTime()) ? d : null
  } catch {
    parsed = null
  }
  if (!parsed) return `Updated ${raw}`
  const now = new Date()
  const sameYear = parsed.getFullYear() === now.getFullYear()
  const dateLabel = parsed.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' })
  return `Updated ${dateLabel}`
}

function hasPrimaryMeta(task: TaskDTO): boolean {
  return Boolean(
    (isBoardFieldVisible('status') && (task.status || '').trim())
    || (isBoardFieldVisible('task_type') && (task.task_type || '').trim())
    || (isBoardFieldVisible('effort') && (task.effort || '').trim())
    || (isBoardFieldVisible('reporter') && (task.reporter || '').trim())
    || (isBoardFieldVisible('assignee') && (task.assignee || '').trim())
    || (isBoardFieldVisible('due_date') && taskDueInfo(task).label)
    || (isBoardFieldVisible('modified') && (task.modified || '').trim())
    || (isBoardFieldVisible('tags') && (task.tags || []).length)
  )
}

function hasTaskMeta(task: TaskDTO): boolean {
  return Boolean(hasPrimaryMeta(task) || (isBoardFieldVisible('sprints') && (task.sprints || []).length))
}

function hasTaskHeader(task: TaskDTO): boolean {
  return Boolean(
    (isBoardFieldVisible('id') && (task.id || '').trim())
    || (isBoardFieldVisible('title') && (task.title || '').trim())
    || (isBoardFieldVisible('priority') && (task.priority || '').trim())
  )
}

function hasTaskIdentity(task: TaskDTO): boolean {
  return Boolean(
    (isBoardFieldVisible('id') && (task.id || '').trim())
    || (isBoardFieldVisible('title') && (task.title || '').trim())
  )
}

function syncProjectRoute(nextProject: string) {
  const desired = nextProject || ''
  const current = typeof route.query.project === 'string' ? route.query.project : ''
  if (current === desired) return
  router.push({ path: '/boards', query: desired ? { project: desired } : {} })
}

// Extend the generic filter sync to also sync the route
function boardOnFilterUpdate(v: Record<string, string>) {
  const hasProjectKey = v && Object.prototype.hasOwnProperty.call(v, 'project')
  if (hasProjectKey) syncProjectRoute((v.project || '').trim())
  onFilterUpdate(v)
}

function boardOnChipsUpdate(v: Record<string, string>) {
  const hasProjectKey = v && Object.prototype.hasOwnProperty.call(v, 'project')
  if (hasProjectKey) syncProjectRoute((v.project || '').trim())
  onChipsUpdate(v)
}

function handleCustomPreset(expression: string) {
  filterBarRef.value?.appendCustomFilter(expression)
}

const hasDoneFilters = computed(() => {
  const d = doneFilters.value
  return d.statuses.length > 0 || (typeof d.maxAgeDays === 'number' && d.maxAgeDays > 0) || (typeof d.maxVisible === 'number' && d.maxVisible > 0)
})
const hasAnyFilters = computed(() => hasFilters.value || groupBy.value !== 'none' || hasDoneFilters.value)

function clearFilters() {
  clearFiltersAction(filterBarRef)
  groupBy.value = 'none'
  saveGroupBy()
  collapsedGroups.value = new Set()
  resetDoneFilters()
}

async function refreshBoardTasks(snapshot?: Record<string, string>) {
  if (!project.value) {
    return
  }
  const raw = snapshot ?? filter.value
  const { serverFilter, normalized } = buildServerFilter(raw, project.value)
  try {
    await store.hydrateAll(serverFilter, { clear: true })
    hasEverLoaded.value = true
  } catch (err: any) {
    showToast(err?.message || 'Failed to load board tasks')
  }
}

function normalizeStatusKey(value: string | null | undefined) {
  return typeof value === 'string'
    ? value.trim().toLowerCase().replace(/[\s_-]+/g, '')
    : ''
}

const statusSource = computed(() => {
  if (statuses.value && statuses.value.length) {
    return [...statuses.value]
  }
  const derived = new Set<string>()
  for (const task of items.value || []) {
    if (task.status) {
      const label = String(task.status).trim()
      if (label) derived.add(label)
    }
  }
  return Array.from(derived)
})

const columnsData = computed(() => {
  const source = statusSource.value as Array<string | null | undefined>
  const seen = new Set<string>()
  const ordered: Array<{ label: string; norm: string }> = []
  for (const raw of source) {
    const label = String(raw ?? '').trim()
    if (!label) continue
    const norm = normalizeStatusKey(label)
    if (!norm || seen.has(norm)) continue
    seen.add(norm)
    ordered.push({ label, norm })
  }
  return ordered
})

const columns = computed(() => columnsData.value.map((item) => item.label))

const columnLookup = computed(() => {
  const map = new Map<string, string>()
  columnsData.value.forEach(({ label, norm }) => {
    map.set(norm, label)
  })
  return map
})

const rawGrouped = computed<Record<string, TaskDTO[]>>(() => {
  const g: Record<string, TaskDTO[]> = {}
  const lookup = columnLookup.value
  const activeProject = project.value
  for (const { label } of columnsData.value) g[label] = []
  // Apply smart filters (client-side) before grouping
  const { normalized } = buildServerFilter(filter.value, activeProject)
  const filtered = applySmartFilters(items.value || [], normalized)
  const dir = normalized.order === 'asc' ? 'asc' : 'desc'
  filtered.sort((a, b) => (dir === 'desc' ? b.modified.localeCompare(a.modified) : a.modified.localeCompare(b.modified)))
  for (const t of filtered) {
    if (!activeProject || !t.id.startsWith(`${activeProject}-`)) continue
    const key = lookup.get(normalizeStatusKey(t.status))
    if (key) {
      g[key].push(t)
    }
  }
  return g
})

const grouped = computed<Record<string, TaskDTO[]>>(() => applyDoneFilters(rawGrouped.value))

function applyDoneFilters(groups: Record<string, TaskDTO[]>) {
  const targetStatuses = doneFilters.value.statuses.filter((label) => label && groups[label])
  const maxAgeDays = typeof doneFilters.value.maxAgeDays === 'number' && doneFilters.value.maxAgeDays > 0
    ? doneFilters.value.maxAgeDays
    : null
  const maxVisible = typeof doneFilters.value.maxVisible === 'number' && doneFilters.value.maxVisible > 0
    ? Math.floor(doneFilters.value.maxVisible)
    : null
  if (!targetStatuses.length || (!maxAgeDays && !maxVisible)) return groups
  const statusSet = new Set(targetStatuses)
  const ageMs = maxAgeDays ? maxAgeDays * MS_PER_DAY : null
  const now = Date.now()
  const result: Record<string, TaskDTO[]> = {}
  Object.entries(groups).forEach(([label, tasks]) => {
    if (!statusSet.has(label)) {
      result[label] = tasks
      return
    }
    let filtered = Array.isArray(tasks) ? [...tasks] : []
    if (ageMs) {
      filtered = filtered.filter((task) => {
        const ts = findLastStatusChangeAt(task, label)
        if (ts === null) return true
        return now - ts <= ageMs
      })
    }
    if (maxVisible) {
      filtered.sort((a, b) => b.modified.localeCompare(a.modified))
      filtered = filtered.slice(0, maxVisible)
    }
    result[label] = filtered
  })
  return result
}

const other = computed(() => {
  const lookup = columnLookup.value
  const activeProject = project.value
  return (items.value || []).filter((t) => {
    if (!activeProject || !t.id.startsWith(`${activeProject}-`)) return false
    return !lookup.get(normalizeStatusKey(t.status))
  })
})

// -- Aligned swimlane computeds -------------------------------------------
function groupKeyFor(task: TaskDTO): string {
  if (groupBy.value === 'assignee') return (task.assignee || '').trim() || '(none)'
  if (groupBy.value === 'priority') return (task.priority || '').trim() || '(none)'
  if (groupBy.value === 'type') return (task.task_type || '').trim() || '(none)'
  return ''
}

const allGroupLabels = computed<string[]>(() => {
  if (groupBy.value === 'none') return []
  const labels = new Set<string>()
  for (const tasks of Object.values(grouped.value)) {
    for (const t of tasks) labels.add(groupKeyFor(t))
  }
  for (const t of other.value) labels.add(groupKeyFor(t))
  const arr = Array.from(labels).sort((a, b) => {
    if (a === '(none)') return 1
    if (b === '(none)') return -1
    return a.localeCompare(b)
  })
  return arr
})

function groupedColumnTasks(st: string, group: string): TaskDTO[] {
  return (grouped.value[st] || []).filter(t => groupKeyFor(t) === group)
}

function groupTotalCount(group: string): number {
  let count = 0
  for (const tasks of Object.values(grouped.value)) {
    count += tasks.filter(t => groupKeyFor(t) === group).length
  }
  return count
}

// -- Flat (ungrouped) column helpers --------------------------------------
function visibleFlatTasks(st: string): TaskDTO[] {
  const all = grouped.value[st] || []
  const limit = visibleLimit(st)
  return all.length <= limit ? all : all.slice(0, limit)
}

function hiddenFlatCount(st: string): number {
  const all = grouped.value[st] || []
  const limit = visibleLimit(st)
  return all.length <= limit ? 0 : all.length - limit
}

const gridStyle = computed(() => ({
  display: 'grid',
  gridTemplateColumns: `repeat(${columns.value.length + (other.value.length ? 1 : 0)}, minmax(260px, 1fr))`,
  gap: '12px',
}))

// --- WIP limits (local-only per project) ---
const wipLimits = ref<Record<string, number>>({})
function wipKey(){ return project.value ? `lotar.wip::${project.value}` : 'lotar.wip' }
function loadWip(){
  try {
    const raw = localStorage.getItem(wipKey())
    const obj = raw ? JSON.parse(raw) : {}
    wipLimits.value = (obj && typeof obj === 'object') ? obj : {}
  } catch { wipLimits.value = {} }
}
function saveWip(){ try { localStorage.setItem(wipKey(), JSON.stringify(wipLimits.value || {})) } catch {} }
function limitOf(st: string): number { const v = (wipLimits.value || {})[st]; return (typeof v === 'number' && v > 0) ? v : 0 }
function countOf(st: string): number { return (grouped.value[st]?.length || 0) }
function overLimit(st: string): boolean { const lim = limitOf(st); return lim > 0 && countOf(st) > lim }
function onWipInput(st: string, ev: Event){
  const val = parseInt((ev.target as HTMLInputElement).value, 10)
  if (!isFinite(val) || val <= 0) { delete (wipLimits.value as any)[st] } else { (wipLimits.value as any)[st] = val }
  saveWip()
}

type DoneFilterSettings = {
  statuses: string[]
  maxAgeDays: number | null
  maxVisible: number | null
}

const doneFilters = ref<DoneFilterSettings>({ statuses: [], maxAgeDays: null, maxVisible: null })

function doneFilterKey(){ return project.value ? `lotar.doneFilters::${project.value}` : 'lotar.doneFilters' }

function loadDoneFilters(){
  try {
    const raw = localStorage.getItem(doneFilterKey())
    if (!raw) {
      doneFilters.value = { statuses: [], maxAgeDays: null, maxVisible: null }
      return
    }
    const parsed = JSON.parse(raw)
    const statuses = Array.isArray(parsed?.statuses) ? parsed.statuses.filter((label: unknown) => typeof label === 'string') : []
    const age = Number(parsed?.maxAgeDays)
    const limit = Number(parsed?.maxVisible)
    doneFilters.value = {
      statuses,
      maxAgeDays: Number.isFinite(age) && age > 0 ? age : null,
      maxVisible: Number.isFinite(limit) && limit > 0 ? Math.floor(limit) : null,
    }
  } catch {
    doneFilters.value = { statuses: [], maxAgeDays: null, maxVisible: null }
  }
}

function saveDoneFilters(){
  try {
    localStorage.setItem(doneFilterKey(), JSON.stringify(doneFilters.value))
  } catch {}
}

function doneStatusSelected(label: string){
  return doneFilters.value.statuses.includes(label)
}

function toggleDoneStatus(label: string){
  if (!label) return
  const set = new Set(doneFilters.value.statuses)
  if (set.has(label)) set.delete(label); else set.add(label)
  doneFilters.value = { ...doneFilters.value, statuses: Array.from(set) }
}

function onDoneMaxAgeInput(ev: Event){
  const value = Number((ev.target as HTMLInputElement).value)
  doneFilters.value = {
    ...doneFilters.value,
    maxAgeDays: Number.isFinite(value) && value > 0 ? value : null,
  }
}

function onDoneMaxVisibleInput(ev: Event){
  const value = Number((ev.target as HTMLInputElement).value)
  doneFilters.value = {
    ...doneFilters.value,
    maxVisible: Number.isFinite(value) && value > 0 ? Math.floor(value) : null,
  }
}

function resetDoneFilters(){
  doneFilters.value = { statuses: [], maxAgeDays: null, maxVisible: null }
}

watch(doneFilters, () => {
  saveDoneFilters()
}, { deep: true })

watch(groupBy, () => { saveGroupBy() })

const DEFAULT_BOARD_FIELDS: Record<string, boolean> = {
  id: true,
  title: true,
  status: false,
  priority: true,
  task_type: false,
  reporter: false,
  assignee: true,
  effort: false,
  tags: true,
  sprints: true,
  due_date: true,
  modified: false,
}

const { fields: boardFields, fieldOptions: boardFieldOptions, load: loadBoardFields, reset: resetBoardFields, isVisible: isBoardFieldVisible, setVisible: setBoardFieldVisible } = useFieldVisibility(
  'lotar.boardFields',
  project,
  DEFAULT_BOARD_FIELDS,
  availableCustomFields,
)

function closeBoardFields() {
  if (fieldsEditorRef.value) {
    fieldsEditorRef.value.open = false
  }
}

function onDragStart(t: any) {
  draggingId.value = t.id
}
function onDragOver(ev: DragEvent) { ev.preventDefault() }
async function onDrop(targetStatus: string) {
  const id = draggingId.value
  if (!id || !targetStatus || targetStatus === '__other__') return
  draggingId.value = ''
  try {
    // Optimistic move via store
    const existing = store.items.value.find(t => t.id === id)
    if (existing) {
      store.upsert({ ...existing, status: targetStatus })
    }
    await api.setStatus(id, targetStatus)
    showToast(`Moved ${id} → ${targetStatus}`)
    await refreshBoardTasks()
  } catch (e: any) {
    showToast(e.message || 'Failed to move task')
    // revert by refetching
    await refreshBoardTasks()
  }
}

function openTask(id: string) {
  openTaskPanel({ taskId: id })
}

async function refreshAll() {
  await refreshProjects()
  await refreshConfig(project.value)
  await refreshSprints(true)
  await refreshBoardTasks()
}

let filterDebounce: ReturnType<typeof setTimeout> | null = null

watch(filter, (value) => {
  if (filterDebounce) clearTimeout(filterDebounce)
  resetExpansion()
  const snapshot = { ...value }
  filterDebounce = setTimeout(() => {
    refreshBoardTasks(snapshot).catch((err) => {
      console.warn('Failed to refresh board after filter change', err)
    })
  }, 150)
}, { deep: true })

onMounted(async () => {
  if (typeof window !== 'undefined') {
    window.addEventListener('click', handleBoardPopoverClick)
  }
  await refreshProjects()
  if (!project.value) {
    project.value = projects.value[0]?.prefix || ''
  }
  await refreshSprints(true)
  await refreshConfig(project.value)
  await refreshBoardTasks()
  loadWip()
  loadDoneFilters()
  loadBoardFields()
})

watch(() => route.query, async (q) => {
  project.value = (q as any).project ? String((q as any).project) : ''
  await refreshConfig(project.value)
  await refreshBoardTasks()
  loadWip()
  loadDoneFilters()
  loadBoardFields()
  groupBy.value = loadGroupBy()
  collapsedGroups.value = new Set()
  resetExpansion()
})

onUnmounted(() => {
  if (filterDebounce) {
    clearTimeout(filterDebounce)
    filterDebounce = null
  }
  if (typeof window !== 'undefined') {
    window.removeEventListener('click', handleBoardPopoverClick)
  }
})
</script>

<style scoped>
.board { align-items: flex-start; }
.column { border: 1px solid var(--border); border-radius: var(--radius-base); background: var(--bg); min-height: 200px; display: flex; flex-direction: column; }
.column.over-limit { border-color: color-mix(in oklab, var(--color-danger) 40%, var(--border)); }
.col-header { position: sticky; top: 0; background: var(--bg); padding: 8px; border-bottom: 1px solid var(--border); border-top-left-radius: var(--radius-base); border-top-right-radius: var(--radius-base); z-index: var(--z-sticky); }
.col-header .warn { color: var(--color-danger-strong); font-weight: 600; }
.col-cards { padding: 8px; display: flex; flex-direction: column; gap: 8px; position: relative; }
.task { padding: 8px; border: 1px solid var(--border); border-radius: var(--radius-base); cursor: grab; user-select: none; }
.task:active { cursor: grabbing; }
.task .id {
  font-family: var(--font-mono);
  margin-right: 6px;
  line-height: var(--line-tight);
  white-space: nowrap;
  overflow-wrap: normal;
  word-break: keep-all;
}
.task .title {
  display: block;
  min-width: 0;
  white-space: normal;
  overflow-wrap: anywhere;
  word-break: break-word;
  line-height: 1.35;
}
.priority { font-size: 12px; color: var(--muted); flex: 0 0 auto; white-space: nowrap; }
.priority.priority--critical { color: var(--danger, #d93025); font-weight: 600; }
.priority.priority--high { color: var(--warning, #e8710a); font-weight: 600; }
.priority.priority--low { opacity: 0.6; }
.task.priority--critical { border-left: 3px solid var(--danger, #d93025); }
.task.priority--high { border-left: 3px solid var(--warning, #e8710a); }
.column:focus { outline: 2px solid color-mix(in oklab, var(--fg) 30%, transparent); outline-offset: 2px; }
.board-controls {
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}
.board-controls :is(.ui-select, .btn) {
  min-height: 2.25rem;
  height: 2.25rem;
  padding-top: 0;
  padding-bottom: 0;
}
.board-controls .btn.icon-only {
  width: 2.25rem;
  height: 2.25rem;
}
.wip-editor {
  position: relative;
}
.wip-editor > summary {
  list-style: none;
  cursor: pointer;
}
.wip-editor > summary::-webkit-details-marker {
  display: none;
}
.wip-editor > .card {
  display: none;
}
.wip-editor[open] > .card {
  display: flex;
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: var(--z-popover);
  box-shadow: var(--shadow-popover);
}

.done-filter {
  position: relative;
}
.done-filter > summary {
  list-style: none;
  cursor: pointer;
}
.done-filter > summary::-webkit-details-marker {
  display: none;
}
.done-filter__card {
  gap: 8px;
  min-width: 280px;
  display: none;
}
.done-filter[open] > .done-filter__card {
  display: flex;
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: var(--z-popover);
  box-shadow: var(--shadow-popover);
}
.done-filter__statuses {
  gap: 4px;
  max-height: 220px;
  overflow-y: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: 6px;
}

.board-fields {
  position: relative;
}

.board-fields > summary {
  list-style: none;
  cursor: pointer;
}

.board-fields > summary::-webkit-details-marker {
  display: none;
}

.board-fields__card {
  gap: 8px;
  min-width: 240px;
  display: none;
}

.board-fields[open] > .board-fields__card {
  display: flex;
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: var(--z-popover);
  box-shadow: var(--shadow-popover);
}

.board-fields__items {
  gap: 4px;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: 6px;
}

.filter-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.board-chips-row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.board-chips-row__right {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-left: auto;
}

.board-chips-row__groupby {
  font-size: var(--text-sm, 0.875rem);
  padding: calc(var(--space-2, 0.5rem) - 2px) var(--space-4, 1rem);
  height: auto;
  min-height: 0;
  line-height: var(--line-tight, 1.25);
}

.board-filter-row {
  flex-wrap: wrap;
  gap: 8px;
  align-items: flex-start;
}

.board-filter-row__bar {
  flex: 1 1 auto;
}

.board-filter-row > .board-fields {
  margin-left: auto;
}

.task-header {
  justify-content: space-between;
  gap: 6px;
  align-items: flex-start;
}

.task-header__left {
  gap: 6px;
  align-items: baseline;
  flex: 1 1 auto;
  min-width: 0;
}

.task-header__left .title {
  flex: 1 1 auto;
  min-width: 0;
}

.task-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 6px;
}

.task-meta.task-meta--no-header {
  margin-top: 0;
}

.task-meta__tags,
.task-meta__sprints {
  gap: 6px;
  flex-wrap: wrap;
  align-items: center;
}

.task-meta__due {
  font-size: 12px;
  color: var(--muted);
}

.task-meta__due.is-overdue {
  color: var(--color-danger-strong);
  font-weight: 600;
}

.chip.sprint-chip {
  font-size: var(--text-xs, 0.75rem);
  padding: calc(var(--space-1, 0.25rem)) var(--space-2, 0.5rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 85%, transparent);
  border-radius: var(--radius-pill);
}

.chip.sprint--active {
  background: color-mix(in oklab, var(--color-accent) 18%, transparent);
  color: var(--color-accent);
}

.chip.sprint--overdue {
  background: color-mix(in oklab, var(--color-danger) 18%, transparent);
  color: var(--color-danger);
}

.chip.sprint--complete {
  background: color-mix(in oklab, var(--color-success) 18%, transparent);
  color: var(--color-success-strong);
}

.chip.sprint--pending,
.chip.sprint--unknown {
  background: color-mix(in oklab, var(--color-muted) 18%, transparent);
  color: var(--color-muted);
}

/* -- Swimlane headers --------------------------------------------------- */
.swimlane-row {
  display: flex;
  align-items: center;
}

.swimlane-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 4px;
  border: none;
  border-bottom: 1px solid var(--border);
  background: none;
  font-size: var(--text-sm, 0.875rem);
  font-weight: 600;
  user-select: none;
  cursor: pointer;
  width: 100%;
  text-align: left;
  color: inherit;
}

.swimlane-header:hover {
  background: color-mix(in oklab, var(--fg) 4%, transparent);
}

.swimlane-chevron {
  display: inline-block;
  transition: transform 0.15s ease;
  font-size: 10px;
  flex-shrink: 0;
}

.swimlane-chevron.open {
  transform: rotate(90deg);
}

.swimlane-label { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.swimlane-count { font-weight: 400; font-size: var(--text-xs, 0.75rem); }

/* -- Column group cells (grouped mode) ---------------------------------- */
.column-group-cell {
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.board-col-header {
  position: sticky;
  top: 0;
  background: var(--bg);
  padding: 8px;
  border-bottom: 1px solid var(--border);
  z-index: var(--z-sticky);
  font-weight: 600;
  order: -1;
}

.board-col-header .warn { color: var(--color-danger-strong); font-weight: 600; }

/* -- Ticket selection --------------------------------------------------- */
.task--selected {
  outline: 2px solid var(--color-accent, #0969da);
  outline-offset: -1px;
  background: color-mix(in oklab, var(--color-accent, #0969da) 6%, var(--bg));
}

/* -- Member badges (assignee colour dots) ------------------------------- */
.member-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  font-size: 10px;
  font-weight: 700;
  color: #fff;
  flex-shrink: 0;
  line-height: 1;
}

.member-badge.small {
  width: 18px;
  height: 18px;
  font-size: 9px;
}

.member-inline {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: inherit;
  color: var(--muted);
}

/* -- Show more / progressive disclosure --------------------------------- */
.show-more-btn {
  display: block;
  width: 100%;
  padding: 6px 12px;
  border: none;
  border-top: 1px dashed var(--border);
  background: none;
  color: var(--color-accent, var(--fg));
  font-size: var(--text-sm, 0.875rem);
  cursor: pointer;
  text-align: center;
}

.show-more-btn:hover {
  background: color-mix(in oklab, var(--color-accent) 8%, transparent);
}
</style>
