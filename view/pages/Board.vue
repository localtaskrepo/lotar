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
        <UiButton
          icon-only
          type="button"
          aria-label="Clear filters"
          title="Clear filters"
          :disabled="!hasFilters"
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
      <SmartListChips
        :statuses="statuses"
        :priorities="priorities"
        :value="filter"
        :custom-presets="customFilterPresets"
        @update:value="onChipsUpdate"
        @preset="handleCustomPreset"
      />
      <div class="row board-filter-row">
        <FilterBar
          ref="filterBarRef"
          class="board-filter-row__bar"
          :statuses="statuses"
          :priorities="priorities"
          :types="types"
          :value="filterPayload"
          :show-status="false"
          emit-project-key
          storage-key="lotar.boards.filter"
          @update:value="onFilterUpdate"
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

    <div v-if="loadingConfig || loadingTasks" style="margin: 12px 0;"><UiLoader>Loading board…</UiLoader></div>

    <div v-else-if="!project">
      <UiEmptyState title="Pick a project" description="Boards are per-project. Choose a project to view its board." />
    </div>

    <div v-else class="board grid" :style="gridStyle">
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
        <div class="col-cards">
          <article v-for="t in (grouped[st] || [])" :key="t.id" class="card task"
                   draggable="true"
                   @dragstart="onDragStart(t)"
                   @dblclick="openTask(t.id)"
                   @keydown.enter.prevent="openTask(t.id)"
                   tabindex="0">
            <header v-if="hasTaskHeader(t)" class="row task-header">
              <template v-if="hasTaskIdentity(t)">
                <div class="row task-header__left">
                  <span v-if="isBoardFieldVisible('id') && (t.id || '').trim()" class="muted id">{{ t.id }}</span>
                  <strong v-if="isBoardFieldVisible('title') && (t.title || '').trim()" class="title">{{ t.title }}</strong>
                </div>
                <span v-if="isBoardFieldVisible('priority') && (t.priority || '').trim()" class="priority">{{ t.priority }}</span>
              </template>
              <template v-else>
                <span v-if="isBoardFieldVisible('priority') && (t.priority || '').trim()" class="priority">{{ t.priority }}</span>
              </template>
            </header>
            <footer v-if="hasTaskMeta(t)" class="task-meta" :class="{ 'task-meta--no-header': !hasTaskHeader(t) }">
              <div v-if="hasPrimaryMeta(t)" class="row task-meta__tags">
                <span v-if="isBoardFieldVisible('status') && (t.status || '').trim()" class="muted">{{ t.status }}</span>
                <span v-if="isBoardFieldVisible('task_type') && (t.task_type || '').trim()" class="muted">{{ t.task_type }}</span>
                <span v-if="isBoardFieldVisible('effort') && (t.effort || '').trim()" class="muted">{{ t.effort }}</span>
                <span v-if="isBoardFieldVisible('reporter') && (t.reporter || '').trim()" class="muted">by @{{ t.reporter }}</span>
                <span v-if="isBoardFieldVisible('assignee') && (t.assignee || '').trim()" class="muted">@{{ t.assignee }}</span>
                <span v-if="isBoardFieldVisible('due_date') && taskDueInfo(t).label" class="task-meta__due" :class="{ 'is-overdue': taskDueInfo(t).overdue }">{{ taskDueInfo(t).label }}</span>
                <span v-if="isBoardFieldVisible('modified') && taskModifiedInfo(t)" class="muted">{{ taskModifiedInfo(t) }}</span>
                <span v-if="isBoardFieldVisible('tags')" v-for="tag in (t.tags || [])" :key="tag" class="tag">{{ tag }}</span>
              </div>
              <div v-if="isBoardFieldVisible('sprints') && t.sprints?.length" class="row task-meta__sprints">
                <span
                  v-for="sprintId in t.sprints"
                  :key="`${t.id}-sprint-${sprintId}`"
                  class="chip small sprint-chip"
                  :class="sprintStateClass(sprintId)"
                  :title="sprintTooltip(sprintId)"
                >
                  {{ sprintLabel(sprintId) }}
                </span>
              </div>
            </footer>
          </article>
          <div v-if="!(grouped[st] && grouped[st].length)" class="muted" style="padding: 8px;">No tasks</div>
        </div>
      </div>
      <div v-if="other.length" class="col column" data-status="__other__">
        <div class="col-header row" style="justify-content: space-between; align-items:center; gap:8px;">
          <strong>Other</strong>
          <span class="muted">{{ other.length }}</span>
        </div>
        <div class="col-cards">
          <article v-for="t in other" :key="t.id" class="card task"
                   draggable="true"
                   @dragstart="onDragStart(t)"
                   @dblclick="openTask(t.id)"
                   tabindex="0">
            <header v-if="hasTaskHeader(t)" class="row task-header">
              <template v-if="hasTaskIdentity(t)">
                <div class="row task-header__left">
                  <span v-if="isBoardFieldVisible('id') && (t.id || '').trim()" class="muted id">{{ t.id }}</span>
                  <strong v-if="isBoardFieldVisible('title') && (t.title || '').trim()" class="title">{{ t.title }}</strong>
                </div>
                <span v-if="isBoardFieldVisible('priority') && (t.priority || '').trim()" class="priority">{{ t.priority }}</span>
              </template>
              <template v-else>
                <span v-if="isBoardFieldVisible('priority') && (t.priority || '').trim()" class="priority">{{ t.priority }}</span>
              </template>
            </header>
            <footer v-if="hasTaskMeta(t)" class="task-meta" :class="{ 'task-meta--no-header': !hasTaskHeader(t) }">
              <div v-if="hasPrimaryMeta(t)" class="row task-meta__tags">
                <span v-if="isBoardFieldVisible('status') && (t.status || '').trim()" class="muted">{{ t.status }}</span>
                <span v-if="isBoardFieldVisible('task_type') && (t.task_type || '').trim()" class="muted">{{ t.task_type }}</span>
                <span v-if="isBoardFieldVisible('effort') && (t.effort || '').trim()" class="muted">{{ t.effort }}</span>
                <span v-if="isBoardFieldVisible('reporter') && (t.reporter || '').trim()" class="muted">by @{{ t.reporter }}</span>
                <span v-if="isBoardFieldVisible('assignee') && (t.assignee || '').trim()" class="muted">@{{ t.assignee }}</span>
                <span v-if="isBoardFieldVisible('due_date') && taskDueInfo(t).label" class="task-meta__due" :class="{ 'is-overdue': taskDueInfo(t).overdue }">{{ taskDueInfo(t).label }}</span>
                <span v-if="isBoardFieldVisible('modified') && taskModifiedInfo(t)" class="muted">{{ taskModifiedInfo(t) }}</span>
                <span v-if="isBoardFieldVisible('tags')" v-for="tag in (t.tags || [])" :key="tag" class="tag">{{ tag }}</span>
              </div>
              <div v-if="isBoardFieldVisible('sprints') && t.sprints?.length" class="row task-meta__sprints">
                <span
                  v-for="sprintId in t.sprints"
                  :key="`${t.id}-other-sprint-${sprintId}`"
                  class="chip small sprint-chip"
                  :class="sprintStateClass(sprintId)"
                  :title="sprintTooltip(sprintId)"
                >
                  {{ sprintLabel(sprintId) }}
                </span>
              </div>
            </footer>
          </article>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '../api/client'
