<template>
  <section class="col" style="gap: 16px;">
    <div class="row" style="justify-content: space-between; align-items: center; gap: 8px; flex-wrap: wrap;">
      <h1>Tasks <span class="muted" v-if="count">({{ count }})</span></h1>
      <div class="row split-actions" style="gap: 8px; align-items: center;">
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
          :disabled="loading"
          :loading="loading"
          label="Refresh tasks"
          title="Refresh tasks"
          @click="retry"
        />
      </div>
    </div>
    <div class="filter-card">
      <SmartListChips
        :statuses="statusOptions"
        :priorities="priorityOptions"
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
        :value="filter"
        @update:value="onFilterUpdate"
      />
    </div>

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
        :sprint-lookup="sprintLookup"
        :has-sprints="hasSprints"
        :sprints-loading="sprintsLoading"
        v-model:bulk="bulk"
        @bulk-assign="openBulkAssign"
        @bulk-unassign="bulkUnassign"
        @bulk-sprint-add="openBulkSprintAdd"
        @bulk-sprint-remove="openBulkSprintRemove"
        @bulk-delete="openBulkDelete"
        @add="openCreate"
  @update:selected-ids="setSelectedIds"
        @open="view"
        @delete="openSingleDelete"
        @update-title="onUpdateTitle"
        @update-tags="onUpdateTags"
        @set-status="onQuickStatus"
        @assign="openSingleAssign"
        @unassign="unassignOne"
        @sprint-add="openSingleSprintAdd"
        @sprint-remove="openSingleSprintRemove"
      />
    </div>

    <Teleport to="body">
      <div
        v-if="assignDialogOpen"
        class="tasks-modal__overlay"
        role="dialog"
        aria-modal="true"
        :aria-label="assignDialogTitle"
        @click.self="closeAssignDialog"
      >
        <UiCard class="tasks-modal__card">
          <form class="col tasks-modal__form" @submit.prevent="submitAssignDialog">
            <header class="row tasks-modal__header">
              <div class="col" style="gap: 4px;">
                <h2>{{ assignDialogTitle }}</h2>
                <p class="muted tasks-modal__hint">Use <code>@me</code> to assign the tasks to yourself.</p>
              </div>
              <UiButton
                variant="ghost"
                icon-only
                type="button"
                aria-label="Close dialog"
                title="Close dialog"
                :disabled="assignDialogSubmitting"
                @click="closeAssignDialog"
              >
                <IconGlyph name="close" />
              </UiButton>
            </header>
            <label class="col" style="gap: 4px;">
              <span class="muted">Assignee</span>
              <input
                ref="assignInputRef"
                class="input"
                v-model="assignInputValue"
                placeholder="username or @me"
                autocomplete="off"
              />
            </label>
            <div class="row" style="gap: 8px; flex-wrap: wrap;">
              <UiButton variant="ghost" type="button" @click="useAssignShortcut('@me')">Use @me</UiButton>
            </div>
            <footer class="row tasks-modal__footer">
              <UiButton variant="primary" type="submit" :disabled="assignDialogSubmitting || !assignInputValue.trim()">
                {{ assignDialogSubmitting ? 'Assigning…' : assignDialogTitle }}
              </UiButton>
              <UiButton variant="ghost" type="button" :disabled="assignDialogSubmitting" @click="closeAssignDialog">Cancel</UiButton>
            </footer>
          </form>
        </UiCard>
      </div>
    </Teleport>

    <Teleport to="body">
      <div
        v-if="sprintDialogOpen"
        class="tasks-modal__overlay"
        role="dialog"
        aria-modal="true"
        :aria-label="sprintDialogTitle"
        @click.self="closeSprintDialog"
      >
        <UiCard class="tasks-modal__card">
          <form class="col tasks-modal__form" @submit.prevent="submitSprintDialog">
            <header class="row tasks-modal__header">
              <div class="col" style="gap: 4px;">
                <h2>{{ sprintDialogTitle }}</h2>
                <p class="muted tasks-modal__hint">
                  Choose the sprint target for the selected task{{ sprintDialogIds.length === 1 ? '' : 's' }}.
                </p>
              </div>
              <UiButton
                variant="ghost"
                icon-only
                type="button"
                aria-label="Close dialog"
                title="Close dialog"
                :disabled="sprintDialogSubmitting"
                @click="closeSprintDialog"
              >
                <IconGlyph name="close" />
              </UiButton>
            </header>
            <label class="col" style="gap: 4px;">
              <span class="muted">Sprint</span>
              <select class="input" v-model="sprintDialogSelection" :disabled="!sprintOptions.length">
                <option v-for="opt in sprintOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
              </select>
            </label>
            <p v-if="!sprintOptions.length" class="muted tasks-modal__hint">No sprints available yet. Create one first.</p>
            <label v-if="sprintDialogMode === 'add'" class="row tasks-modal__checkbox">
              <input type="checkbox" v-model="sprintDialogKeepExisting" />
              Keep existing sprint memberships
            </label>
            <label v-if="sprintDialogMode === 'add'" class="row tasks-modal__checkbox">
              <input type="checkbox" v-model="sprintDialogAllowClosed" />
              Allow assigning to closed sprints
            </label>
            <footer class="row tasks-modal__footer">
              <UiButton variant="primary" type="submit" :disabled="sprintDialogSubmitting || !sprintOptions.length">
                {{ sprintDialogSubmitting ? (sprintDialogMode === 'add' ? 'Assigning…' : 'Removing…') : (sprintDialogMode === 'add' ? 'Assign to sprint' : 'Remove from sprint') }}
              </UiButton>
              <UiButton variant="ghost" type="button" :disabled="sprintDialogSubmitting" @click="closeSprintDialog">Cancel</UiButton>
            </footer>
          </form>
        </UiCard>
      </div>
    </Teleport>

    <Teleport to="body">
      <div
        v-if="deleteDialogOpen"
        class="tasks-modal__overlay"
        role="dialog"
        aria-modal="true"
        :aria-label="deleteDialogTitle"
        @click.self="closeDeleteDialog"
      >
        <UiCard class="tasks-modal__card">
          <div class="col tasks-modal__form">
            <header class="row tasks-modal__header">
              <div class="col" style="gap: 4px;">
                <h2>{{ deleteDialogTitle }}</h2>
                <p class="muted tasks-modal__hint">This cannot be undone.</p>
              </div>
              <UiButton
                variant="ghost"
                icon-only
                type="button"
                aria-label="Close dialog"
                title="Close dialog"
                :disabled="deleteDialogSubmitting"
                @click="closeDeleteDialog"
              >
                <IconGlyph name="close" />
              </UiButton>
            </header>
            <p>Are you sure you want to delete the selected task{{ deleteDialogIds.length === 1 ? '' : 's' }}?</p>
            <footer class="row tasks-modal__footer">
              <UiButton variant="danger" type="button" :disabled="deleteDialogSubmitting" @click="submitDeleteDialog">
                {{ deleteDialogSubmitting ? 'Deleting…' : 'Delete' }}
              </UiButton>
              <UiButton variant="ghost" type="button" :disabled="deleteDialogSubmitting" @click="closeDeleteDialog">Cancel</UiButton>
            </footer>
          </div>
        </UiCard>
      </div>
    </Teleport>

  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '../api/client'
