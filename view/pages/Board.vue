<template>
  <section class="col" style="gap: 16px">
    <div class="row" style="justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px">
      <h1>
        Boards <span v-if="project" class="muted">— {{ project }}</span>
      </h1>
      <div class="row board-controls">
        <UiButton
          class="board-columns-button"
          type="button"
          data-testid="board-columns-button"
          @click="columnsDialogOpen = true"
        >
          <IconGlyph name="columns" aria-hidden="true" />
          <span>Columns</span>
        </UiButton>
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
      <FilterBar
        ref="filterBarRef"
        :statuses="statuses"
        :priorities="priorities"
        :types="types"
        :sprint-options="sprintFilterOptions"
        :custom-presets="customFilterPresets"
        :tag-options="filterOptions.tags"
        :assignee-options="filterOptions.assignees"
        :custom-field-values="filterOptions.customFieldValues"
        :custom-field-options="availableCustomFields"
        :value="filterPayload"
        :show-status="false"
        emit-project-key
        storage-key="lotar.boards.filter"
        @update:value="onFilterUpdate"
      >
        <template #actions>
          <ColumnsMenu
            :open="fieldsMenuOpen"
            :options="boardFieldsOptions"
            :is-visible="boardFields.isVisible"
            :set-visible="boardFields.toggleColumn"
            label="Card fields"
            @update:open="fieldsMenuOpen = $event"
            @reset="boardFields.resetColumns"
          >
            <template #trigger="{ open, toggle }">
              <UiButton
                class="responsive-action"
                type="button"
                aria-label="Configure card fields"
                title="Choose which fields appear on cards"
                :aria-expanded="open"
                @click="toggle"
              >
                <IconGlyph name="columns" aria-hidden="true" />
                <span class="responsive-action__label">Fields</span>
              </UiButton>
            </template>
          </ColumnsMenu>
          <select
            v-model="groupBy"
            class="input board-chips-row__groupby"
            aria-label="Group cards by"
            data-testid="board-groupby"
          >
            <option value="none">No grouping</option>
            <option value="assignee">Group by assignee</option>
            <option value="priority">Group by priority</option>
            <option value="type">Group by type</option>
          </select>
        </template>
      </FilterBar>
    </div>

    <div v-if="loadError && items.length" class="card refresh-error" role="alert">
      <span class="refresh-error__message">Board refresh failed: {{ loadError }}</span>
      <UiButton variant="ghost" type="button" :disabled="loadingTasks" @click="refreshBoardTasks()">Retry</UiButton>
    </div>

    <div
      v-if="project && !initialLoading && hiddenColumns.length"
      class="row board-hidden-note muted"
      data-testid="board-hidden-columns"
    >
      <span>
        {{ hiddenColumns.length }} {{ hiddenColumns.length === 1 ? 'column' : 'columns' }} hidden ({{
          hiddenColumns.map((col) => col.label).join(', ')
        }}) · {{ hiddenTaskCount }} {{ hiddenTaskCount === 1 ? 'task' : 'tasks' }}
      </span>
      <UiButton class="board-hidden-note__action" variant="ghost" type="button" @click="boardLayout.showAll()"
        >Show all</UiButton
      >
    </div>

    <div v-if="initialLoading" style="margin: 12px 0"><UiLoader>Loading board…</UiLoader></div>

    <div v-else-if="loadError && project && !items.length" style="margin: 12px 0">
      <UiEmptyState
        title="We couldn't load board tasks"
        :description="loadError"
        primary-label="Retry"
        @primary="refreshBoardTasks()"
      />
    </div>

    <div v-else-if="!project">
      <UiEmptyState title="Pick a project" description="Choose a project below to open its board.">
        <template #actions>
          <div class="col" style="gap: 4px; width: 320px; max-width: 100%">
            <label for="board-project-select">Project</label>
            <UiSelect
              id="board-project-select"
              :model-value="project"
              :disabled="loadingProjects || !projects.length"
              @update:model-value="onFilterUpdate({ ...filter, project: $event })"
            >
              <option value="" disabled>
                {{
                  loadingProjects ? 'Loading projects…' : projects.length ? 'Choose a project' : 'No projects available'
                }}
              </option>
              <option v-for="item in projects" :key="item.prefix" :value="item.prefix">
                {{ item.name && item.name !== item.prefix ? `${item.name} (${item.prefix})` : item.prefix }}
              </option>
            </UiSelect>
          </div>
        </template>
      </UiEmptyState>
    </div>

    <div v-else class="board grid" :style="gridStyle">
      <template v-if="groupBy !== 'none'">
        <template v-for="group in allGroupLabels" :key="group">
          <!-- Swimlane header spans all columns -->
          <div class="swimlane-row" :style="{ gridColumn: `1 / -1` }">
            <button
              type="button"
              class="swimlane-header"
              :class="{ collapsed: collapsedGroups.has(group) }"
              @click="toggleGroup(group)"
            >
              <span class="swimlane-chevron" :class="{ open: !collapsedGroups.has(group) }">▶</span>
              <span
                v-if="groupBy === 'assignee'"
                class="member-badge"
                :style="{ background: memberColor(group === '(none)' ? '' : group) }"
                :title="group"
                >{{ memberInitials(group === '(none)' ? '' : group) }}</span
              >
              <span class="swimlane-label">{{ group }}</span>
              <span class="muted swimlane-count">{{ groupTotalCount(group) }}</span>
            </button>
          </div>
          <!-- Per-lane tasks for this group; stacked lanes keep one labelled section per status -->
          <template v-if="!collapsedGroups.has(group)">
            <div
              v-for="lane in boardLanes"
              :key="`${group}::${lane.key}`"
              class="col column-group-cell"
              :data-status="lane.stacked ? undefined : statusAttr(lane.columns[0]!)"
              :data-column-kind="lane.stacked ? 'stack' : kindAttr(lane.columns[0]!)"
              :class="{ 'over-limit': laneOverLimit(lane), 'column-group-cell--stacked': lane.stacked }"
              @dragover.prevent="onDragOver"
              @drop.prevent="onLaneDrop(lane, $event)"
            >
              <div
                v-for="col in lane.columns"
                :key="col.key"
                class="column-section"
                :class="{ 'column-section--stacked': lane.stacked }"
                :data-section-key="col.key"
                :data-status="lane.stacked ? statusAttr(col) : undefined"
                :data-column-kind="lane.stacked ? kindAttr(col) : undefined"
              >
                <div v-if="lane.stacked" class="column-section__label muted">{{ col.label }}</div>
                <TransitionGroup name="task-list" tag="div" class="col-cards">
                  <article
                    v-for="task in groupedColumnTasks(col, group)"
                    :key="task.id"
                    class="card task"
                    :class="[priorityClass(task.priority), { 'task--selected': selectedTaskId === task.id }]"
                    draggable="true"
                    @dragstart="onDragStart(task)"
                    @dblclick="openTask(task.id)"
                    @click.exact="selectTask(task.id)"
                    @keydown.enter.prevent="openTask(task.id)"
                    tabindex="0"
                  >
                    <header v-if="hasTaskHeader(task)" class="row task-header">
                      <template v-if="hasTaskIdentity(task)">
                        <div class="row task-header__left">
                          <span v-if="boardFields.isVisible('id') && (task.id || '').trim()" class="muted id">{{
                            task.id
                          }}</span>
                          <strong v-if="boardFields.isVisible('title') && (task.title || '').trim()" class="title">{{
                            task.title
                          }}</strong>
                        </div>
                        <span
                          v-if="boardFields.isVisible('priority') && (task.priority || '').trim()"
                          class="priority"
                          :class="priorityClass(task.priority)"
                          >{{ task.priority }}</span
                        >
                      </template>
                      <template v-else>
                        <span
                          v-if="boardFields.isVisible('priority') && (task.priority || '').trim()"
                          class="priority"
                          :class="priorityClass(task.priority)"
                          >{{ task.priority }}</span
                        >
                      </template>
                    </header>
                    <BoardCardMeta
                      :task="task"
                      :has-header="hasTaskHeader(task)"
                      :due-info="taskDueInfo(task)"
                      :modified-info="taskModifiedInfo(task)"
                      :sprint="boardSprintHelpers"
                    />
                  </article>
                  <div
                    v-if="!groupedColumnTasks(col, group).length"
                    key="__group-empty__"
                    class="muted"
                    style="padding: 4px 0; font-size: var(--text-xs, 0.75rem)"
                  >
                    —
                  </div>
                </TransitionGroup>
              </div>
            </div>
          </template>
        </template>
        <!-- Lane headers at the very top (rendered first via CSS order) -->
        <div
          v-for="lane in boardLanes"
          :key="`hdr-${lane.key}`"
          class="col-header row board-col-header lane-header"
          :class="{ 'lane-header--stacked': lane.stacked }"
          :data-status="lane.stacked ? undefined : statusAttr(lane.columns[0]!)"
          :data-column-kind="lane.stacked ? 'stack' : kindAttr(lane.columns[0]!)"
          style="justify-content: space-between; align-items: center; gap: 8px"
        >
          <strong class="lane-header__title" :title="lane.title">{{ lane.title }}</strong>
          <span class="row" style="gap: 6px; align-items: center">
            <UiButton
              v-if="!lane.stacked"
              icon-only
              variant="ghost"
              class="board-col-add"
              type="button"
              :aria-label="lane.columns[0]!.synthetic ? 'Add task' : `Add task in ${lane.columns[0]!.label}`"
              :title="lane.columns[0]!.synthetic ? 'Add task' : `Add task in ${lane.columns[0]!.label}`"
              @click="lane.columns[0]!.synthetic ? openCreateInBoard() : openCreateInStatus(lane.columns[0]!.label)"
            >
              <IconGlyph name="plus" />
            </UiButton>
            <details
              v-else
              class="lane-add-menu"
              @keydown.escape="($event.currentTarget as HTMLDetailsElement).open = false"
            >
              <summary
                class="btn ghost icon-only board-col-add"
                :aria-label="`Add task in ${lane.title}`"
                :title="`Add task in ${lane.title}`"
              >
                <IconGlyph name="plus" />
              </summary>
              <div class="card lane-add-menu__list">
                <button
                  v-for="col in lane.columns"
                  :key="col.key"
                  type="button"
                  class="lane-add-menu__item"
                  @click="
                    ($event) => {
                      closeMenu($event)
                      openCreateInStatus(col.label)
                    }
                  "
                >
                  Add in {{ col.label }}
                </button>
              </div>
            </details>
            <span class="muted lane-header__count" :class="{ warn: laneOverLimit(lane) }">
              <template v-if="laneLimit(lane) > 0">{{ laneCount(lane) }} / {{ laneLimit(lane) }}</template>
              <template v-else>{{ laneCount(lane) }}</template>
            </span>
          </span>
        </div>
      </template>
      <template v-else>
        <div
          v-for="lane in boardLanes"
          :key="lane.key"
          class="col column"
          :data-status="lane.stacked ? undefined : statusAttr(lane.columns[0]!)"
          :data-column-kind="lane.stacked ? 'stack' : kindAttr(lane.columns[0]!)"
          :class="{ 'over-limit': laneOverLimit(lane), 'column--stacked': lane.stacked }"
          :tabindex="lane.stacked ? undefined : 0"
          @dragover.prevent="onDragOver"
          @drop.prevent="onLaneDrop(lane, $event)"
          @keydown.enter.prevent="!lane.stacked && onDrop(lane.columns[0]!)"
        >
          <div
            class="col-header row lane-header"
            :class="{ 'lane-header--stacked': lane.stacked }"
            style="justify-content: space-between; align-items: center; gap: 8px"
          >
            <strong class="lane-header__title" :title="lane.title">{{ lane.title }}</strong>
            <span class="row" style="gap: 6px; align-items: center">
              <UiButton
                v-if="!lane.stacked"
                icon-only
                variant="ghost"
                class="board-col-add"
                type="button"
                :aria-label="lane.columns[0]!.synthetic ? 'Add task' : `Add task in ${lane.columns[0]!.label}`"
                :title="lane.columns[0]!.synthetic ? 'Add task' : `Add task in ${lane.columns[0]!.label}`"
                @click="lane.columns[0]!.synthetic ? openCreateInBoard() : openCreateInStatus(lane.columns[0]!.label)"
              >
                <IconGlyph name="plus" />
              </UiButton>
              <details
                v-else
                class="lane-add-menu"
                @keydown.escape="($event.currentTarget as HTMLDetailsElement).open = false"
              >
                <summary
                  class="btn ghost icon-only board-col-add"
                  :aria-label="`Add task in ${lane.title}`"
                  :title="`Add task in ${lane.title}`"
                >
                  <IconGlyph name="plus" />
                </summary>
                <div class="card lane-add-menu__list">
                  <button
                    v-for="col in lane.columns"
                    :key="col.key"
                    type="button"
                    class="lane-add-menu__item"
                    @click="
                      ($event) => {
                        closeMenu($event)
                        openCreateInStatus(col.label)
                      }
                    "
                  >
                    Add in {{ col.label }}
                  </button>
                </div>
              </details>
              <span class="muted lane-header__count" :class="{ warn: laneOverLimit(lane) }">
                <template v-if="laneLimit(lane) > 0">{{ laneCount(lane) }} / {{ laneLimit(lane) }}</template>
                <template v-else>{{ laneCount(lane) }}</template>
              </span>
            </span>
          </div>
          <!-- One section per status; a single-status lane renders its cards directly -->
          <div
            v-for="col in lane.columns"
            :key="col.key"
            class="column-section"
            :class="{ 'column-section--stacked': lane.stacked }"
            :data-section-key="col.key"
            :data-status="lane.stacked ? statusAttr(col) : undefined"
            :data-column-kind="lane.stacked ? kindAttr(col) : undefined"
            :tabindex="lane.stacked ? 0 : undefined"
            @keydown.enter.self.prevent="lane.stacked && onDrop(col)"
          >
            <div v-if="lane.stacked" class="row column-section__head">
              <span class="column-section__label muted">{{ col.label }}</span>
              <span class="row" style="gap: 4px; align-items: center">
                <UiButton
                  icon-only
                  variant="ghost"
                  class="board-col-add"
                  type="button"
                  :aria-label="`Add task in ${col.label}`"
                  :title="`Add task in ${col.label}`"
                  @click="openCreateInStatus(col.label)"
                >
                  <IconGlyph name="plus" />
                </UiButton>
                <span class="muted column-section__count">{{ countOf(col) }}</span>
              </span>
            </div>
            <TransitionGroup name="task-list" tag="div" class="col-cards">
              <article
                v-for="task in visibleFlatTasks(col)"
                :key="task.id"
                class="card task"
                :class="[priorityClass(task.priority), { 'task--selected': selectedTaskId === task.id }]"
                draggable="true"
                @dragstart="onDragStart(task)"
                @dblclick="openTask(task.id)"
                @click.exact="selectTask(task.id)"
                @keydown.enter.prevent="openTask(task.id)"
                tabindex="0"
              >
                <header v-if="hasTaskHeader(task)" class="row task-header">
                  <template v-if="hasTaskIdentity(task)">
                    <div class="row task-header__left">
                      <span v-if="boardFields.isVisible('id') && (task.id || '').trim()" class="muted id">{{
                        task.id
                      }}</span>
                      <strong v-if="boardFields.isVisible('title') && (task.title || '').trim()" class="title">{{
                        task.title
                      }}</strong>
                    </div>
                    <span
                      v-if="boardFields.isVisible('priority') && (task.priority || '').trim()"
                      class="priority"
                      :class="priorityClass(task.priority)"
                      >{{ task.priority }}</span
                    >
                  </template>
                  <template v-else>
                    <span
                      v-if="boardFields.isVisible('priority') && (task.priority || '').trim()"
                      class="priority"
                      :class="priorityClass(task.priority)"
                      >{{ task.priority }}</span
                    >
                  </template>
                </header>
                <BoardCardMeta
                  :task="task"
                  :has-header="hasTaskHeader(task)"
                  :due-info="taskDueInfo(task)"
                  :modified-info="taskModifiedInfo(task)"
                  :sprint="boardSprintHelpers"
                />
              </article>
              <div v-if="!columnTasks(col).length" key="__empty__" class="muted" style="padding: 8px">No tasks</div>
            </TransitionGroup>
            <button v-if="hiddenFlatCount(col) > 0" type="button" class="show-more-btn" @click="showMore(col.key)">
              Show {{ hiddenFlatCount(col) }} more…
            </button>
          </div>
        </div>
      </template>
    </div>
    <BoardColumnsDialog
      :open="columnsDialogOpen"
      :statuses="layoutStatuses"
      :layout="boardLayout.layout.value"
      @close="columnsDialogOpen = false"
      @apply="applyColumnsLayout"
    />
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, shallowRef, watch } from 'vue'
import { useRoute } from 'vue-router'
import { api } from '../api/client'
import type { TaskDTO } from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import BoardCardMeta from '../components/BoardCardMeta.vue'
import ColumnsMenu from '../components/ColumnsMenu.vue'
import IconGlyph from '../components/IconGlyph.vue'
import ReloadButton from '../components/ReloadButton.vue'
import { showToast } from '../components/toast'
import UiButton from '../components/UiButton.vue'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiLoader from '../components/UiLoader.vue'
import UiSelect from '../components/UiSelect.vue'
import { useColumns, provideColumnStore } from '../composables/useColumns'
import { computeBoardLanes, useBoardLayout, type BoardLane, type BoardLayout } from '../composables/useBoardLayout'
import BoardColumnsDialog from '../components/BoardColumnsDialog.vue'
import { useCompletionPolicy, completionStatusKey, normalizeStatusKey } from '../composables/useCompletionPolicy'
import { useConfig } from '../composables/useConfig'
import { useFilterOptions } from '../composables/useFilterOptions'
import { buildServerFilter, useCustomFilterPresets, useProjectFilterSync } from '../composables/useFilterBuilder'
import { MS_PER_DAY } from '../utils/date'
import { storageGet, storageSet } from '../utils/storage'
import { useProjects } from '../composables/useProjects'
import { useSprintFormatting } from '../composables/useSprintFormatting'
import { useSprintFilterOptions, useSprints } from '../composables/useSprints'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { useTaskStore, type TaskQueryHandle } from '../composables/useTaskStore'
import { parseTaskDate, startOfLocalDay } from '../utils/date'
import { sortTasks } from '../utils/taskSort'
import { memberColor, memberInitials } from '../utils/member'
import { statusEnteredAt } from '../utils/taskHistory'

