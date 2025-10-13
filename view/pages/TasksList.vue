<template>
  <section class="col" style="gap: 16px;">
    <div class="row" style="justify-content: space-between; align-items: baseline; gap: 8px; flex-wrap: wrap;">
      <h1>Tasks <span class="muted" v-if="count">({{ count }})</span></h1>
    </div>
    <SmartListChips
      :statuses="statusOptions"
      :priorities="priorityOptions"
      :value="filter"
      @update:value="onChipsUpdate"
    />
    <FilterBar :statuses="statuses" :priorities="priorities" :types="types" :value="filter" @update:value="onFilterUpdate" />

    <div class="col" style="gap: 16px;">
      <UiLoader v-if="loading && !hasTasks" size="md" />
      <UiEmptyState
        v-else-if="error"
        title="We couldn't load tasks"
        :description="error"
        primary-label="Retry"
        secondary-label="Clear filters"
        @primary="retry"
        @secondary="resetFilters"
      />
      <UiEmptyState
        v-else-if="!loading && !hasTasks"
        title="No tasks match your filters"
        description="Try adjusting filters or create a new task to get started."
        primary-label="New task"
        secondary-label="Clear filters"
        @primary="openCreate"
        @secondary="resetFilters"
      />
      <TaskTable
        v-else
        :tasks="tasks"
        :loading="loading"
        :statuses="statuses"
        :selectable="bulk"
        :selected-ids="selectedIds"
        :project-key="filter.project || (tasks[0]?.id?.split('-')[0] || '')"
        :touches="activityTouches"
        v-model:bulk="bulk"
        v-model:bulk-assignee="bulkAssignee"
        @bulk-assign="bulkAssign"
        @bulk-unassign="bulkUnassign"
  @add="openCreate"
        @update:selected-ids="(v: string[]) => selectedIds = v"
        @open="view"
        @delete="removeTask"
        @update-title="onUpdateTitle"
        @update-tags="onUpdateTags"
        @set-status="onQuickStatus"
        @assign="assignOne"
        @unassign="unassignOne"
      />
    </div>

  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '../api/client'
import type { TaskDTO } from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import SmartListChips from '../components/SmartListChips.vue'
import TaskTable from '../components/TaskTable.vue'
import { showToast } from '../components/toast'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiLoader from '../components/UiLoader.vue'
import { useActivity } from '../composables/useActivity'
import { useConfig } from '../composables/useConfig'
import { useProjects } from '../composables/useProjects'
import { useSse } from '../composables/useSse'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { useTasks } from '../composables/useTasks'
import { parseTaskDate, startOfLocalDay } from '../utils/date'

const router = useRouter()
const { items, loading, error, count, refresh, remove } = useTasks()
const { openTaskPanel } = useTaskPanelController()
const hasTasks = computed(() => (items.value?.length ?? 0) > 0)
const tasks = items

const { add: addActivity, markTaskTouch, removeTaskTouch, touches: activityTouches } = useActivity()

const route = useRoute()
const { statuses, priorities, types, refresh: refreshConfig } = useConfig()
const statusOptions = computed(() => [...(statuses.value || [])])
const priorityOptions = computed(() => [...(priorities.value || [])])

const filter = ref<Record<string,string>>({})

function onFilterUpdate(v: Record<string,string>){ filter.value = v }
function onChipsUpdate(v: Record<string,string>){ filter.value = { ...v } }
function resetFilters(){ filter.value = {}; selectedIds.value = [] }

type NavMode = 'push' | 'replace' | 'none'
const MS_PER_DAY = 24 * 60 * 60 * 1000

function startOfDay(date: Date) {
  return startOfLocalDay(date)
}

function parseDateLike(value?: string | null) {
  return parseTaskDate(value)
}

