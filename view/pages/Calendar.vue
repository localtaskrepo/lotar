<template>
  <section class="col" style="gap:16px;">
    <div class="row" style="justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px;">
      <h1>Calendar <span v-if="project" class="muted">— {{ project }}</span></h1>
      <div class="row calendar-controls">
        <div class="row" style="gap:6px; align-items:center;">
          <UiButton icon-only aria-label="Previous month" @click="prevMonth">
            <IconGlyph name="chevron-left" />
          </UiButton>
          <strong>{{ monthLabel }}</strong>
          <UiButton icon-only aria-label="Next month" @click="nextMonth">
            <IconGlyph name="chevron-right" />
          </UiButton>
          <UiButton @click="goToday">Today</UiButton>
        </div>
        <UiButton class="toggle-sprints" variant="ghost" :class="{ active: showSprints }" :aria-pressed="showSprints ? 'true' : 'false'" @click="toggleSprints">
          {{ showSprints ? 'Hide sprint schedule' : 'Show sprint schedule' }}
        </UiButton>
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
          :disabled="loadingTasks"
          :loading="loadingTasks"
          label="Refresh calendar data"
          title="Refresh calendar data"
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
        :enable-due-soon="false"
        :enable-recent="false"
        @update:value="onChipsUpdate"
        @preset="handleCustomPreset"
      />
      <FilterBar
        ref="filterBarRef"
        :statuses="statuses"
        :priorities="priorities"
        :types="types"
        :value="filterPayload"
        storage-key="lotar.calendar.filter"
        emit-project-key
        :show-order="false"
        @update:value="onFilterUpdate"
      />
    </div>

    <div v-if="loadingTasks" style="margin: 12px 0;"><UiLoader>Loading calendar…</UiLoader></div>

    <div v-else class="calendar">
      <div class="grid header">
        <div v-for="d in weekDays" :key="d" class="cell head">{{ d }}</div>
      </div>
      <div class="grid body">
        <div
          v-for="(cell, idx) in cells"
          :key="cell.dateKey || idx"
          class="cell day"
          :class="{ other: !cell.inMonth, today: isTodayCell(cell.date) }"
          :data-date="cell.dateKey"
        >
          <div class="date">{{ cell.date.getDate() }}</div>
          <div v-if="showSprints && cell.sprints.length" class="sprints">
            <div
              v-for="sprint in cell.sprints"
              :key="`${sprint.id}-${cell.dateKey}`"
              class="sprint-pill"
              :class="{
                'partial-start': !sprint.isStart,
                'partial-end': !sprint.isEnd,
                'dim-before': sprint.beforeActualStart,
                'dim-after': sprint.afterActualEnd,
                'actual-end': sprint.isActualEnd,
              }"
              :style="{ '--sprint-color': sprintColorForState(sprint.state) }"
              :data-sprint-id="sprint.id"
              :data-sprint-state="sprint.state"
              :data-actual-phase="sprint.afterActualEnd ? 'after-end' : sprint.beforeActualStart ? 'before-start' : 'active'"
              :title="formatSprintTooltip(sprint)"
            >
              <span class="sprint-pill__label">{{ sprint.label }}</span>
            </div>
          </div>
          <ul class="tasks">
            <li
              v-for="(t, i) in cell.tasks.slice(0, 5)"
              :key="t.id"
              class="task-item"
              role="button"
              tabindex="0"
              @click="openTask(t.id)"
              @keydown.enter.prevent="openTask(t.id)"
              @keydown.space.prevent="openTask(t.id)"
            >
              <TaskHoverCard :task="t" :placement="(idx % 7) >= 4 ? 'right' : 'left'">
                <div class="task-inline">
                  <span class="id">{{ t.id }}</span>
                  <span class="title" :title="t.title">{{ shortTitle(t.title) }}</span>
                </div>
              </TaskHoverCard>
            </li>
            <li v-if="cell.tasks.length > 5" class="muted more" @click="openDay(cell.date)">+{{ cell.tasks.length - 5 }} more</li>
          </ul>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import type { SprintListItem, TaskListFilter } from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import IconGlyph from '../components/IconGlyph.vue'
