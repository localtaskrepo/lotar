<template>
  <section class="col" style="gap:16px;">
    <div class="row" style="justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px;">
      <h1>Boards <span v-if="project" class="muted">— {{ project }}</span></h1>
      <div class="row" style="gap:8px; align-items:center; flex-wrap: wrap;">
        <UiSelect v-model="project" @change="onProjectChange" style="min-width:240px;">
          <option value="">Select project…</option>
          <option v-for="p in projects" :key="p.prefix" :value="p.prefix">{{ p.prefix }} — {{ p.name }}</option>
        </UiSelect>
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
        <UiButton @click="refreshAll">Refresh</UiButton>
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
            <TaskHoverCard :task="t" block>
              <header class="row" style="justify-content: space-between; gap: 6px; align-items: baseline;">
                <div>
                  <span class="muted id">{{ t.id }}</span>
                  <strong class="title">{{ t.title }}</strong>
                </div>
                <span class="priority">{{ t.priority }}</span>
              </header>
              <footer class="row" style="gap:6px; flex-wrap: wrap;">
                <span v-if="t.assignee" class="muted">@{{ t.assignee }}</span>
                <span v-for="tag in t.tags" :key="tag" class="chip small">{{ tag }}</span>
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
              <footer class="row" style="gap:6px; flex-wrap: wrap;">
                <span v-if="t.assignee" class="muted">@{{ t.assignee }}</span>
                <span v-for="tag in t.tags" :key="tag" class="chip small">{{ tag }}</span>
              </footer>
            </TaskHoverCard>
          </article>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '../api/client'
import type { TaskDTO } from '../api/types'
import TaskHoverCard from '../components/TaskHoverCard.vue'
import { showToast } from '../components/toast'
import UiButton from '../components/UiButton.vue'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiLoader from '../components/UiLoader.vue'
import UiSelect from '../components/UiSelect.vue'
import { useConfig } from '../composables/useConfig'
import { useProjects } from '../composables/useProjects'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { useTasks } from '../composables/useTasks'

const router = useRouter()
const route = useRoute()
const { projects, refresh: refreshProjects } = useProjects()
const { statuses, refresh: refreshConfig, loading: loadingConfig } = useConfig()
const { items, refresh, loading: loadingTasks } = useTasks()
const { openTaskPanel } = useTaskPanelController()

const project = ref<string>('')
const draggingId = ref<string>('')

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

const grouped = computed<Record<string, TaskDTO[]>>(() => {
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
    const prev = idx >= 0 ? { ...items.value[idx] } : null
    if (idx >= 0) (items.value[idx] as any).status = targetStatus
  await api.setStatus(id, targetStatus)
    showToast(`Moved ${id} → ${targetStatus}`)
  } catch (e: any) {
    showToast(e.message || 'Failed to move task')
    // revert by refetching
    await refresh({ project: project.value } as any)
  }
}

function openTask(id: string) {
  openTaskPanel({ taskId: id })
}

function onProjectChange() {
  router.push({ path: '/boards', query: project.value ? { project: project.value } : {} })
}

async function refreshAll() {
  await refreshProjects()
  await refreshConfig(project.value)
  await refresh({ project: project.value } as any)
}

onMounted(async () => {
  await refreshProjects()
  project.value = route.query.project ? String(route.query.project) : (projects.value[0]?.prefix || '')
  await refreshConfig(project.value)
  await refresh({ project: project.value } as any)
  loadWip()
})

watch(() => route.query, async (q) => {
  project.value = (q as any).project ? String((q as any).project) : ''
  await refreshConfig(project.value)
  await refresh({ project: project.value } as any)
  loadWip()
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
</style>
