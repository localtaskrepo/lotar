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
  tags: string[]
  customFields: Record<string, string>
}

type SprintDefaults = {
  length: string
  overdue_after: string
  capacity_points: number | null
  capacity_hours: number | null
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
    tags: [],
    customFields: {},
  })

  const sprintDefaults = reactive<SprintDefaults>({
    length: '',
    overdue_after: '',
    capacity_points: null,
    capacity_hours: null,
  })

  const statuses = ref<string[]>([])
  const priorities = ref<string[]>([])
  const types = ref<string[]>([])
  const customFieldKeys = ref<string[]>([])
  const tags = ref<string[]>([])
  const members = ref<string[]>([])

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
    customFieldKeys.value = []
    tags.value = []
    members.value = []
    defaults.project = currentProject.value || ''
    defaults.status = ''
    defaults.priority = ''
    defaults.type = ''
    defaults.reporter = ''
    defaults.assignee = ''
    defaults.tags = []
    defaults.customFields = {}
    sprintDefaults.length = ''
    sprintDefaults.overdue_after = ''
    sprintDefaults.capacity_points = null
    sprintDefaults.capacity_hours = null
  }

  function applyConfig(config: any) {
    const extractLists = (c: any) => ({
      statuses: sanitizeStringList(pickArray(c, ['issue_states', 'values'], pickArray(c, ['issue_states']))),
      priorities: sanitizeStringList(pickArray(c, ['issue_priorities', 'values'], pickArray(c, ['issue_priorities']))),
      types: sanitizeStringList(pickArray(c, ['issue_types', 'values'], pickArray(c, ['issue_types']))),
      tags: sanitizeStringList(pickArray(c, ['tags', 'values'], pickArray(c, ['tags']))),
      customFields: sanitizeStringList(pickArray(c, ['custom_fields', 'values'], pickArray(c, ['custom_fields']))),
    })

    const base = extractLists(config)
    statuses.value = base.statuses
    priorities.value = base.priorities
    types.value = base.types
    tags.value = base.tags
    customFieldKeys.value = base.customFields
    members.value = sanitizeStringList(pickArray(config, ['members'], pickArray(config, ['default', 'members'])))

    const defaultStatus = config?.default_status
    const defaultPriority = config?.default_priority
    const defaultReporter = config?.default_reporter
    const defaultAssignee = config?.default_assignee
    const defaultTags = config?.default_tags
    const defaultProject = config?.default_project || ''

    defaults.project = currentProject.value || String(defaultProject || '')
    defaults.status = (defaultStatus && String(defaultStatus)) || statuses.value[0] || ''
    defaults.priority = (defaultPriority && String(defaultPriority)) || priorities.value[0] || ''
    defaults.type = types.value[0] || ''
    defaults.reporter = (defaultReporter && String(defaultReporter)) || ''
    defaults.assignee = (defaultAssignee && String(defaultAssignee)) || ''
    defaults.tags = Array.isArray(defaultTags)
      ? sanitizeStringList(defaultTags.map((t: any) => String(t)))
      : []
    defaults.customFields = {}

    const sprintDefaultsConfig = config?.sprints?.defaults ?? {}
    sprintDefaults.length = typeof sprintDefaultsConfig.length === 'string' ? sprintDefaultsConfig.length : ''
    sprintDefaults.overdue_after = typeof sprintDefaultsConfig.overdue_after === 'string' ? sprintDefaultsConfig.overdue_after : ''
    sprintDefaults.capacity_points = typeof sprintDefaultsConfig.capacity_points === 'number'
      ? sprintDefaultsConfig.capacity_points
      : null
    sprintDefaults.capacity_hours = typeof sprintDefaultsConfig.capacity_hours === 'number'
      ? sprintDefaultsConfig.capacity_hours
      : null
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
    customFields: customFieldKeys,
    tags,
    members,
    defaults: computed(() => defaults),
    sprintDefaults: computed(() => sprintDefaults),
    refresh,
  }
}