import type { TaskDTO, TaskListFilter } from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import IconGlyph from '../components/IconGlyph.vue'
import ReloadButton from '../components/ReloadButton.vue'
import SmartListChips from '../components/SmartListChips.vue'
import { showToast } from '../components/toast'
import UiButton from '../components/UiButton.vue'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiLoader from '../components/UiLoader.vue'
import { useConfig } from '../composables/useConfig'
import { useProjects } from '../composables/useProjects'
import { useSprints } from '../composables/useSprints'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { useTasks } from '../composables/useTasks'
import { parseTaskDate, startOfLocalDay } from '../utils/date'
import { findLastStatusChangeAt } from '../utils/taskHistory'

const router = useRouter()
const route = useRoute()
const { projects, refresh: refreshProjects } = useProjects()
const { statuses, priorities, types, customFields: availableCustomFields, refresh: refreshConfig, loading: loadingConfig } = useConfig()
const { sprints, refresh: refreshSprints } = useSprints()
const { items, refresh, loading: loadingTasks } = useTasks()
const { openTaskPanel } = useTaskPanelController()

const project = ref<string>('')
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
const BUILTIN_QUERY_KEYS = new Set(['q', 'project', 'status', 'priority', 'type', 'assignee', 'tags', 'due', 'recent', 'needs'])
const hasFilters = computed(() => Object.entries(filter.value).some(([key, value]) => key !== 'order' && !!value))
const customFilterPresets = computed(() => {
  const names = (availableCustomFields.value || []).filter((name) => name !== '*')
  return names.slice(0, 6).map((name) => ({
    label: name,
    expression: `field:${name}=`,
  }))
})

