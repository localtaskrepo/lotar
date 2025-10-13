<template>
  <section class="col" style="gap:16px;">
    <div class="row" style="justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px;">
      <h1>Calendar <span v-if="project" class="muted">— {{ project }}</span></h1>
      <div class="row" style="gap:8px; align-items:center; flex-wrap: wrap;">
        <UiSelect v-model="project" @change="onProjectChange" style="min-width:240px;">
          <option value="">All projects</option>
          <option v-for="p in projects" :key="p.prefix" :value="p.prefix">{{ p.prefix }} — {{ p.name }}</option>
        </UiSelect>
        <div class="row" style="gap:6px; align-items:center;">
          <UiButton @click="prevMonth">◀</UiButton>
          <strong>{{ monthLabel }}</strong>
          <UiButton @click="nextMonth">▶</UiButton>
          <UiButton @click="goToday">Today</UiButton>
        </div>
        <UiButton @click="refreshAll">Refresh</UiButton>
      </div>
    </div>

    <div v-if="loadingTasks" style="margin: 12px 0;"><UiLoader>Loading calendar…</UiLoader></div>

    <div v-else class="calendar">
      <div class="grid header">
        <div v-for="d in weekDays" :key="d" class="cell head">{{ d }}</div>
      </div>
      <div class="grid body">
        <div v-for="(cell, idx) in cells" :key="idx" class="cell day" :class="{ other: !cell.inMonth, today: isTodayCell(cell.date) }">
          <div class="date">{{ cell.date.getDate() }}</div>
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
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import TaskHoverCard from '../components/TaskHoverCard.vue'
import UiButton from '../components/UiButton.vue'
import UiLoader from '../components/UiLoader.vue'
import UiSelect from '../components/UiSelect.vue'
import { useProjects } from '../composables/useProjects'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { useTasks } from '../composables/useTasks'
import { parseTaskDate, startOfLocalDay, toDateKey } from '../utils/date'

const route = useRoute()
const router = useRouter()
const { projects, refresh: refreshProjects } = useProjects()
const { items, refresh, loading: loadingTasks } = useTasks()
const { openTaskPanel } = useTaskPanelController()

const project = ref<string>('')
const cursor = ref<Date>(new Date()) // month cursor

const weekDays = ['Sun','Mon','Tue','Wed','Thu','Fri','Sat']

const monthLabel = computed(() => cursor.value.toLocaleDateString(undefined, { month: 'long', year: 'numeric' }))

function startOfMonth(d: Date){ return new Date(d.getFullYear(), d.getMonth(), 1) }
function endOfMonth(d: Date){ return new Date(d.getFullYear(), d.getMonth()+1, 0) }
function startOfGrid(d: Date){ const s = startOfMonth(d); const w = s.getDay(); return new Date(s.getFullYear(), s.getMonth(), 1 - w) }
function endOfGrid(d: Date){ const e = endOfMonth(d); const w = e.getDay(); return new Date(e.getFullYear(), e.getMonth(), e.getDate() + (6 - w)) }
const cells = computed(() => {
  const start = startOfGrid(cursor.value)
  const end = endOfGrid(cursor.value)
  const days: Array<{ date: Date; inMonth: boolean; tasks: any[] }> = []
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

  for (let d = new Date(start); d <= end; d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1)) {
    const key = toDateKey(d)
    days.push({ date: new Date(d), inMonth: d.getMonth() === month, tasks: (byDate[key] || []) })
  }
  return days
})

function prevMonth(){ cursor.value = new Date(cursor.value.getFullYear(), cursor.value.getMonth()-1, 1); pushRoute() }
function nextMonth(){ cursor.value = new Date(cursor.value.getFullYear(), cursor.value.getMonth()+1, 1); pushRoute() }
function goToday(){ cursor.value = new Date(); pushRoute() }

function isTodayCell(d: Date){ const now = new Date(); return d.getFullYear()===now.getFullYear() && d.getMonth()===now.getMonth() && d.getDate()===now.getDate() }

function pushRoute(){
  const y = cursor.value.getFullYear(); const m = String(cursor.value.getMonth()+1).padStart(2,'0')
  const q: any = { month: `${y}-${m}` }
  if (project.value) q.project = project.value
  router.push({ path: '/calendar', query: q })
}

function openTask(id: string){
  openTaskPanel({ taskId: id })
}
function openDay(d: Date){ /* future: open a day view or filter tasks list */ }

function shortTitle(title?: string | null){
  const value = (title || '').trim()
  if (!value) return '(no title)'
  if (value.length <= 48) return value
  return `${value.slice(0, 45).trimEnd()}…`
}

function onProjectChange(){ pushRoute(); refresh({ project: project.value || undefined } as any) }

async function refreshAll(){ await refreshProjects(); await refresh({ project: project.value || undefined } as any) }

onMounted(async () => {
  await refreshProjects()
  const q = route.query as Record<string, any>
  project.value = q.project ? String(q.project) : ''
  // Parse month from query (YYYY-MM)
  if (q.month && /^\d{4}-\d{2}$/.test(String(q.month))) {
    const [y, m] = String(q.month).split('-').map((s: string) => parseInt(s, 10))
    cursor.value = new Date(y, m - 1, 1)
  }
  await refresh({ project: project.value || undefined } as any)
})

watch(() => route.query, async (q) => {
  const r = q as any
  const p = r.project ? String(r.project) : ''
  if (p !== project.value) {
    project.value = p
    await refresh({ project: project.value || undefined } as any)
  }
  if (r.month && /^\d{4}-\d{2}$/.test(String(r.month))) {
    const [y, m] = String(r.month).split('-').map((s: string) => parseInt(s, 10))
    const next = new Date(y, m - 1, 1)
    if (next.getTime() !== cursor.value.getTime()) cursor.value = next
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
</style>