import type { TaskDTO } from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import IconGlyph from '../components/IconGlyph.vue'
import ReloadButton from '../components/ReloadButton.vue'
import SmartListChips from '../components/SmartListChips.vue'
import TaskTable from '../components/TaskTable.vue'
import { showToast } from '../components/toast'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiLoader from '../components/UiLoader.vue'
import { useActivity } from '../composables/useActivity'
import { useConfig } from '../composables/useConfig'
import { useProjects } from '../composables/useProjects'
import { useSprints } from '../composables/useSprints'
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
const { statuses, priorities, types, refresh: refreshConfig, customFields: availableCustomFields } = useConfig()
const statusOptions = computed(() => [...(statuses.value || [])])
const priorityOptions = computed(() => [...(priorities.value || [])])
const customFilterPresets = computed(() => {
  const names = (availableCustomFields.value || []).filter((name) => name !== '*')
  return names.slice(0, 6).map((name) => ({
    label: name,
    expression: `field:${name}=`,
  }))
})

const { sprints, loading: sprintsLoading, refresh: refreshSprints, active: activeSprints } = useSprints()
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
const sprintSelection = ref('active')
const allowClosedSprint = ref(false)
const sprintOptions = computed(() => {
  const options: Array<{ value: string; label: string }> = []
  const activeList = activeSprints.value
  const activeLabel = (() => {
    if (!activeList.length) return 'Auto (requires an active sprint)'
    if (activeList.length === 1) {
      const sprint = activeList[0]
      const name = sprint.label || sprint.display_name || `Sprint ${sprint.id}`
      return `Auto (active: #${sprint.id} ${name})`
    }
    return 'Auto (multiple active sprints – specify one)'
  })()
  options.push({ value: 'active', label: activeLabel })
  options.push({ value: 'next', label: 'Next sprint' })
  options.push({ value: 'previous', label: 'Previous sprint' })
  const sorted = [...sprints.value].sort((a, b) => a.id - b.id)
  sorted.forEach((item) => {
    const name = item.label || item.display_name || `Sprint ${item.id}`
    const state = item.state.charAt(0).toUpperCase() + item.state.slice(1)
    options.push({ value: String(item.id), label: `#${item.id} ${name} (${state})` })
  })
  return options
})
const hasSprints = computed(() => sprints.value.length > 0)