const route = useRoute()
const { projects, refresh: refreshProjects, loading: loadingProjects } = useProjects()
const {
  statuses,
  priorities,
  types,
  customFields: availableCustomFields,
  refresh: refreshConfig,
  loading: loadingConfig,
  scope: configScope,
  tags: configuredTags,
  members,
} = useConfig()
const { sprints, refresh: refreshSprints } = useSprints()
const store = useTaskStore()
// DEV-65: read the board's keyed query so other consumers (panel, routes in
// flight) cannot overwrite this board's results, and so SSE-triggered
// authoritative refreshes keep this membership current while mounted.
const boardQuery = shallowRef<TaskQueryHandle | null>(null)
let activeQueryHandle: TaskQueryHandle | null = null
let activeQueryKey = ''
function adoptQuery(handle: TaskQueryHandle) {
  if (activeQueryKey === handle.key) return
  activeQueryHandle?.release()
  activeQueryHandle = handle
  activeQueryKey = handle.key
  handle.retain()
  boardQuery.value = handle
}
const loadingTasks = computed(() => boardQuery.value?.status.value === 'loading')
const loadError = computed(() => boardQuery.value?.error.value ?? null)
const items = computed(() => boardQuery.value?.tasks.value ?? [])
// DEV-21: shared per-project completion policy (terminal statuses, due
// buckets) resolved from server config; never re-inferred client-side.
const completion = useCompletionPolicy({ tasks: () => items.value })
const { openTaskPanel } = useTaskPanelController()