const sprintLookup = computed<Record<number, { label: string; state?: string }>>(() => {
  const lookup: Record<number, { label: string; state?: string }> = {}
  for (const sprint of sprints.value) {
    const base = sprint.display_name || sprint.label || `Sprint ${sprint.id}`
    lookup[sprint.id] = {
      label: `#${sprint.id} ${base}`.trim(),
      state: sprint.state,
    }
  }
  return lookup
})

const MS_PER_DAY = 24 * 60 * 60 * 1000

function sprintLabel(id: number) {
  const entry = sprintLookup.value[id]
  const raw = (entry?.label || `#${id}`).trim()
  const firstSpace = raw.indexOf(' ')
  if (firstSpace === -1) return raw
  const head = raw.slice(0, firstSpace)
  const tail = raw.slice(firstSpace + 1).trim()
  if (!tail) return head
  return `${head} ${tail}`
}

function sprintStateClass(id: number) {
  const state = sprintLookup.value[id]?.state?.toLowerCase()
  if (!state) return 'sprint--unknown'
  return `sprint--${state}`
}

function sprintTooltip(id: number) {
  return sprintLabel(id)
}

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
    return { label: `Overdue ${dateLabel}`, overdue: true }
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

function resolveProjectSelection(requested: string | undefined) {
  const trimmed = (requested || '').trim()
  return trimmed
}

function sanitizeFilterInput(payload: Record<string, string>) {
  const next: Record<string, string> = {}
  const hasProjectKey = payload && Object.prototype.hasOwnProperty.call(payload, 'project')
  if (hasProjectKey) {
    const nextProject = resolveProjectSelection(payload.project)
    if (nextProject !== project.value) {
      project.value = nextProject
    }
    syncProjectRoute(nextProject)
  }
  Object.entries(payload || {}).forEach(([key, value]) => {
    if (key === 'project') return
    if (value === undefined || value === null) return
    next[key] = value
  })
  const prev = filter.value
  const sameSize = Object.keys(next).length === Object.keys(prev).length
  if (sameSize) {
    const unchanged = Object.entries(next).every(([key, value]) => prev[key] === value)
    if (unchanged) return prev
  }
  return next
}

function syncProjectRoute(nextProject: string) {
  const desired = nextProject || ''
  const current = typeof route.query.project === 'string' ? route.query.project : ''
  if (current === desired) return
  router.push({ path: '/boards', query: desired ? { project: desired } : {} })
}

function onFilterUpdate(v: Record<string, string>) {
  filter.value = sanitizeFilterInput(v)
}

function onChipsUpdate(v: Record<string, string>) {
  filter.value = sanitizeFilterInput(v)
}

function handleCustomPreset(expression: string) {
  filterBarRef.value?.appendCustomFilter(expression)
}

function clearFilters() {
  filter.value = {}
  filterBarRef.value?.clear?.()
}

function listFromCsv(value: string): string[] {
  return value.split(',').map((entry) => entry.trim()).filter(Boolean)
}