function applySmartFilters(q: Record<string, string>) {
  const wantsUnassigned = q.assignee === '__none__'
  const due = q.due || ''
  const recent = q.recent || ''
  const needsSet = new Set((q.needs || '').split(',').map(s => s.trim()).filter(Boolean))
  const now = new Date()
  const today = startOfDay(now)
  const tomorrow = new Date(today.getTime() + MS_PER_DAY)
  const soonCutoff = new Date(today.getTime() + 7 * MS_PER_DAY)
  const recentCutoff = new Date(now.getTime() - 7 * MS_PER_DAY)

  const filtered = items.value.filter((task) => {
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
      if (due === 'today') {
        if (dueTime < todayStart || dueTime >= tomorrowStart) {
          return false
        }
      } else if (due === 'soon') {
        if (dueTime < tomorrowStart || dueTime > soonCutoffTime) {
          return false
        }
      } else if (due === 'later') {
        if (dueTime <= soonCutoffTime) {
          return false
        }
      } else if (due === 'overdue') {
        if (dueTime >= todayStart) {
          return false
        }
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

async function applyFilter(raw: Record<string,string>, nav: NavMode = 'push') {
  const q = { ...raw }
  const qnorm: Record<string, string> = {}
  if (q.q) qnorm.q = q.q
  if (q.project) qnorm.project = q.project
  if (q.status) qnorm.status = q.status
  if (q.priority) qnorm.priority = q.priority
  if (q.type) qnorm.type = q.type
  if (q.assignee) qnorm.assignee = q.assignee
  if (q.tags) qnorm.tags = q.tags
  if (q.due) qnorm.due = q.due
  if (q.recent) qnorm.recent = q.recent
  if (q.needs) qnorm.needs = q.needs
  qnorm.order = (q.order === 'asc' || q.order === 'desc') ? q.order : 'desc'

  if (nav === 'replace') {
    router.replace({ path: '/', query: qnorm })
  } else if (nav === 'push') {
    router.push({ path: '/', query: qnorm })
  }

  const serverFilter: any = {}
  if (qnorm.q) serverFilter.q = qnorm.q
  if (qnorm.project) serverFilter.project = qnorm.project
  if (qnorm.status) serverFilter.status = qnorm.status.split(',').map(s => s.trim()).filter(Boolean)
  if (qnorm.priority) serverFilter.priority = qnorm.priority.split(',').map(s => s.trim()).filter(Boolean)
  if (qnorm.type) serverFilter.type = qnorm.type.split(',').map(s => s.trim()).filter(Boolean)
  if (qnorm.assignee && qnorm.assignee !== '__none__') serverFilter.assignee = qnorm.assignee
  if (qnorm.tags) serverFilter.tags = qnorm.tags.split(',').map(s => s.trim()).filter(Boolean)

  await refreshConfig(serverFilter.project)
  await refresh(serverFilter)

  applySmartFilters(qnorm)

  const dir = qnorm.order === 'asc' ? 'asc' : 'desc'
  items.value.sort((a,b) => (dir === 'desc' ? b.modified.localeCompare(a.modified) : a.modified.localeCompare(b.modified)))
}

async function retry(){ await applyFilter(filter.value, 'none') }


const view = (id: string) => openTask(id)

async function removeTask(id: string){ await remove(id); showToast('Task deleted') }

async function onUpdateTitle(payload: { id: string; title: string }){
  const { id, title } = payload
  const before = tasks.value.find(t => t.id === id)?.title || ''
  try {
    const updated = await api.updateTask(id, { title })
    const idx = tasks.value.findIndex(t => t.id === id)
    if (idx >= 0) tasks.value[idx] = updated
    showToast('Title updated')
  } catch (e: any) {
    // revert optimistic change if we ever add it
    showToast(e.message || 'Failed to update title')
    const idx = tasks.value.findIndex(t => t.id === id)
    if (idx >= 0) tasks.value[idx].title = before
  }
}

async function onUpdateTags(payload: { id: string; tags: string[] }){
  const { id, tags } = payload
  try {
    const updated = await api.updateTask(id, { tags })
    const idx = tasks.value.findIndex(t => t.id === id)
    if (idx >= 0) tasks.value[idx] = updated
    showToast('Tags updated')
  } catch (e: any) {
    showToast(e.message || 'Failed to update tags')
  }
}

async function onQuickStatus(payload: { id: string; status: string }){
  const { id, status } = payload
  try {
    const updated = await api.setStatus(id, status)
    const idx = tasks.value.findIndex(t => t.id === id)
    if (idx >= 0) tasks.value[idx] = updated
    showToast('Status updated')
  } catch (e: any) {
    showToast(e.message || 'Failed to update status')
  }
}

// Bulk selection & quick assign/unassign
const bulk = ref(false)
const selectedIds = ref<string[]>([])
const bulkAssignee = ref('')
async function assignOne(id: string){
  try {
    const value = (bulkAssignee.value || '@me')
    const updated = await api.updateTask(id, { assignee: value })
    const idx = tasks.value.findIndex(t => t.id === id)
    if (idx >= 0) tasks.value[idx] = updated
    showToast('Assigned')
  } catch (e:any) { showToast(e.message || 'Failed to assign') }
}
async function unassignOne(id: string){
  try {
    const updated = await api.updateTask(id, { assignee: '' as any })
    const idx = tasks.value.findIndex(t => t.id === id)
    if (idx >= 0) tasks.value[idx] = updated
    showToast('Unassigned')
  } catch (e:any) { showToast(e.message || 'Failed to unassign') }
}
async function bulkAssign(){ const ids = [...selectedIds.value]; await Promise.all(ids.map(id => assignOne(id))); showToast(`Assigned ${ids.length} task(s)`) }
async function bulkUnassign(){ const ids = [...selectedIds.value]; await Promise.all(ids.map(id => unassignOne(id))); showToast(`Unassigned ${ids.length} task(s)`)}

const { refresh: refreshProjects } = useProjects()

let sse: { es: EventSource; close(): void; on(event: string, handler: (e: MessageEvent) => void): void; off(event: string, handler: (e: MessageEvent) => void): void } | null = null
const sseUnsubscribers: Array<() => void> = []
let refreshTimer: any = null
let disposed = false

function scheduleRefresh() {
  if (disposed) return
  if (refreshTimer) return
  refreshTimer = setTimeout(async () => {
    refreshTimer = null
    if (disposed) return
    try {
      await applyFilter(filter.value, 'none')
    } catch (err) {
      console.warn('Failed to refresh after SSE event', err)
    }
  }, 200)
}

function formatActivityMessage(kind: 'task_created' | 'task_updated' | 'task_deleted', payload: any) {
  const id = payload?.id as string | undefined
  const actor = payload?.triggered_by as string | undefined
  const title = payload?.title as string | undefined
  const label = id ? id : title ? title : 'Task'
  const actorSuffix = actor ? ` by ${actor}` : ''
  switch (kind) {
    case 'task_created':
      return `${label} created${actorSuffix}`
    case 'task_updated':
      return `${label} updated${actorSuffix}`
    case 'task_deleted':
      return `${label} deleted${actorSuffix}`
    default:
      return null
  }
}

function handleTaskEvent(kind: 'task_created' | 'task_updated' | 'task_deleted', payload: any) {
  const id = payload?.id as string | undefined
  const actor = payload?.triggered_by as string | undefined
  const title = payload?.title as string | undefined
  if (id) {
    if (kind === 'task_deleted') {
      removeTaskTouch(id)
    } else {
      const touchKind = kind === 'task_created' ? 'created' : 'updated'
      markTaskTouch({ id, kind: touchKind, actor, title, time: payload?.modified as string | undefined })
    }
  }
  const message = formatActivityMessage(kind, payload)
  if (message) {
    const activityKind = kind === 'task_created' ? 'create' : kind === 'task_deleted' ? 'delete' : 'update'
    addActivity({ kind: activityKind, message })
  }
  scheduleRefresh()
}

function registerSseHandlers() {
  if (!sse) return
  const kinds: Array<'task_created' | 'task_updated' | 'task_deleted'> = ['task_created', 'task_updated', 'task_deleted']
  kinds.forEach((kind) => {
    const handler = (ev: MessageEvent) => {
      if (!ev.data) return
      let payload: any
      try {
        payload = JSON.parse(ev.data)
      } catch (err) {
        console.warn('Failed to parse SSE payload', err)
        return
      }
      handleTaskEvent(kind, payload)
    }
    sse!.on(kind, handler)
    sseUnsubscribers.push(() => {
      if (sse) sse.off(kind, handler)
    })
  })
}
onMounted(async () => {
  filter.value = Object.fromEntries(Object.entries(route.query).map(([k,v]) => [k, String(v)]))
  await refreshProjects()
  await applyFilter(filter.value, 'replace')
  sse = useSse('/api/events', { kinds: 'task_created,task_updated,task_deleted' })
  registerSseHandlers()
})

onUnmounted(() => {
  disposed = true
  if (refreshTimer) {
    clearTimeout(refreshTimer)
    refreshTimer = null
  }
  sseUnsubscribers.splice(0).forEach((fn) => fn())
  if (sse) sse.close()
})

// Debounced fetch on any filter change; also sync URL and config
let debounceTimer: any = null

watch(filter, (q) => {
  if (debounceTimer) clearTimeout(debounceTimer)
  const snapshot = { ...q }
  debounceTimer = setTimeout(() => {
    applyFilter(snapshot).catch((err) => {
      console.warn('Failed to apply filter', err)
    })
  }, 150)
}, { deep: true })

const openCreate = () => {
  openTaskPanel({
    taskId: 'new',
    initialProject: filter.value.project || null,
    onCreated: handleTaskCreated,
  })
}

const openTask = (id: string) => {
  openTaskPanel({
    taskId: id,
    onUpdated: handleTaskUpdated,
  })
}

const handleTaskCreated = (task: TaskDTO) => {
  applyFilter(filter.value, 'none').catch(() => {})
  router.push(`/task/${encodeURIComponent(task.id)}`)
}

const handleTaskUpdated = (task: TaskDTO) => {
  const idx = tasks.value.findIndex(t => t.id === task.id)
  if (idx >= 0) {
    tasks.value[idx] = task
  }
}
</script>

<style scoped>
</style>