const project = ref<string>(route.query.project ? String(route.query.project) : '')
const filterOptions = useFilterOptions(
  () => items.value,
  () => project.value,
  {
    scope: () => configScope.value,
    tags: () => configuredTags.value,
    members: () => members.value,
  },
)
const draggingId = ref<string>('')
const filter = ref<Record<string, string>>(typeof route.query.q === 'string' ? { q: route.query.q } : {})
const filterBarRef = ref<{ clear?: () => void } | null>(null)
const columnsDialogOpen = ref(false)
const fieldsMenuOpen = ref(false)
const filterPayload = computed(() => ({
  ...filter.value,
  ...(project.value || Object.prototype.hasOwnProperty.call(route.query, 'project') ? { project: project.value } : {}),
}))
const {
  hasFilters,
  onFilterUpdate,
  clearFilters: clearFiltersAction,
} = useProjectFilterSync(project, filter, {
  routePath: '/boards',
})
const customFilterPresets = useCustomFilterPresets(availableCustomFields)

const { sprintLabel, sprintStateClass, sprintTooltip } = useSprintFormatting(sprints)

const sprintFilterOptions = useSprintFilterOptions(sprints)

// -- Swimlane group-by (persisted per project) ----------------------------
type GroupByMode = 'none' | 'assignee' | 'priority' | 'type'
function groupByKey() {
  return project.value ? `lotar.boardGroupBy::${project.value}` : 'lotar.boardGroupBy'
}
function loadGroupBy(): GroupByMode {
  const v = storageGet(groupByKey())
  if (v === 'assignee' || v === 'priority' || v === 'type') return v
  return 'none'
}
const groupBy = ref<GroupByMode>(loadGroupBy())
function saveGroupBy() {
  storageSet(groupByKey(), groupBy.value)
}