watch(
  () => sprintOptions.value,
  (options) => {
    if (!options.some((opt) => opt.value === sprintSelection.value)) {
      sprintSelection.value = options[0]?.value ?? 'active'
    }
  },
  { immediate: true },
)

const filter = ref<Record<string, string>>({})
const filterBarRef = ref<{ appendCustomFilter: (expr: string) => void; clear?: () => void } | null>(null)
const BUILTIN_QUERY_KEYS = new Set(['q', 'project', 'status', 'priority', 'type', 'assignee', 'tags', 'due', 'recent', 'needs'])
const hasFilters = computed(() => Object.entries(filter.value).some(([key, value]) => key !== 'order' && !!value))

function onFilterUpdate(v: Record<string,string>){ filter.value = v }
function onChipsUpdate(v: Record<string,string>){ filter.value = { ...v } }
function resetFilters(){
  filter.value = {}
  selectedIds.value = []
  filterBarRef.value?.clear?.()
}
function clearFilters(){ resetFilters() }

function handleCustomPreset(expression: string) {
  filterBarRef.value?.appendCustomFilter(expression)
}

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
  if (disposed) return
  const onTasksRoute = router.currentRoute.value.path === '/'
  if (!onTasksRoute) return
  const q = { ...raw }
  const qnorm: Record<string, string> = {}
  const extraQuery: Record<string, string> = {}
  for (const [key, value] of Object.entries(q)) {
    if (!value || key === 'order') continue
    if (BUILTIN_QUERY_KEYS.has(key)) {
      qnorm[key] = value
    } else {
      extraQuery[key] = value
    }
  }
  qnorm.order = (q.order === 'asc' || q.order === 'desc') ? q.order : 'desc'
  const nextQuery = { ...extraQuery, ...qnorm }

  if (disposed) return

  if (nav === 'replace') {
    if (!disposed) await router.replace({ path: '/', query: nextQuery })
  } else if (nav === 'push') {
    if (!disposed) await router.push({ path: '/', query: nextQuery })
  }

  const serverFilter: any = {}
  if (qnorm.q) serverFilter.q = qnorm.q
  if (qnorm.project) serverFilter.project = qnorm.project
  if (qnorm.status) serverFilter.status = qnorm.status.split(',').map(s => s.trim()).filter(Boolean)
  if (qnorm.priority) serverFilter.priority = qnorm.priority.split(',').map(s => s.trim()).filter(Boolean)
  if (qnorm.type) serverFilter.type = qnorm.type.split(',').map(s => s.trim()).filter(Boolean)
  if (qnorm.assignee && qnorm.assignee !== '__none__') serverFilter.assignee = qnorm.assignee
  if (qnorm.tags) serverFilter.tags = qnorm.tags.split(',').map(s => s.trim()).filter(Boolean)
  Object.assign(serverFilter, extraQuery)

  if (disposed) return

  await refreshConfig(serverFilter.project)
  if (disposed) return
  await refresh(serverFilter)
  if (disposed) return

  applySmartFilters(qnorm)

  if (disposed) return

  const dir = qnorm.order === 'asc' ? 'asc' : 'desc'
  items.value.sort((a,b) => (dir === 'desc' ? b.modified.localeCompare(a.modified) : a.modified.localeCompare(b.modified)))
}

async function retry(){ await applyFilter(filter.value, 'none') }


const view = (id: string) => openTask(id)

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

// Selection and bulk dialogs
const bulk = ref(false)
const selectedIds = ref<string[]>([])

function setSelectedIds(value: string[]) {
  selectedIds.value = Array.isArray(value) ? [...value] : []
}

