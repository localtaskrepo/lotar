import { computed, reactive, ref, watch } from 'vue'
import { api } from '../api/client'
import { createResource } from './useResource'

type ConfigDefaults = {
  project: string
  status: string
  priority: string
  type: string
  reporter: string
  assignee: string
  category: string
  tags: string[]
}

function sanitizeStringList(list: string[]): string[] {
  return list
    .map((item) => item.trim())
    .filter((item) => item.length > 0)
}

function pick(obj: any, path: string[]): any {
  let cur = obj
  for (const p of path) {
    if (!cur || typeof cur !== 'object') return undefined
    cur = cur[p]
  }
  return cur
}

function pickArray(obj: any, path: string[], fallback: string[] = []): string[] {
  const value = pick(obj, path)
  if (Array.isArray(value)) {
    return value.map((item) => String(item))
  }
  return fallback
}

export function useConfig() {
  const currentProject = ref<string>('')
  const defaults = reactive<ConfigDefaults>({
    project: '',
    status: '',
    priority: '',
    type: '',
    reporter: '',
    assignee: '',
    category: '',
    tags: [],
  })

  const statuses = ref<string[]>([])
  const priorities = ref<string[]>([])
  const types = ref<string[]>([])
  const categories = ref<string[]>([])
  const tags = ref<string[]>([])

  const cfgResource = createResource<any, [string?]>(
    async (project?: string) => {
      const projParam = project && project.trim().length > 0 ? project : undefined
      return api.showConfig(projParam)
    },
  )

  const cfg = computed<any>(() => cfgResource.data.value ?? null)
  const loading = cfgResource.loading
  const error = computed<string | null>(() => cfgResource.error.value?.message ?? null)

  function resetState() {
    statuses.value = []
    priorities.value = []
    types.value = []
    categories.value = []
    tags.value = []
    defaults.project = currentProject.value || ''
    defaults.status = ''
    defaults.priority = ''
    defaults.type = ''
    defaults.reporter = ''
    defaults.assignee = ''
    defaults.category = ''
    defaults.tags = []
  }

  function applyConfig(config: any) {
    const extractLists = (c: any) => ({
      statuses: sanitizeStringList(pickArray(c, ['issue_states', 'values'], pickArray(c, ['issue_states']))),
      priorities: sanitizeStringList(pickArray(c, ['issue_priorities', 'values'], pickArray(c, ['issue_priorities']))),
      types: sanitizeStringList(pickArray(c, ['issue_types', 'values'], pickArray(c, ['issue_types']))),
      categories: sanitizeStringList(pickArray(c, ['categories', 'values'], pickArray(c, ['categories']))),
      tags: sanitizeStringList(pickArray(c, ['tags', 'values'], pickArray(c, ['tags']))),
    })

    const base = extractLists(config)
    statuses.value = base.statuses
    priorities.value = base.priorities
    types.value = base.types
    categories.value = base.categories
    tags.value = base.tags

    const defaultStatus = config?.default_status
    const defaultPriority = config?.default_priority
    const defaultReporter = config?.default_reporter
    const defaultAssignee = config?.default_assignee
    const defaultCategory = config?.default_category
    const defaultTags = config?.default_tags
    const defaultPrefix = config?.default_prefix || config?.default_project || ''

    defaults.project = currentProject.value || String(defaultPrefix || '')
    defaults.status = (defaultStatus && String(defaultStatus)) || statuses.value[0] || ''
    defaults.priority = (defaultPriority && String(defaultPriority)) || priorities.value[0] || ''
    defaults.type = types.value[0] || ''
    defaults.reporter = (defaultReporter && String(defaultReporter)) || ''
    defaults.assignee = (defaultAssignee && String(defaultAssignee)) || ''
    defaults.category = (defaultCategory && String(defaultCategory)) || ''
    defaults.tags = Array.isArray(defaultTags)
      ? sanitizeStringList(defaultTags.map((t: any) => String(t)))
      : []
  }

  watch(
    () => cfgResource.data.value,
    (value) => {
      if (!value) {
        resetState()
        return
      }
      applyConfig(value)
    },
    { immediate: true },
  )

  async function refresh(project?: string) {
    const trimmed = project ? String(project).trim() : ''
    currentProject.value = trimmed
    await cfgResource.refresh(trimmed || undefined)
  }

  return {
    cfg,
    loading,
    error,
    statuses,
    priorities,
    types,
    categories,
    tags,
    defaults: computed(() => defaults),
    refresh,
  }
}