// -- Initial loading --------------------------------------------------------
// Per-current-handle first load: the loader covers the loading, queued-idle,
// and config phases of a key without a snapshot. A background refresh of an
// established key (hasSnapshot) keeps the grid mounted.
const initialLoading = computed(() => {
  const query = boardQuery.value
  if (!query || query.hasSnapshot.value || query.error.value) return false
  return loadingConfig.value || query.status.value === 'loading' || query.status.value === 'idle'
})

// -- Ticket highlight on single click -------------------------------------
const selectedTaskId = ref('')
function selectTask(id: string) {
  selectedTaskId.value = selectedTaskId.value === id ? '' : id
}

// -- Collapsible groups ---------------------------------------------------
const collapsedGroups = ref<Set<string>>(new Set())
function toggleGroup(label: string) {
  const next = new Set(collapsedGroups.value)
  if (next.has(label)) next.delete(label)
  else next.add(label)
  collapsedGroups.value = next
}

// -- Progressive disclosure (virtual scrolling) ---------------------------
const COLUMN_PAGE_SIZE = 30
const columnExpansion = ref<Record<string, number>>({})
function visibleLimit(st: string): number {
  return columnExpansion.value[st] || COLUMN_PAGE_SIZE
}
function showMore(st: string) {
  columnExpansion.value = { ...columnExpansion.value, [st]: visibleLimit(st) + COLUMN_PAGE_SIZE }
}
function resetExpansion() {
  columnExpansion.value = {}
}