function normalizeFilter(raw: Record<string, string>) {
  const normalized: Record<string, string> = {}
  const extras: Record<string, string> = {}
  const source = raw || {}
  for (const [key, value] of Object.entries(source)) {
    if (!value || key === 'order') continue
    if (BUILTIN_QUERY_KEYS.has(key)) {
      normalized[key] = value
    } else {
      extras[key] = value
    }
  }
  normalized.order = source.order === 'asc' ? 'asc' : 'desc'
  return { normalized, extras }
}

function buildServerFilter(raw: Record<string, string>) {
  const { normalized, extras } = normalizeFilter(raw)
  const serverFilter: TaskListFilter = {}
  if (project.value) serverFilter.project = project.value
  if (normalized.q) serverFilter.q = normalized.q
  if (normalized.status) serverFilter.status = listFromCsv(normalized.status)
  if (normalized.priority) serverFilter.priority = listFromCsv(normalized.priority)
  if (normalized.type) serverFilter.type = listFromCsv(normalized.type)
  if (normalized.assignee && normalized.assignee !== '__none__') serverFilter.assignee = normalized.assignee
  if (normalized.tags) serverFilter.tags = listFromCsv(normalized.tags)
  Object.assign(serverFilter, extras)
  return { serverFilter, normalized }
}

function applySmartFilters(q: Record<string, string>) {
  const wantsUnassigned = q.assignee === '__none__'
  const due = q.due || ''
  const recent = q.recent || ''
  const needsSet = new Set((q.needs || '').split(',').map((s) => s.trim()).filter(Boolean))
  const now = new Date()
  const today = startOfDay(now)
  const tomorrow = new Date(today.getTime() + MS_PER_DAY)
  const soonCutoff = new Date(today.getTime() + 7 * MS_PER_DAY)
  const recentCutoff = new Date(now.getTime() - 7 * MS_PER_DAY)

  const list = Array.isArray(items.value) ? items.value : []
  const filtered = list.filter((task) => {
    if (wantsUnassigned && (task.assignee || '').trim()) {
      return false
    }

    if (due) {
      const dueDate = parseDateLike(task.due_date)
      if (!dueDate) {
        return false
      }
      const dueTime = startOfDay(dueDate).getTime()
      const todayStart = today.getTime()
      const tomorrowStart = tomorrow.getTime()
      const soonCutoffTime = startOfDay(soonCutoff).getTime()
      if (due === 'today' && (dueTime < todayStart || dueTime >= tomorrowStart)) {
        return false
      }
      if (due === 'soon' && (dueTime < tomorrowStart || dueTime > soonCutoffTime)) {
        return false
      }
      if (due === 'later' && dueTime <= soonCutoffTime) {
        return false
      }
      if (due === 'overdue' && dueTime >= todayStart) {
        return false
      }
    }

    if (recent === '7d') {
      const modified = parseDateLike(task.modified)
      if (!modified || modified.getTime() < recentCutoff.getTime()) {
        return false
      }
    }

    if (needsSet.size) {
      if (needsSet.has('effort')) {
        const effort = (task.effort || '').trim()
        if (effort) {
          return false
        }
      }
      if (needsSet.has('due')) {
        if ((task.due_date || '').trim()) {
          return false
        }
      }
    }

    return true
  })

  items.value = filtered
}