const ASSIGNEE_STORAGE_KEY = 'lotar.tasks.assign.last'
const lastAssignee = ref('@me')
if (typeof window !== 'undefined') {
  try {
    const stored = window.localStorage.getItem(ASSIGNEE_STORAGE_KEY)
    if (stored) lastAssignee.value = stored
  } catch {
    // ignore storage errors
  }
}

const assignDialogOpen = ref(false)
const assignDialogSubmitting = ref(false)
const assignDialogIds = ref<string[]>([])
const assignDialogMode = ref<'single' | 'bulk'>('bulk')
const assignInputValue = ref('')
const assignInputRef = ref<HTMLInputElement | null>(null)

const assignDialogCount = computed(() => assignDialogIds.value.length)
const assignDialogTitle = computed(() =>
  assignDialogMode.value === 'single'
    ? 'Assign task'
    : `Assign ${assignDialogCount.value} task${assignDialogCount.value === 1 ? '' : 's'}`,
)

function openAssignDialog(ids: string[], mode: 'single' | 'bulk') {
  const unique = Array.from(new Set(ids))
  if (!unique.length) {
    showToast('Select at least one task first')
    return
  }
  assignDialogIds.value = unique
  assignDialogMode.value = mode
  assignInputValue.value = lastAssignee.value || '@me'
  assignDialogOpen.value = true
  nextTick(() => {
    assignInputRef.value?.focus()
    assignInputRef.value?.select()
  })
}

function closeAssignDialog(force?: boolean | Event) {
  const forced = force === true
  if (assignDialogSubmitting.value && !forced) return
  assignDialogOpen.value = false
  assignDialogIds.value = []
}

function useAssignShortcut(value: string) {
  assignInputValue.value = value
  nextTick(() => assignInputRef.value?.focus())
}

async function applyAssignment(ids: string[], assignee: string) {
  const unique = Array.from(new Set(ids))
  const failures: Array<{ id: string; error: unknown }> = []
  let success = 0
  for (const id of unique) {
    try {
      const updated = await api.updateTask(id, { assignee })
      const idx = tasks.value.findIndex((t) => t.id === id)
      if (idx >= 0) tasks.value[idx] = updated
      success += 1
    } catch (error) {
      failures.push({ id, error })
    }
  }
  return { success, failures }
}

async function submitAssignDialog() {
  if (assignDialogSubmitting.value) return
  const input = (assignInputValue.value || '').trim()
  if (!input) {
    showToast('Enter an assignee or use @me')
    return
  }
  assignDialogSubmitting.value = true
  try {
    const { success, failures } = await applyAssignment(assignDialogIds.value, input)
    if (success) {
      const message =
        assignDialogMode.value === 'single'
          ? `Task assigned to ${input}`
          : `Assigned ${success} task${success === 1 ? '' : 's'} to ${input}`
      showToast(message)
    }
    if (failures.length) {
      showToast(`Failed to assign ${failures.length} task${failures.length === 1 ? '' : 's'}`)
      console.error('Assignment errors', failures)
    }
    lastAssignee.value = input
    if (typeof window !== 'undefined') {
      try {
        window.localStorage.setItem(ASSIGNEE_STORAGE_KEY, input)
      } catch {
        // ignore
      }
    }
  closeAssignDialog(true)
  } finally {
    assignDialogSubmitting.value = false
  }
}

function openSingleAssign(id: string) {
  openAssignDialog([id], 'single')
}

function openBulkAssign() {
  openAssignDialog(selectedIds.value, 'bulk')
}

async function unassignTasks(ids: string[]) {
  const unique = Array.from(new Set(ids))
  const failures: Array<{ id: string; error: unknown }> = []
  let success = 0
  for (const id of unique) {
    try {
      const updated = await api.updateTask(id, { assignee: '' as any })
      const idx = tasks.value.findIndex((t) => t.id === id)
      if (idx >= 0) tasks.value[idx] = updated
      success += 1
    } catch (error) {
      failures.push({ id, error })
    }
  }
  if (success) {
    showToast(`Unassigned ${success} task${success === 1 ? '' : 's'}`)
  }
  if (failures.length) {
    showToast(`Failed to unassign ${failures.length} task${failures.length === 1 ? '' : 's'}`)
    console.error('Unassign errors', failures)
  }
}

async function unassignOne(id: string) {
  await unassignTasks([id])
}