function startOfDay(date: Date) {
  return startOfLocalDay(date)
}

function parseDateLike(value?: string | null) {
  return parseTaskDate(value)
}

function priorityClass(value: string | undefined): string {
  const v = (value || '').trim().toLowerCase()
  if (v === 'critical') return 'priority--critical'
  if (v === 'high') return 'priority--high'
  if (v === 'low') return 'priority--low'
  return ''
}

function taskDueInfo(task: TaskDTO): { label: string; overdue: boolean } {
  const raw = (task.due_date || '').trim()
  if (!raw) return { label: '', overdue: false }
  const parsed = parseDateLike(raw)
  if (!parsed) return { label: raw, overdue: false }

  const today = startOfDay(new Date())
  const due = startOfDay(parsed)
  const diffDays = Math.round((due.getTime() - today.getTime()) / MS_PER_DAY)
  const sameYear = parsed.getFullYear() === today.getFullYear()
  const dateLabel = parsed.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: sameYear ? undefined : 'numeric',
  })
  if (diffDays < 0) {
    // Shared server-basis predicate (terminal tasks are never overdue;
    // stale authoritative buckets follow the server day, not this local
    // diffDays) — keeps the card label consistent with hover/table/sprints.
    const overdue = completion.isTaskOverdue(task)
    return { label: overdue ? `Overdue ${dateLabel}` : `Due ${dateLabel}`, overdue }
  }
  return { label: `Due ${dateLabel}`, overdue: false }
}

function taskModifiedInfo(task: TaskDTO): string {
  const raw = (task.modified || '').trim()
  if (!raw) return ''
  let parsed: Date | null
  try {
    const d = new Date(raw)
    parsed = Number.isFinite(d.getTime()) ? d : null
  } catch {
    parsed = null
  }
  if (!parsed) return `Updated ${raw}`
  const now = new Date()
  const sameYear = parsed.getFullYear() === now.getFullYear()
  const dateLabel = parsed.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: sameYear ? undefined : 'numeric',
  })
  return `Updated ${dateLabel}`
}

function hasTaskHeader(task: TaskDTO): boolean {
  return Boolean(
    (boardFields.isVisible('id') && (task.id || '').trim()) ||
    (boardFields.isVisible('title') && (task.title || '').trim()) ||
    (boardFields.isVisible('priority') && (task.priority || '').trim()),
  )
}

function hasTaskIdentity(task: TaskDTO): boolean {
  return Boolean(
    (boardFields.isVisible('id') && (task.id || '').trim()) ||
    (boardFields.isVisible('title') && (task.title || '').trim()),
  )
}

const hasAnyFilters = computed(() => hasFilters.value || groupBy.value !== 'none')

function clearFilters() {
  clearFiltersAction(filterBarRef)
  groupBy.value = 'none'
  saveGroupBy()
  collapsedGroups.value = new Set()
}

