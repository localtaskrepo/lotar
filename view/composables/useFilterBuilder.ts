import { computed, type Ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import type { TaskDTO, TaskListFilter } from '../api/types'
import { MS_PER_DAY, parseTaskDate, startOfLocalDay } from '../utils/date'

const BUILTIN_QUERY_KEYS = new Set([
  'q', 'project', 'status', 'priority', 'type', 'assignee', 'tags', 'due', 'recent', 'needs', 'sprints',
])


export function listFromCsv(value: string): string[] {
  return value.split(',').map((entry) => entry.trim()).filter(Boolean)
}

export function normalizeFilter(raw: Record<string, string>) {
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

export function buildServerFilter(
    raw: Record<string, string>,
    project: string,
): {
    serverFilter: TaskListFilter
    normalized: Record<string, string>
    extras: Record<string, string>
} {
    const { normalized, extras } = normalizeFilter(raw)
    const serverFilter: TaskListFilter = {}
    if (project) serverFilter.project = project
    if (normalized.q) serverFilter.q = normalized.q
    if (normalized.status) serverFilter.status = listFromCsv(normalized.status)
    if (normalized.priority) serverFilter.priority = listFromCsv(normalized.priority)
    if (normalized.type) serverFilter.type = listFromCsv(normalized.type)
    if (normalized.assignee && normalized.assignee !== '__none__') serverFilter.assignee = normalized.assignee
    if (normalized.tags) serverFilter.tags = listFromCsv(normalized.tags)
    if (normalized.sprints) serverFilter.sprints = listFromCsv(normalized.sprints).map(Number).filter(Number.isFinite)
    Object.assign(serverFilter, extras)
    return { serverFilter, normalized, extras }
}

export function applySmartFilters(
  taskList: TaskDTO[],
  q: Record<string, string>,
): TaskDTO[] {
  const wantsUnassigned = q.assignee === '__none__'
  const due = q.due || ''
  const recent = q.recent || ''
  const needsSet = new Set(listFromCsv(q.needs || ''))
  const now = new Date()
  const today = startOfLocalDay(now)
  const tomorrow = new Date(today.getTime() + MS_PER_DAY)
  const soonCutoff = new Date(today.getTime() + 7 * MS_PER_DAY)
  const recentCutoff = new Date(now.getTime() - 7 * MS_PER_DAY)

  if (!wantsUnassigned && !due && !recent && !needsSet.size) return taskList

  return taskList.filter((task) => {
    if (wantsUnassigned && (task.assignee || '').trim()) return false

    if (due) {
      const dueDate = parseTaskDate(task.due_date)
      if (!dueDate) return false
      const dueTime = startOfLocalDay(dueDate).getTime()
      const todayStart = today.getTime()
      const tomorrowStart = tomorrow.getTime()
      const soonCutoffTime = startOfLocalDay(soonCutoff).getTime()
      if (due === 'today' && (dueTime < todayStart || dueTime >= tomorrowStart)) return false
      if (due === 'soon' && (dueTime < tomorrowStart || dueTime > soonCutoffTime)) return false
      if (due === 'later' && dueTime <= soonCutoffTime) return false
      if (due === 'overdue' && dueTime >= todayStart) return false
    }

    if (recent === '7d') {
      const modified = parseTaskDate(task.modified)
      if (!modified || modified.getTime() < recentCutoff.getTime()) return false
    }

    if (needsSet.size) {
      if (needsSet.has('effort') && (task.effort || '').trim()) return false
      if (needsSet.has('due') && (task.due_date || '').trim()) return false
    }

    return true
  })
}

export function useProjectFilterSync(
  projectRef: Ref<string>,
  filterRef: Ref<Record<string, string>>,
  options?: {
    onProjectChange?: (project: string) => void
    /** When set (and no onProjectChange), keep ?project= in the URL in sync on this path. */
    routePath?: string
  },
) {
  const hasFilters = computed(() =>
    Object.entries(filterRef.value).some(([key, value]) => key !== 'order' && !!value),
  )

  const route = useRoute()
  const router = useRouter()

  function syncProjectRoute(nextProject: string) {
    const desired = nextProject || ''
    const current = typeof route.query.project === 'string' ? route.query.project : ''
    if (current === desired) return
    router.push({ path: options?.routePath ?? route.path, query: desired ? { project: desired } : {} })
  }

  function resolveProjectSelection(requested: string | undefined) {
    return (requested || '').trim()
  }

  function sanitizeFilterInput(payload: Record<string, string>) {
    const next: Record<string, string> = {}
    const hasProjectKey = payload && Object.prototype.hasOwnProperty.call(payload, 'project')
    if (hasProjectKey) {
      const nextProject = resolveProjectSelection(payload.project)
      if (nextProject !== projectRef.value) {
        projectRef.value = nextProject
      }
    }
    Object.entries(payload || {}).forEach(([key, value]) => {
      if (key === 'project') return
      if (value === undefined || value === null) return
      next[key] = value
    })
    const prev = filterRef.value
    const sameSize = Object.keys(next).length === Object.keys(prev).length
    if (sameSize) {
      const unchanged = Object.entries(next).every(([key, value]) => prev[key] === value)
      if (unchanged) return prev
    }
    return next
  }

  function onFilterUpdate(v: Record<string, string>) {
    const hasProjectKey = v && Object.prototype.hasOwnProperty.call(v, 'project')
    if (hasProjectKey) {
      const nextProject = (v.project || '').trim()
      if (options?.onProjectChange) {
        options.onProjectChange(nextProject)
      } else if (options?.routePath) {
        syncProjectRoute(nextProject)
      }
    }
    filterRef.value = sanitizeFilterInput(v)
  }

  function clearFilters(filterBarRef: Ref<{ clear?: () => void } | null>) {
    filterRef.value = {}
    filterBarRef.value?.clear?.()
  }

  return { hasFilters, sanitizeFilterInput, onFilterUpdate, clearFilters }
}

export function useCustomFilterPresets(customFieldNames: Ref<string[]>) {
  return computed(() => {
    const names = (customFieldNames.value || []).filter((name) => name !== '*')
    return names.slice(0, 6).map((name) => ({
      label: name,
      expression: `field:${name}=`,
    }))
  })
}