async function bulkUnassign() {
  if (!selectedIds.value.length) {
    showToast('Select at least one task first')
    return
  }
  await unassignTasks(selectedIds.value)
}

function parseSprintToken(token: string): number | string | undefined {
  const trimmed = (token || '').trim()
  if (!trimmed || trimmed === 'active' || trimmed === 'auto') return undefined
  if (trimmed === 'next') return 'next'
  if (trimmed === 'previous' || trimmed === 'prev') return 'previous'
  const numeric = Number(trimmed)
  if (Number.isInteger(numeric) && numeric > 0) return numeric
  return trimmed
}

async function performSprintAction(
  taskIds: string[],
  mode: 'add' | 'remove',
  options: { sprint?: string; allowClosed?: boolean; keepExisting?: boolean } = {},
) {
  if (!taskIds.length) {
    showToast('Select at least one task first')
    return
  }
  const payload: any = { tasks: taskIds }
  const token = options.sprint ?? sprintSelection.value
  const sprintRef = parseSprintToken(token)
  if (sprintRef !== undefined) payload.sprint = sprintRef
  if (mode === 'add' && (options.allowClosed ?? allowClosedSprint.value)) payload.allow_closed = true
  payload.cleanup_missing = true
  const keepExisting = options.keepExisting ?? false
  if (mode === 'add' && !keepExisting) payload.force_single = true
  try {
    const response = mode === 'add' ? await api.sprintAdd(payload) : await api.sprintRemove(payload)
    const changed = response.modified.length
    const sprintName = response.sprint_label || `Sprint #${response.sprint_id}`
    if (changed > 0) {
      const verb = mode === 'add' ? 'Added' : 'Removed'
      showToast(`${verb} ${changed} task(s) ${mode === 'add' ? 'to' : 'from'} ${sprintName}`)
    } else {
      showToast(mode === 'add' ? 'No tasks assigned' : 'No tasks removed')
    }
    const messages = Array.isArray(response.messages) ? response.messages : []
    if (messages.length) {
      messages.forEach((message) => showToast(message))
    } else if (mode === 'add' && Array.isArray(response.replaced) && response.replaced.length) {
      response.replaced.forEach((entry) => {
        if (!entry?.previous?.length) return
        const prev = entry.previous.map((id) => `#${id}`).join(', ')
        showToast(`${entry.task_id} moved from ${prev}`)
      })
    }
    const autoCleanup = response.integrity?.auto_cleanup
    if (autoCleanup?.removed_references) {
      showToast(`Automatically cleaned ${autoCleanup.removed_references} dangling sprint reference(s).`)
    }
    if (response.integrity?.missing_sprints && response.integrity.missing_sprints.length) {
      showToast(`Still spotting missing sprint IDs: ${response.integrity.missing_sprints.map((id) => `#${id}`).join(', ')}`)
    }
    await refreshSprints(true)
    await applyFilter(filter.value, 'none')
  } catch (e:any) {
    showToast(e.message || (mode === 'add' ? 'Failed to assign to sprint' : 'Failed to remove from sprint'))
  }
}

const sprintDialogOpen = ref(false)
const sprintDialogSubmitting = ref(false)
const sprintDialogIds = ref<string[]>([])
const sprintDialogMode = ref<'add' | 'remove'>('add')
const sprintDialogSelection = ref('active')
const sprintDialogAllowClosed = ref(false)
const sprintDialogKeepExisting = ref(false)

watch(
  () => sprintSelection.value,
  (value) => {
    sprintDialogSelection.value = value
  },
  { immediate: true },
)

watch(
  () => allowClosedSprint.value,
  (value) => {
    sprintDialogAllowClosed.value = value
  },
  { immediate: true },
)

const sprintDialogTitle = computed(() =>
  sprintDialogMode.value === 'add'
    ? `Add ${sprintDialogIds.value.length} task${sprintDialogIds.value.length === 1 ? '' : 's'} to sprint`
    : `Remove ${sprintDialogIds.value.length} task${sprintDialogIds.value.length === 1 ? '' : 's'} from sprint`,
)

function openSprintDialog(ids: string[], mode: 'add' | 'remove') {
  const unique = Array.from(new Set(ids))
  if (!unique.length) {
    showToast('Select at least one task first')
    return
  }
  sprintDialogIds.value = unique
  sprintDialogMode.value = mode
  sprintDialogSelection.value = sprintSelection.value
  sprintDialogAllowClosed.value = allowClosedSprint.value
  sprintDialogKeepExisting.value = false
  sprintDialogOpen.value = true
}

function closeSprintDialog(force?: boolean | Event) {
  const forced = force === true
  if (sprintDialogSubmitting.value && !forced) return
  sprintDialogOpen.value = false
  sprintDialogIds.value = []
  sprintDialogKeepExisting.value = false
}

async function submitSprintDialog() {
  if (sprintDialogSubmitting.value) return
  if (!sprintOptions.value.length) {
    showToast('No sprints available yet')
    return
  }
  sprintDialogSubmitting.value = true
  try {
    await performSprintAction(sprintDialogIds.value, sprintDialogMode.value, {
      sprint: sprintDialogSelection.value,
      allowClosed: sprintDialogAllowClosed.value,
      keepExisting: sprintDialogKeepExisting.value,
    })
    sprintSelection.value = sprintDialogSelection.value
    allowClosedSprint.value = sprintDialogAllowClosed.value
  closeSprintDialog(true)
  } finally {
    sprintDialogSubmitting.value = false
  }
}

function openSingleSprintAdd(id: string) {
  openSprintDialog([id], 'add')
}

function openSingleSprintRemove(id: string) {
  openSprintDialog([id], 'remove')
}

function openBulkSprintAdd() {
  openSprintDialog(selectedIds.value, 'add')
}

function openBulkSprintRemove() {
  openSprintDialog(selectedIds.value, 'remove')
}

const deleteDialogOpen = ref(false)
const deleteDialogSubmitting = ref(false)
const deleteDialogIds = ref<string[]>([])
const deleteDialogMode = ref<'single' | 'bulk'>('single')

const deleteDialogTitle = computed(() =>
  deleteDialogMode.value === 'single'
    ? 'Delete task'
    : `Delete ${deleteDialogIds.value.length} selected task${deleteDialogIds.value.length === 1 ? '' : 's'}`,
)

function openDeleteDialog(ids: string[], mode: 'single' | 'bulk') {
  const unique = Array.from(new Set(ids))
  if (!unique.length) {
    showToast('Select at least one task first')
    return
  }
  deleteDialogIds.value = unique
  deleteDialogMode.value = mode
  deleteDialogOpen.value = true
}

function closeDeleteDialog(force?: boolean | Event) {
  const forced = force === true
  if (deleteDialogSubmitting.value && !forced) return
  deleteDialogOpen.value = false
  deleteDialogIds.value = []
}

async function deleteTasks(ids: string[]) {
  const unique = Array.from(new Set(ids))
  const failures: Array<{ id: string; error: unknown }> = []
  let success = 0
  for (const id of unique) {
    try {
      await remove(id)
      success += 1
      selectedIds.value = selectedIds.value.filter((value) => value !== id)
    } catch (error) {
      failures.push({ id, error })
    }
  }
  return { success, failures }
}

async function submitDeleteDialog() {
  if (deleteDialogSubmitting.value) return
  deleteDialogSubmitting.value = true
  try {
    const { success, failures } = await deleteTasks(deleteDialogIds.value)
    if (success) {
      showToast(`Deleted ${success} task${success === 1 ? '' : 's'}`)
    }
    if (failures.length) {
      showToast(`Failed to delete ${failures.length} task${failures.length === 1 ? '' : 's'}`)
      console.error('Delete errors', failures)
    }
  closeDeleteDialog(true)
  } finally {
    deleteDialogSubmitting.value = false
  }
}

function openSingleDelete(id: string) {
  openDeleteDialog([id], 'single')
}

function openBulkDelete() {
  openDeleteDialog(selectedIds.value, 'bulk')
}

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
  await refreshSprints(true)
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
.filter-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 0;
}

.tasks-modal__overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: color-mix(in oklab, var(--color-bg, #0f172a) 20%, transparent);
  z-index: 1000;
}

.tasks-modal__card {
  width: min(520px, 100%);
  max-height: calc(100vh - 48px);
  overflow-y: auto;
}

.tasks-modal__form {
  gap: 16px;
}

.tasks-modal__header {
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
  flex-wrap: wrap;
}

.tasks-modal__footer {
  gap: 8px;
  flex-wrap: wrap;
}

.tasks-modal__hint {
  font-size: var(--text-sm, 0.875rem);
}

.tasks-modal__checkbox {
  gap: 8px;
  align-items: center;
}
</style>