import ReloadButton from '../components/ReloadButton.vue'
import SmartListChips from '../components/SmartListChips.vue'
import TaskHoverCard from '../components/TaskHoverCard.vue'
import UiButton from '../components/UiButton.vue'
import UiLoader from '../components/UiLoader.vue'
import { useConfig } from '../composables/useConfig'
import { useProjects } from '../composables/useProjects'
import { useSprints } from '../composables/useSprints'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { useTasks } from '../composables/useTasks'
import { parseTaskDate, startOfLocalDay, toDateKey } from '../utils/date'
import { buildSprintSchedule, type SprintCalendarDayEntry } from '../utils/sprintCalendar'

const route = useRoute()
const router = useRouter()
const { refresh: refreshProjects } = useProjects()
const { items, refresh: refreshTasks, loading: loadingTasks } = useTasks()
const { sprints: sprintList, refresh: refreshSprints } = useSprints()
const { openTaskPanel } = useTaskPanelController()
const { statuses, priorities, types, customFields: availableCustomFields, refresh: refreshConfig } = useConfig()

const project = ref<string>('')
const cursor = ref<Date>(new Date()) // month cursor
const showSprints = ref(false)
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

type SprintState = SprintListItem['state']
const sprintStateColors: Record<SprintState | 'default', string> = {
  pending: 'var(--color-muted, #64748b)',
  active: 'var(--color-accent, #0ea5e9)',
  overdue: 'var(--color-danger, #ef4444)',
  complete: 'var(--color-success, #16a34a)',
  default: 'var(--color-muted, #64748b)',
}

function sprintColorForState(state?: string | null): string {
  const normalized = (state || '').toLowerCase() as SprintState
  return sprintStateColors[normalized] || sprintStateColors.default
}

const weekDays = ['Sun','Mon','Tue','Wed','Thu','Fri','Sat']
const MS_PER_DAY = 24 * 60 * 60 * 1000

function resolveProjectSelection(requested: string | undefined) {
  return (requested || '').trim()
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
  pushRoute()
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

const monthLabel = computed(() => cursor.value.toLocaleDateString(undefined, { month: 'long', year: 'numeric' }))

function startOfMonth(d: Date){ return new Date(d.getFullYear(), d.getMonth(), 1) }
function endOfMonth(d: Date){ return new Date(d.getFullYear(), d.getMonth()+1, 0) }
function startOfGrid(d: Date){ const s = startOfMonth(d); const w = s.getDay(); return new Date(s.getFullYear(), s.getMonth(), 1 - w) }
function endOfGrid(d: Date){ const e = endOfMonth(d); const w = e.getDay(); return new Date(e.getFullYear(), e.getMonth(), e.getDate() + (6 - w)) }
function startOfDay(date: Date) { return startOfLocalDay(date) }
function parseDateLike(value?: string | null) { return parseTaskDate(value) }
const sprintSchedule = computed(() => {
  if (!showSprints.value) return {} as Record<string, SprintCalendarDayEntry[]>
  return buildSprintSchedule(sprintList.value, startOfGrid(cursor.value), endOfGrid(cursor.value))
})
const cells = computed(() => {
  const start = startOfGrid(cursor.value)
  const end = endOfGrid(cursor.value)
  const days: Array<{ date: Date; dateKey: string; inMonth: boolean; tasks: any[]; sprints: SprintCalendarDayEntry[] }> = []
  const month = cursor.value.getMonth()

  // Index tasks by due date for this window
  const byDate: Record<string, any[]> = {}
  for (const t of items.value || []) {
    const due = (t as any).due_date
    if (!due) continue
    const parsed = parseTaskDate(due)
    if (!parsed) continue
    const dueStart = startOfLocalDay(parsed)
    if (dueStart >= start && dueStart <= end) {
      const key = toDateKey(dueStart)
      ;(byDate[key] ||= []).push(t)
    }
  }

  const schedule = sprintSchedule.value

  for (let d = new Date(start); d <= end; d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1)) {
    const key = toDateKey(d)
    days.push({
      date: new Date(d),
      dateKey: key,
      inMonth: d.getMonth() === month,
      tasks: (byDate[key] || []),
      sprints: schedule[key] || [],
    })
  }
  return days
})

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
      if (needsSet.has('effort') && (task.effort || '').trim()) {
        return false
      }
      if (needsSet.has('due') && (task.due_date || '').trim()) {
        return false
      }
    }

    return true
  })

  items.value = filtered
}