async function refreshBoardTasks(snapshot?: Record<string, string>) {
  if (!project.value) {
    return
  }
  const raw = snapshot ?? filter.value
  const { serverFilter } = buildServerFilter(raw, project.value)
  adoptQuery(store.getQuery(serverFilter))
  // Refresh failures surface through the query's error state (loadError);
  // previously they were swallowed by the shared status with stale rows left behind.
  await activeQueryHandle!.refresh()
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

// DEV-130: per-project board layout (column order, stacked and hidden
// statuses, per-column WIP limits and card rules).
const layoutStatuses = computed(() => columnsData.value.map(({ label, norm }) => ({ key: norm, label })))
const boardLayout = useBoardLayout(project, layoutStatuses)
boardLayout.load()

const columnLookup = computed(() => {
  const map = new Map<string, string>()
  columnsData.value.forEach(({ label, norm }) => {
    map.set(norm, label)
  })
  return map
})

// DEV-68 shared ordered base: ONE deterministic pass (project-prefix guard +
// server-basis display sort) partitions tasks into configured columns plus a
// synthetic fallback bucket for unknown custom statuses. Flat and grouped
// renders derive from the same buckets, so ordering, counts, and membership
// cannot diverge. Filter semantics stay server-authoritative — this pass only
// orders and buckets the query's own rows.
const partitionedBoardTasks = computed<{ configured: Record<string, TaskDTO[]>; other: TaskDTO[] }>(() => {
  const lookup = columnLookup.value
  const activeProject = project.value
  // Smart filters (due/recent/needs/assignee) are applied server-side by the
  // hydrate; only the deterministic display order is applied here.
  const { normalized } = buildServerFilter(filter.value, activeProject)
  const sorted = sortTasks(items.value || [], 'modified', normalized.order === 'asc' ? 'asc' : 'desc')
  const configured: Record<string, TaskDTO[]> = {}
  for (const { label } of columnsData.value) configured[label] = []
  const other: TaskDTO[] = []
  for (const t of sorted) {
    if (!activeProject || !t.id.startsWith(`${activeProject}-`)) continue
    const key = lookup.get(normalizeStatusKey(t.status))
    if (key) (configured[key] ??= []).push(t)
    else other.push(t)
  }
  return { configured, other }
})

const grouped = computed<Record<string, TaskDTO[]>>(() => applyCardRules(partitionedBoardTasks.value.configured))

/**
 * Per-column card rules from the board layout (DEV-130): hide cards that
 * entered their status more than N days ago (see statusEnteredAt) and/or keep only the N
 * most recently modified cards. Rules apply to each status section of the
 * column; hidden cards still exist and stay searchable.
 */
function applyCardRules(groups: Record<string, TaskDTO[]>) {
  const configByLabel = new Map<string, { maxAgeDays?: number; maxVisible?: number }>()
  for (const column of boardLayout.layout.value.columns) {
    if (!column.maxAgeDays && !column.maxVisible) continue
    for (const key of column.statuses) {
      const label = columnLookup.value.get(key)
      if (label) configByLabel.set(label, column)
    }
  }
  if (!configByLabel.size) return groups
  const now = Date.now()
  const result: Record<string, TaskDTO[]> = {}
  Object.entries(groups).forEach(([label, tasks]) => {
    const rules = configByLabel.get(label)
    if (!rules) {
      result[label] = tasks
      return
    }
    let filtered = Array.isArray(tasks) ? [...tasks] : []
    if (rules.maxAgeDays) {
      const ageMs = rules.maxAgeDays * MS_PER_DAY
      filtered = filtered.filter((task) => {
        const ts = statusEnteredAt(task, label)
        return ts === null || now - ts <= ageMs
      })
    }
    if (rules.maxVisible) {
      filtered.sort((a, b) => b.modified.localeCompare(a.modified))
      filtered = filtered.slice(0, rules.maxVisible)
    }
    result[label] = filtered
  })
  return result
}

// Column descriptors drive every render branch (flat + grouped). `key` is the
// normalized status for configured columns and a reserved sentinel for the
// synthetic fallback; normalizeStatusKey strips underscores, so no configured
// status can ever collide with the sentinel key — a user workflow status
// literally named "Other" or "__other__" stays a real configured column.
// The synthetic bucket is presentation-only: it is never a completion
// status, never a done-filter target, and never a status-write target.
const OTHER_COLUMN_KEY = '__other__'
type BoardColumnDescriptor = {
  key: string
  label: string
  norm: string
  synthetic: boolean
}

const boardColumns = computed<BoardColumnDescriptor[]>(() => {
  const cols: BoardColumnDescriptor[] = columnsData.value.map(({ label, norm }) => ({
    key: norm,
    label,
    norm,
    synthetic: false,
  }))
  if (partitionedBoardTasks.value.other.length) {
    cols.push({ key: OTHER_COLUMN_KEY, label: 'Other', norm: OTHER_COLUMN_KEY, synthetic: true })
  }
  return cols
})

// DEV-130: per-project column layout (hidden and stacked statuses). Lanes are
// the visual columns; each lane holds one or more status columns, and every
// status keeps its own section, counts, WIP limit and drop target.
const boardLanes = computed<BoardLane<BoardColumnDescriptor>[]>(() =>
  computeBoardLanes(boardColumns.value, boardLayout.layout.value),
)
const hiddenColumns = computed(() =>
  boardColumns.value.filter((col) => !col.synthetic && boardLayout.isHidden(col.key)),
)
const hiddenTaskCount = computed(() =>
  hiddenColumns.value.reduce((sum, col) => sum + (partitionedBoardTasks.value.configured[col.label]?.length ?? 0), 0),
)

function closeMenu(event: Event) {
  const menu = (event.currentTarget as HTMLElement | null)?.closest('details')
  if (menu) menu.open = false
}

function applyColumnsLayout(next: BoardLayout) {
  boardLayout.save(next)
  columnsDialogOpen.value = false
}

function statusAttr(col: BoardColumnDescriptor): string {
  return col.synthetic ? OTHER_COLUMN_KEY : col.label
}

function kindAttr(col: BoardColumnDescriptor): string {
  return col.synthetic ? 'other' : 'status'
}

/** Drops on a stacked lane land in the status section under the pointer. */
function onLaneDrop(lane: BoardLane<BoardColumnDescriptor>, event: DragEvent) {
  if (!lane.stacked) {
    void onDrop(lane.columns[0]!)
    return
  }
  const target = event.target instanceof Element ? event.target.closest('[data-section-key]') : null
  const key = target?.getAttribute('data-section-key')
  const col = lane.columns.find((entry) => entry.key === key)
  if (col) void onDrop(col)
  else draggingId.value = ''
}

function columnTasks(col: BoardColumnDescriptor): TaskDTO[] {
  return col.synthetic ? partitionedBoardTasks.value.other : grouped.value[col.label] || []
}

// -- Aligned swimlane group-by ---------------------------------------------
function groupKeyFor(task: TaskDTO): string {
  if (groupBy.value === 'assignee') return (task.assignee || '').trim() || '(none)'
  if (groupBy.value === 'priority') return (task.priority || '').trim() || '(none)'
  if (groupBy.value === 'type') return (task.task_type || '').trim() || '(none)'
  return ''
}

const allGroupLabels = computed<string[]>(() => {
  if (groupBy.value === 'none') return []
  const labels = new Set<string>()
  for (const col of boardColumns.value) {
    for (const t of columnTasks(col)) labels.add(groupKeyFor(t))
  }
  const source = groupBy.value === 'priority' ? priorities.value : groupBy.value === 'type' ? types.value : []
  // Match the server's enum keys without folding distinct non-ASCII labels.
  const keyFor = (label: string) =>
    label
      .trim()
      .replace(/[A-Z]/g, (char) => char.toLowerCase())
      .replace(/[ _-]/g, '')
  const ranks = new Map<string, number>()
  source.forEach((label, index) => {
    const key = keyFor(label)
    if (key && !ranks.has(key)) ranks.set(key, groupBy.value === 'priority' ? -index : index)
  })
  const arr = Array.from(labels).sort((a, b) => {
    if (a === b) return 0
    if (a === '(none)') return 1
    if (b === '(none)') return -1
    const aRank = ranks.get(keyFor(a)) ?? Infinity
    const bRank = ranks.get(keyFor(b)) ?? Infinity
    if (aRank !== bRank) return aRank - bRank
    return a.localeCompare(b)
  })
  return arr
})

function groupedColumnTasks(col: BoardColumnDescriptor, group: string): TaskDTO[] {
  return columnTasks(col).filter((t) => groupKeyFor(t) === group)
}

function groupTotalCount(group: string): number {
  let count = 0
  for (const col of boardColumns.value) {
    count += columnTasks(col).filter((t) => groupKeyFor(t) === group).length
  }
  return count
}

// -- Flat (ungrouped) column helpers --------------------------------------
function visibleFlatTasks(col: BoardColumnDescriptor): TaskDTO[] {
  const all = columnTasks(col)
  const limit = visibleLimit(col.key)
  return all.length <= limit ? all : all.slice(0, limit)
}

function hiddenFlatCount(col: BoardColumnDescriptor): number {
  const all = columnTasks(col)
  const limit = visibleLimit(col.key)
  return all.length <= limit ? 0 : all.length - limit
}

const gridStyle = computed(() => ({
  display: 'grid',
  gridTemplateColumns: `repeat(${Math.max(boardLanes.value.length, 1)}, minmax(260px, 1fr))`,
  gap: '12px',
}))

// --- WIP limits: one limit per board column, counting all of its statuses ---
function laneCount(lane: BoardLane<BoardColumnDescriptor>): number {
  return lane.columns.reduce((sum, col) => sum + columnTasks(col).length, 0)
}
function laneLimit(lane: BoardLane<BoardColumnDescriptor>): number {
  return lane.config.wipLimit ?? 0
}
function laneOverLimit(lane: BoardLane<BoardColumnDescriptor>): boolean {
  const limit = laneLimit(lane)
  return limit > 0 && laneCount(lane) > limit
}
function countOf(col: BoardColumnDescriptor): number {
  return columnTasks(col).length
}

watch(groupBy, () => {
  saveGroupBy()
})

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

const boardFields = useColumns({
  storagePrefix: 'lotar.boardFields',
  defaultVisible: Object.entries(DEFAULT_BOARD_FIELDS)
    .filter(([, on]) => on)
    .map(([key]) => key),
})
provideColumnStore(boardFields)

const boardFieldsOptions = computed(() => boardFields.fieldOptions.value)

const boardSprintHelpers = {
  label: sprintLabel,
  stateClass: sprintStateClass,
  tooltip: sprintTooltip,
}

watch(availableCustomFields, (keys) => boardFields.setCustomFieldKeys(keys), { immediate: true })

watch(project, (next) => boardFields.setProjectKey(next), { immediate: true })

function onDragStart(t: any) {
  draggingId.value = t.id
}
function onDragOver(ev: DragEvent) {
  ev.preventDefault()
}
async function onDrop(target: BoardColumnDescriptor) {
  const id = draggingId.value
  if (!id) return
  draggingId.value = ''
  // The synthetic fallback column is presentation-only: a drop there must
  // never write a status. Configured columns carry real workflow statuses —
  // including a legitimate configured literal "__other__", which must keep
  // writing its real status value.
  if (target.synthetic) return
  const targetStatus = target.label
  const existing = items.value.find((t) => t.id === id)
  // DEV-102: dropping a card on the column its effective status already
  // occupies is a pure no-op — no optimistic upsert, no status POST, no
  // toast, no refresh. The comparison mirrors the server's ASCII
  // case-folded configured-label matching (a task's "todo" already satisfies
  // the configured "Todo" column) but deliberately does NOT strip
  // separators, so distinct raw statuses that merely share a bucket — e.g.
  // a task's "Other" inside a configured "__other__" column — still produce
  // a real status write judged by the server. The board has no
  // within-column order payload today; if column-local reordering ever
  // gains persistence, this guard must be revisited first.
  if (existing && completionStatusKey(existing.status) === completionStatusKey(targetStatus)) {
    return
  }
  try {
    // Optimistic move via store (resolved against this board's query)
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

function openCreateInStatus(status: string) {
  openTaskPanel({
    taskId: 'new',
    initialProject: project.value || null,
    initialStatus: status,
    onCreated: () => {
      refreshAll()
    },
  })
}

function openCreateInBoard() {
  openTaskPanel({
    taskId: 'new',
    initialProject: project.value || null,
    onCreated: () => {
      refreshAll()
    },
  })
}

async function refreshAll() {
  await refreshProjects()
  await refreshConfig(project.value)
  await refreshSprints(true)
  await refreshBoardTasks()
}

let filterDebounce: ReturnType<typeof setTimeout> | null = null

watch(
  filter,
  (value) => {
    if (filterDebounce) clearTimeout(filterDebounce)
    resetExpansion()
    const snapshot = { ...value }
    filterDebounce = setTimeout(() => {
      refreshBoardTasks(snapshot).catch((err) => {
        console.warn('Failed to refresh board after filter change', err)
      })
    }, 150)
  },
  { deep: true },
)

onMounted(async () => {
  await refreshProjects()
  await refreshSprints(true)
  await refreshConfig(project.value)
  await refreshBoardTasks()
})

watch(
  () => route.query.project,
  async (value) => {
    project.value = value ? String(value) : ''
    await refreshConfig(project.value)
    await refreshBoardTasks()
    boardLayout.load()
    groupBy.value = loadGroupBy()
    collapsedGroups.value = new Set()
    resetExpansion()
  },
)

onUnmounted(() => {
  if (filterDebounce) {
    clearTimeout(filterDebounce)
    filterDebounce = null
  }
  activeQueryHandle?.release()
  activeQueryHandle = null
  activeQueryKey = ''
  boardQuery.value = null
})
</script>

<style scoped>
.board {
  align-items: flex-start;
  /* Kanban columns keep a 260px minimum width; on narrow viewports the board
     itself becomes the horizontal scroll container instead of the page. */
  overflow-x: auto;
  max-width: 100%;
  padding-bottom: 4px;
}
.column {
  border: 1px solid var(--border);
  border-radius: var(--radius-base);
  background: var(--bg);
  min-height: 200px;
  display: flex;
  flex-direction: column;
}
.column.over-limit {
  border-color: color-mix(in oklab, var(--color-danger) 40%, var(--border));
}
.board-col-add {
  min-width: var(--control-height);
  min-height: var(--control-height);
  opacity: 0;
  transition: opacity var(--duration-fast) var(--ease-standard);
}

.col-header:hover .board-col-add,
.board-col-add:focus-visible {
  opacity: 1;
}

@media (hover: none) {
  .board-col-add {
    opacity: 1;
  }
}

.col-header {
  position: sticky;
  top: 0;
  background: var(--bg);
  padding: 8px;
  border-bottom: 1px solid var(--border);
  border-top-left-radius: var(--radius-base);
  border-top-right-radius: var(--radius-base);
  z-index: var(--z-sticky);
}
.col-header .warn {
  color: var(--color-danger-strong);
  font-weight: 600;
}
.col-cards {
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  position: relative;
}
.task {
  padding: 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-base);
  cursor: grab;
  user-select: none;
}
.task:active {
  cursor: grabbing;
}
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
.priority {
  font-size: 12px;
  color: var(--muted);
  flex: 0 0 auto;
  white-space: nowrap;
}
.priority.priority--critical {
  color: var(--danger, #d93025);
  font-weight: 600;
}
.priority.priority--high {
  color: var(--warning, #e8710a);
  font-weight: 600;
}
.priority.priority--low {
  opacity: 0.6;
}
.task.priority--critical {
  border-left: 3px solid var(--danger, #d93025);
}
.task.priority--high {
  border-left: 3px solid var(--warning, #e8710a);
}
.column:focus {
  outline: 2px solid color-mix(in oklab, var(--fg) 30%, transparent);
  outline-offset: 2px;
}
.board-controls {
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}
.board-controls :is(.ui-select, .btn) {
  min-height: var(--control-height);
  padding-top: 0;
  padding-bottom: 0;
}
.board-controls .btn.icon-only {
  width: var(--control-height);
  height: var(--control-height);
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
  padding: 0 var(--space-4, 1rem);
  height: var(--control-height);
  min-height: var(--control-height);
  line-height: var(--line-tight, 1.25);
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
.task-meta__sprints,
.task-meta__custom-field {
  gap: 6px;
  flex-wrap: wrap;
  align-items: center;
}

.task-meta__custom-field__label {
  font-weight: 500;
}

.task-meta__custom-field__value {
  color: var(--fg);
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

.swimlane-label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.swimlane-count {
  font-weight: 400;
  font-size: var(--text-xs, 0.75rem);
}

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

.board-col-header .warn {
  color: var(--color-danger-strong);
  font-weight: 600;
}

/* -- Column layout: lanes, stacked statuses, hidden notice (DEV-130) ----- */
/* A single-status lane renders its cards inline, exactly like a plain column. */
.column-section {
  display: contents;
}
.column-section--stacked {
  display: flex;
  flex-direction: column;
}
.column-section--stacked + .column-section--stacked {
  border-top: 1px solid var(--border);
}
.column-section--stacked:focus-visible {
  outline: 2px solid color-mix(in oklab, var(--fg) 30%, transparent);
  outline-offset: -2px;
}
.column-section__head {
  justify-content: space-between;
  align-items: center;
  gap: 6px;
  padding: 6px 8px 0;
}
.column-section__head:hover .board-col-add {
  opacity: 1;
}
.column-section__label {
  font-size: var(--text-xs, 0.75rem);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.column-section__count {
  font-size: var(--text-xs, 0.75rem);
}
.column-group-cell .column-section__label {
  display: block;
  padding: 6px 8px 0;
}
.column-group-cell .column-section--stacked + .column-section--stacked {
  border-top: 1px dashed var(--border);
}
.lane-header__title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.lane-header__count.warn {
  color: var(--color-danger-strong);
  font-weight: 600;
}
.column.over-limit > .lane-header,
.board-col-header.lane-header:has(.warn) {
  box-shadow: inset 0 -2px 0 color-mix(in oklab, var(--color-danger) 45%, transparent);
}
.lane-add-menu {
  position: relative;
  display: inline-flex;
}
.lane-add-menu > summary {
  list-style: none;
  display: inline-flex;
  box-sizing: border-box;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}
.lane-add-menu > summary::-webkit-details-marker {
  display: none;
}
.lane-add-menu[open] > summary {
  opacity: 1;
}
.lane-add-menu__list {
  position: absolute;
  right: 0;
  top: calc(100% + 4px);
  z-index: var(--z-popover, 50);
  display: flex;
  flex-direction: column;
  min-width: 160px;
  padding: 4px;
  gap: 2px;
}
.lane-add-menu__item {
  text-align: left;
  border: none;
  background: transparent;
  color: inherit;
  padding: 6px 8px;
  border-radius: var(--radius-sm, 6px);
  cursor: pointer;
  font: inherit;
}
.lane-add-menu__item:hover,
.lane-add-menu__item:focus-visible {
  background: color-mix(in oklab, var(--fg) 8%, transparent);
}
.board-columns-button {
  gap: var(--space-2, 0.5rem);
}
.board-hidden-note {
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-top: -8px;
  font-size: var(--text-sm, 0.875rem);
}
.board-hidden-note__action {
  white-space: nowrap;
}

/* -- Refresh error banner ------------------------------------------------ */
.refresh-error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
  border-left: 3px solid var(--color-danger, #c62828);
}

.refresh-error__message {
  color: var(--color-danger-strong, var(--color-danger, #c62828));
}

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
