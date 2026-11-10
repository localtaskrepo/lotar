import { computed, reactive, ref, watch } from 'vue'
import { api } from '../api/client'
import type { ConfigShowResponse } from '../api/types'
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
    return list.map((item) => item.trim()).filter((item) => item.length > 0)
}

function pick(obj: unknown, path: string[]): unknown {
    let cur = obj
    for (const p of path) {
        if (!cur || typeof cur !== 'object') return undefined
        cur = (cur as Record<string, unknown>)[p]
    }
    return cur
}

function pickArray(obj: unknown, path: string[], fallback: string[] = []): string[] {
    const value = pick(obj, path)
    if (Array.isArray(value)) {
        return value.map((item) => String(item))
    }
    return fallback
}

export function useConfig(options: { throwOnError?: boolean } = {}) {
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

    const cfgResource = createResource<ConfigShowResponse, [string?]>(
        async (project?: string) => {
            const projParam = project && project.trim().length > 0 ? project : undefined
            return api.showConfig(projParam)
        },
        { throwOnError: options.throwOnError },
    )

    const cfg = computed<ConfigShowResponse | null>(() => cfgResource.data.value ?? null)
    const loading = cfgResource.loading
    const error = computed<string | null>(() => cfgResource.error.value?.message ?? null)

    const attachmentsDir = computed<string>(() => {
        const raw = cfg.value?.attachments_dir
        const value = typeof raw === 'string' ? raw.trim() : ''
        return value || '@attachments'
    })

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

    function applyConfig(config: ConfigShowResponse) {
        const extractLists = (c: ConfigShowResponse) => ({
            statuses: sanitizeStringList(pickArray(c, ['issue_states', 'values'], pickArray(c, ['issue_states']))),
            priorities: sanitizeStringList(
                pickArray(c, ['issue_priorities', 'values'], pickArray(c, ['issue_priorities'])),
            ),
            types: sanitizeStringList(pickArray(c, ['issue_types', 'values'], pickArray(c, ['issue_types']))),
            tags: sanitizeStringList(pickArray(c, ['tags', 'values'], pickArray(c, ['tags']))),
            customFields: sanitizeStringList(
                pickArray(c, ['custom_fields', 'values'], pickArray(c, ['custom_fields'])),
            ),
        })

        const base = extractLists(config)
        statuses.value = base.statuses
        priorities.value = base.priorities
        types.value = base.types
        tags.value = base.tags
        customFieldKeys.value = base.customFields
        members.value = sanitizeStringList(pickArray(config, ['members'], pickArray(config, ['default', 'members'])))

        // An empty, missing or non-scalar setting means "not set".
        const text = (value: unknown) =>
            (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') && value
                ? String(value)
                : ''

        defaults.project = currentProject.value || text(config.default_project)
        defaults.status = text(config.default_status) || statuses.value[0] || ''
        defaults.priority = text(config.default_priority) || priorities.value[0] || ''
        defaults.type = types.value[0] || ''
        defaults.reporter = text(config.default_reporter)
        defaults.assignee = text(config.default_assignee)
        defaults.tags = sanitizeStringList(pickArray(config, ['default_tags']))
        defaults.customFields = {}

        const length = pick(config, ['sprints', 'defaults', 'length'])
        const overdueAfter = pick(config, ['sprints', 'defaults', 'overdue_after'])
        const capacityPoints = pick(config, ['sprints', 'defaults', 'capacity_points'])
        const capacityHours = pick(config, ['sprints', 'defaults', 'capacity_hours'])
        sprintDefaults.length = typeof length === 'string' ? length : ''
        sprintDefaults.overdue_after = typeof overdueAfter === 'string' ? overdueAfter : ''
        sprintDefaults.capacity_points = typeof capacityPoints === 'number' ? capacityPoints : null
        sprintDefaults.capacity_hours = typeof capacityHours === 'number' ? capacityHours : null
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
        const trimmed = project ? project.trim() : ''
        currentProject.value = trimmed
        cfgResource.reset()
        resetState()
        await cfgResource.refresh(trimmed || undefined)
    }

    return {
        scope: currentProject,
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
        attachmentsDir,
        refresh,
    }
}