async function refreshCalendarTasks(snapshot?: Record<string, string>) {
  const raw = snapshot ?? filter.value
  const { serverFilter, normalized } = buildServerFilter(raw)
  await refreshTasks(serverFilter)
  applySmartFilters(normalized)
  const dir = normalized.order === 'asc' ? 'asc' : 'desc'
  if (Array.isArray(items.value)) {
    items.value.sort((a, b) => (dir === 'desc' ? b.modified.localeCompare(a.modified) : a.modified.localeCompare(b.modified)))
  }
}

function prevMonth(){ cursor.value = new Date(cursor.value.getFullYear(), cursor.value.getMonth()-1, 1); pushRoute() }
function nextMonth(){ cursor.value = new Date(cursor.value.getFullYear(), cursor.value.getMonth()+1, 1); pushRoute() }
function goToday(){ cursor.value = new Date(); pushRoute() }

function isTodayCell(d: Date){ const now = new Date(); return d.getFullYear()===now.getFullYear() && d.getMonth()===now.getMonth() && d.getDate()===now.getDate() }

function pushRoute(){
  const y = cursor.value.getFullYear(); const m = String(cursor.value.getMonth()+1).padStart(2,'0')
  const q: any = { month: `${y}-${m}` }
  if (project.value) q.project = project.value
  if (showSprints.value) q.sprints = '1'
  router.push({ path: '/calendar', query: q })
}

function openTask(id: string){
  openTaskPanel({ taskId: id })
}
function openDay(d: Date){ /* future: open a day view or filter tasks list */ }

function toggleSprints(){
  showSprints.value = !showSprints.value
  pushRoute()
}

function shortTitle(title?: string | null){
  const value = (title || '').trim()
  if (!value) return '(no title)'
  if (value.length <= 48) return value
  return `${value.slice(0, 45).trimEnd()}…`
}

async function refreshAll(){
  await Promise.all([
    refreshProjects(),
    refreshSprints(true),
    refreshConfig(project.value),
  ])
  await refreshCalendarTasks()
}

function formatSprintTooltip(entry: SprintCalendarDayEntry){
  const startLabel = entry.startDate.toLocaleDateString()
  const endLabel = entry.endDate.toLocaleDateString()
  const parts = [`Planned: ${startLabel} – ${endLabel}`]
  const actualStart = entry.actualStartDate?.toLocaleDateString()
  const actualEnd = entry.actualEndDate?.toLocaleDateString()
  if (actualStart || actualEnd) {
    const actualBits: string[] = []
    if (actualStart) actualBits.push(`start ${actualStart}`)
    if (actualEnd) actualBits.push(`end ${actualEnd}`)
    parts.push(`Actual: ${actualBits.join(', ')}`)
  }
  return `${entry.label} · ${parts.join(' | ')}`
}

let filterDebounce: ReturnType<typeof setTimeout> | null = null

watch(filter, (value) => {
  if (filterDebounce) clearTimeout(filterDebounce)
  const snapshot = { ...value }
  filterDebounce = setTimeout(() => {
    refreshCalendarTasks(snapshot).catch(() => {})
  }, 150)
}, { deep: true })

onUnmounted(() => {
  if (filterDebounce) {
    clearTimeout(filterDebounce)
    filterDebounce = null
  }
})

onMounted(async () => {
  await Promise.all([refreshProjects(), refreshSprints(true)])
  const q = route.query as Record<string, any>
  project.value = q.project ? String(q.project) : ''
  // Parse month from query (YYYY-MM)
  if (q.month && /^\d{4}-\d{2}$/.test(String(q.month))) {
    const [y, m] = String(q.month).split('-').map((s: string) => parseInt(s, 10))
    cursor.value = new Date(y, m - 1, 1)
  }
  showSprints.value = q.sprints === '1'
  await refreshConfig(project.value)
  await refreshCalendarTasks()
})

