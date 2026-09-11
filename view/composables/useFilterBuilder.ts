import { computed, type Ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import type { TaskListFilter } from '../api/types'
import { normalizeSortBy, normalizeSortOrder } from '../utils/taskSort'

const BUILTIN_QUERY_KEYS = new Set([
  'q', 'project', 'status', 'priority', 'type', 'assignee', 'tags', 'due', 'recent', 'needs', 'sprints', 'sort_by', 'order',
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
    if (key === 'sort_by') {
      // Round-trip the raw value: valid keys normalize on the wire, invalid
      // ones are forwarded so the server's strict parser surfaces the error
      // instead of the UI silently re-sorting by default.
      normalized.sort_by = value.trim()
      continue
    }
    if (BUILTIN_QUERY_KEYS.has(key)) {
      normalized[key] = value
    } else {
      extras[key] = value
    }
  }
  normalized.order = source.order && source.order.trim() ? source.order : 'desc'
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
    // Forwarded verbatim, including `__none__` (unassigned) and `@me`, both of
    // which the server resolves.
    if (normalized.assignee) serverFilter.assignee = normalized.assignee
    if (normalized.tags) serverFilter.tags = listFromCsv(normalized.tags)
    if (normalized.sprints) {
      // Strict sprints CSV (backend rejects invalid entries): forward parsed
      // ids only when EVERY token is a positive integer; otherwise pass the
      // raw value through so the API rejects the query instead of the UI
      // silently widening it by dropping bad tokens.
      const tokens = listFromCsv(normalized.sprints)
      const ids = tokens.map((token) => Number(token))
      const allValid = tokens.length > 0 && tokens.every((token, i) => /^\d+$/.test(token) && ids[i]! > 0)
      serverFilter.sprints = allValid ? ids : normalized.sprints
    }
    if (normalized.due) serverFilter.due = normalized.due
    if (normalized.recent) serverFilter.recent = normalized.recent
    if (normalized.needs) serverFilter.needs = normalized.needs
    const sortByRaw = (normalized.sort_by || '').trim()
    if (sortByRaw) serverFilter.sort_by = normalizeSortBy(sortByRaw) ?? sortByRaw
    // Forwarded verbatim — including invalid values — so explicit errors
    // surface server-side instead of silently defaulting.
    serverFilter.order = normalized.order as TaskListFilter['order']
    Object.assign(serverFilter, extras)
    return { serverFilter, normalized, extras }
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