async function refreshBoardTasks(snapshot?: Record<string, string>) {
  if (!project.value) {
    items.value = []
    return
  }
  const raw = snapshot ?? filter.value
  const { serverFilter, normalized } = buildServerFilter(raw)
  try {
    await refresh(serverFilter)
    applySmartFilters(normalized)
    const dir = normalized.order === 'asc' ? 'asc' : 'desc'
    if (Array.isArray(items.value)) {
      items.value.sort((a, b) => (dir === 'desc' ? b.modified.localeCompare(a.modified) : a.modified.localeCompare(b.modified)))
    }
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
  for (const t of items.value || []) {
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

type BoardFieldKey = 'id' | 'title' | 'status' | 'priority' | 'task_type' | 'reporter' | 'assignee' | 'effort' | 'tags' | 'sprints' | 'due_date' | 'modified'
type BoardFieldSettings = Record<BoardFieldKey, boolean>

const DEFAULT_BOARD_FIELDS: BoardFieldSettings = {
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

const boardFields = ref<BoardFieldSettings>({ ...DEFAULT_BOARD_FIELDS })

const boardFieldOptions = computed(() => ([
  { key: 'id', label: 'ID' },
  { key: 'title', label: 'Title' },
  { key: 'status', label: 'Status' },
  { key: 'priority', label: 'Priority' },
  { key: 'task_type', label: 'Type' },
  { key: 'effort', label: 'Effort' },
  { key: 'reporter', label: 'Reporter' },
  { key: 'assignee', label: 'Assignee' },
  { key: 'tags', label: 'Tags' },
  { key: 'sprints', label: 'Sprints' },
  { key: 'due_date', label: 'Due' },
  { key: 'modified', label: 'Updated' },
] as Array<{ key: BoardFieldKey; label: string }>))

function boardFieldsKey(){ return project.value ? `lotar.boardFields::${project.value}` : 'lotar.boardFields' }

function loadBoardFields() {
  try {
    const raw = localStorage.getItem(boardFieldsKey())
    if (!raw) {
      boardFields.value = { ...DEFAULT_BOARD_FIELDS }
      return
    }
    const parsed = JSON.parse(raw)
    if (!parsed || typeof parsed !== 'object') {
      boardFields.value = { ...DEFAULT_BOARD_FIELDS }
      return
    }
    const next: BoardFieldSettings = { ...DEFAULT_BOARD_FIELDS }

    // Migrate old key (kept for compatibility): due -> due_date
    const legacyDue = (parsed as any).due
    if (typeof legacyDue === 'boolean' && typeof (parsed as any).due_date !== 'boolean') {
      next.due_date = legacyDue
    }

    for (const { key } of boardFieldOptions.value) {
      const v = (parsed as any)[key]
      if (typeof v === 'boolean') {
        next[key] = v
      }
    }
    boardFields.value = next
  } catch {
    boardFields.value = { ...DEFAULT_BOARD_FIELDS }
  }
}

function saveBoardFields() {
  try {
    localStorage.setItem(boardFieldsKey(), JSON.stringify(boardFields.value))
  } catch {}
}

function resetBoardFields() {
  boardFields.value = { ...DEFAULT_BOARD_FIELDS }
}

function closeBoardFields() {
  if (fieldsEditorRef.value) {
    fieldsEditorRef.value.open = false
  }
}

function isBoardFieldVisible(key: BoardFieldKey): boolean {
  return boardFields.value[key] !== false
}

function setBoardFieldVisible(key: BoardFieldKey, ev: Event) {
  const checked = Boolean((ev.target as HTMLInputElement | null)?.checked)
  boardFields.value = { ...boardFields.value, [key]: checked }
}

watch(boardFields, () => {
  saveBoardFields()
}, { deep: true })

function onDragStart(t: any) {
  draggingId.value = t.id
}
function onDragOver(ev: DragEvent) { ev.preventDefault() }
async function onDrop(targetStatus: string) {
  const id = draggingId.value
  if (!id || !targetStatus || targetStatus === '__other__') return
  draggingId.value = ''
  try {
    // Optimistic move
    const idx = items.value.findIndex(x => x.id === id)
    if (idx >= 0) (items.value[idx] as any).status = targetStatus
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
  const snapshot = { ...value }
  filterDebounce = setTimeout(() => {
    refreshBoardTasks(snapshot).catch(() => {})
  }, 150)
}, { deep: true })

onMounted(async () => {
  if (typeof window !== 'undefined') {
    window.addEventListener('click', handleBoardPopoverClick)
  }
  await refreshProjects()
  project.value = route.query.project ? String(route.query.project) : (projects.value[0]?.prefix || '')
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
.col-cards { padding: 8px; display: flex; flex-direction: column; gap: 8px; }
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
</style>
