<template>
  <section class="col" style="gap:16px;">
    <div class="row" style="justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px;">
      <h1>Boards <span v-if="project" class="muted">— {{ project }}</span></h1>
      <div class="row board-controls">
        <details class="wip-editor">
          <summary class="btn">WIP limits</summary>
          <div class="card col" style="gap:8px; min-width: 260px;">
            <div class="row" v-for="st in columns" :key="st" style="gap:6px; align-items:center;">
              <label style="min-width: 120px;">{{ st }}</label>
              <input class="input" type="number" min="0" :value="wipLimits[st] ?? ''" @input="onWipInput(st, $event)" placeholder="—" style="max-width: 100px;" />
            </div>
            <small class="muted">Leave empty or 0 for no limit. Limits are saved locally per project.</small>
          </div>
        </details>
        <details class="done-filter">
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
      <FilterBar
        ref="filterBarRef"
        :statuses="statuses"
        :priorities="priorities"
        :types="types"
        :value="filterPayload"
        :show-status="false"
        emit-project-key
        storage-key="lotar.boards.filter"
        @update:value="onFilterUpdate"
      />
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
            <TaskHoverCard :task="t" block>
              <header class="row" style="justify-content: space-between; gap: 6px; align-items: baseline;">
                <div>
                  <span class="muted id">{{ t.id }}</span>
                  <strong class="title">{{ t.title }}</strong>
                </div>
                <span class="priority">{{ t.priority }}</span>
              </header>
              <footer class="task-meta">
                <div class="row task-meta__tags">
                  <span v-if="t.assignee" class="muted">@{{ t.assignee }}</span>
                  <span v-for="tag in t.tags" :key="tag" class="chip small">{{ tag }}</span>
                </div>
                <div v-if="t.sprints?.length" class="row task-meta__sprints">
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
            </TaskHoverCard>
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
            <TaskHoverCard :task="t" block>
              <header class="row" style="justify-content: space-between; gap: 6px; align-items: baseline;">
                <div>
                  <span class="muted id">{{ t.id }}</span>
                  <strong class="title">{{ t.title }}</strong>
                </div>
                <span class="priority">{{ t.priority }}</span>
              </header>
              <footer class="task-meta">
                <div class="row task-meta__tags">
                  <span v-if="t.assignee" class="muted">@{{ t.assignee }}</span>
                  <span v-for="tag in t.tags" :key="tag" class="chip small">{{ tag }}</span>
                </div>
                <div v-if="t.sprints?.length" class="row task-meta__sprints">
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
            </TaskHoverCard>
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
import TaskHoverCard from '../components/TaskHoverCard.vue'
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
  await refreshProjects()
  project.value = route.query.project ? String(route.query.project) : (projects.value[0]?.prefix || '')
  await refreshSprints(true)
  await refreshConfig(project.value)
  await refreshBoardTasks()
  loadWip()
  loadDoneFilters()
})

watch(() => route.query, async (q) => {
  project.value = (q as any).project ? String((q as any).project) : ''
  await refreshConfig(project.value)
  await refreshBoardTasks()
  loadWip()
  loadDoneFilters()
})

onUnmounted(() => {
  if (filterDebounce) {
    clearTimeout(filterDebounce)
    filterDebounce = null
  }
})
</script>

<style scoped>
.board { align-items: flex-start; }
.column { border: 1px solid var(--border); border-radius: 8px; background: var(--bg); min-height: 200px; display: flex; flex-direction: column; }
.column.over-limit { border-color: color-mix(in oklab, #ef4444 40%, var(--border)); }
.col-header { position: sticky; top: 0; background: var(--bg); padding: 8px; border-bottom: 1px solid var(--border); border-top-left-radius: 8px; border-top-right-radius: 8px; z-index: 1; }
.col-header .warn { color: #b91c1c; font-weight: 600; }
.col-cards { padding: 8px; display: flex; flex-direction: column; gap: 8px; }
.task { padding: 8px; border: 1px solid var(--border); border-radius: 8px; cursor: grab; user-select: none; }
.task:active { cursor: grabbing; }
.task .id { font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace; margin-right: 6px; }
.task .title { display: inline-block; max-width: 42ch; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.priority { font-size: 12px; color: var(--muted); }
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
  z-index: 20;
  box-shadow: var(--shadow-md, 0 10px 30px rgba(15, 23, 42, 0.15));
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
  z-index: 20;
  box-shadow: var(--shadow-md, 0 10px 30px rgba(15, 23, 42, 0.15));
}
.done-filter__statuses {
  gap: 4px;
  max-height: 220px;
  overflow-y: auto;
  border: 1px solid var(--border, rgba(15,12,32,0.15));
  border-radius: 6px;
  padding: 6px;
}

.filter-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.task-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 6px;
}

.task-meta__tags,
.task-meta__sprints {
  gap: 6px;
  flex-wrap: wrap;
  align-items: center;
}

.chip.sprint-chip {
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
</style>