watch(() => route.query, async (q) => {
  const r = q as any
  const nextProject = r.project ? String(r.project) : ''
  if (nextProject !== project.value) {
    project.value = nextProject
    await refreshConfig(project.value)
    await refreshCalendarTasks()
  }
  if (r.month && /^\d{4}-\d{2}$/.test(String(r.month))) {
    const [y, m] = String(r.month).split('-').map((s: string) => parseInt(s, 10))
    const next = new Date(y, m - 1, 1)
    if (next.getTime() !== cursor.value.getTime()) cursor.value = next
  }
  const show = r.sprints === '1'
  if (show !== showSprints.value) {
    showSprints.value = show
  }
})

watch(showSprints, (enabled, previous) => {
  if (enabled && !previous) {
    void refreshSprints(true)
  }
})
</script>

<style scoped>
.calendar { display: grid; gap: 8px; }
.grid { display: grid; grid-template-columns: repeat(7, 1fr); gap: 8px; }
.cell { border: 1px solid var(--border); border-radius: 8px; padding: 8px; background: var(--bg); min-height: 100px; }
.head { text-align: center; font-weight: 600; color: var(--muted); border: none; background: transparent; min-height: 20px; }
.day.other { opacity: 0.6; }
.day.today { outline: 2px solid color-mix(in oklab, var(--fg) 30%, transparent); outline-offset: 2px; }
.date { font-weight: 600; margin-bottom: 6px; }
.tasks { display: flex; flex-direction: column; gap: 4px; list-style: none; padding: 0; margin: 0; }
.tasks .task-item {
  display: flex;
  align-items: center;
  gap: 6px;
  position: relative;
  cursor: pointer;
  padding: 3px 4px;
  border-radius: 6px;
  transition: background 120ms ease, box-shadow 120ms ease;
  outline: none;
}
.tasks .task-item .task-inline {
  display: grid;
  grid-template-columns: auto 1fr;
  align-items: center;
  gap: 6px;
  width: 100%;
  min-height: 20px;
}
.tasks .task-item:hover {
  background: color-mix(in oklab, var(--surface, #f8fafc) 82%, transparent);
}
.tasks .task-item:focus-visible {
  box-shadow: var(--focus-ring, 0 0 0 3px rgba(14, 165, 233, 0.3));
}
.tasks .task-item .id {
  font-size: 11px;
  font-weight: 600;
  color: var(--color-muted, #64748b);
  flex-shrink: 0;
}
.tasks .task-item .title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  line-height: 1.1;
}
.tasks li.more { cursor: pointer; }
.toggle-sprints.active {
  background: color-mix(in oklab, var(--accent, #0ea5e9) 20%, transparent);
  border-color: color-mix(in oklab, var(--accent, #0ea5e9) 35%, transparent);
}
.sprints {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 6px;
}
.sprint-pill {
  --_color: var(--sprint-color, #475569);
  background: color-mix(in oklab, var(--_color) 20%, transparent);
  border: 1px solid color-mix(in oklab, var(--_color) 40%, transparent);
  color: color-mix(in oklab, var(--_color) 70%, black);
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 999px;
  display: flex;
  align-items: center;
  min-height: 18px;
}
.sprint-pill.dim-before,
.sprint-pill.dim-after {
  opacity: 0.55;
}
.sprint-pill.actual-end {
  position: relative;
}
.sprint-pill.actual-end::after {
  content: '';
  width: 6px;
  height: 6px;
  border-radius: 50%;
  margin-left: 6px;
  background: color-mix(in oklab, var(--_color) 85%, var(--bg, #fff));
  box-shadow: 0 0 0 1px color-mix(in oklab, var(--_color) 70%, transparent);
}
.sprint-pill.partial-start { border-top-left-radius: 4px; border-bottom-left-radius: 4px; }
.sprint-pill.partial-end { border-top-right-radius: 4px; border-bottom-right-radius: 4px; }
.sprint-pill__label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.calendar-controls {
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}
.calendar-controls :is(.ui-select, .btn) {
  min-height: 2.25rem;
  height: 2.25rem;
  padding-top: 0;
  padding-bottom: 0;
}
.calendar-controls .btn.icon-only {
  width: 2.25rem;
  height: 2.25rem;
}
.toggle-sprints {
  border: 1px solid var(--border);
}
.filter-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
</style>
