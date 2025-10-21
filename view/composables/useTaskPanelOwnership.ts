import { computed, nextTick, ref, watch, type ComputedRef, type Ref } from 'vue'
import { api } from '../api/client'

export type OwnershipMode = 'select' | 'custom'

export interface TaskPanelFormShape {
    id?: string
    project?: string
    reporter?: string
    assignee?: string
}

export interface TaskPanelDefaultsShape {
    reporter?: string | null
    assignee?: string | null
}

interface UseTaskPanelOwnershipOptions {
    form: TaskPanelFormShape & Record<string, any>
    mode: ComputedRef<'create' | 'edit'>
    ready: Ref<boolean>
    suppressWatch: Ref<boolean>
    defaults: Ref<TaskPanelDefaultsShape>
    mergeKnownTags: (tags: Array<string | null | undefined>) => void
    updateField: (field: 'reporter' | 'assignee') => Promise<void> | void
    onFieldBlur: (field: 'reporter' | 'assignee') => void
}

export function useTaskPanelOwnership(options: UseTaskPanelOwnershipOptions) {
    const knownUsers = ref<string[]>([])
    const whoami = ref('')

    const reporterMode = ref<OwnershipMode>('select')
    const assigneeMode = ref<OwnershipMode>('select')
    const reporterCustom = ref('')
    const assigneeCustom = ref('')

    const orderedKnownUsers = computed(() => {
        const base = knownUsers.value.slice().filter(Boolean)
        if (!whoami.value) return base
        const filtered = base.filter((user) => user !== whoami.value)
        return [whoami.value, ...filtered]
    })

    const reporterSelection = computed<string>({
        get() {
            const value = options.form.reporter?.trim() || ''
            if (!value) return ''
            return orderedKnownUsers.value.includes(value) ? value : '__custom'
        },
        set(value) {
            if (value === '__custom') {
                reporterMode.value = 'custom'
                reporterCustom.value = options.form.reporter || reporterCustom.value || ''
                return
            }
            reporterMode.value = 'select'
            reporterCustom.value = ''
            if (options.form.reporter !== value) {
                options.form.reporter = value
                syncReporterControl()
            }
        },
    })

    const assigneeSelection = computed<string>({
        get() {
            const value = options.form.assignee?.trim() || ''
            if (!value) return ''
            return orderedKnownUsers.value.includes(value) ? value : '__custom'
        },
        set(value) {
            if (value === '__custom') {
                assigneeMode.value = 'custom'
                assigneeCustom.value = options.form.assignee || assigneeCustom.value || ''
                return
            }
            assigneeMode.value = 'select'
            assigneeCustom.value = ''
            if (options.form.assignee !== value) {
                options.form.assignee = value
                syncAssigneeControl()
            }
        },
    })

    function setReporterSelection(value: string) {
        reporterSelection.value = value
    }

    function setAssigneeSelection(value: string) {
        assigneeSelection.value = value
    }

    function setReporterCustom(value: string) {
        reporterCustom.value = value
    }

    function setAssigneeCustom(value: string) {
        assigneeCustom.value = value
    }

    function commitReporterCustom() {
        options.form.reporter = reporterCustom.value.trim()
        syncReporterControl()
        options.onFieldBlur('reporter')
    }

    function commitAssigneeCustom() {
        options.form.assignee = assigneeCustom.value.trim()
        syncAssigneeControl()
        options.onFieldBlur('assignee')
    }

    function resetReporterSelection() {
        reporterSelection.value = ''
        options.onFieldBlur('reporter')
    }

    function resetAssigneeSelection() {
        assigneeSelection.value = ''
        options.onFieldBlur('assignee')
    }

    function syncAssigneeControl() {
        const value = options.form.assignee?.trim() || ''
        if (!value) {
            assigneeMode.value = 'select'
            assigneeCustom.value = ''
            return
        }
        if (orderedKnownUsers.value.includes(value)) {
            assigneeMode.value = 'select'
            assigneeCustom.value = ''
        } else {
            assigneeMode.value = 'custom'
            assigneeCustom.value = value
        }
    }

    function syncReporterControl() {
        const value = options.form.reporter?.trim() || ''
        if (!value) {
            reporterMode.value = 'select'
            reporterCustom.value = ''
            return
        }
        if (orderedKnownUsers.value.includes(value)) {
            reporterMode.value = 'select'
            reporterCustom.value = ''
        } else {
            reporterMode.value = 'custom'
            reporterCustom.value = value
        }
    }

    function syncOwnershipControls() {
        syncAssigneeControl()
        syncReporterControl()
    }

    async function preloadPeople(project?: string | null) {
        const scope = project && project.trim() ? project.trim() : undefined
        try {
            const list = await api.listTasks(scope ? ({ project: scope } as any) : ({} as any))
            const seen = new Set<string>()
            const tags = new Set<string>()
            list.forEach((item) => {
                if (item.assignee) seen.add(item.assignee)
                const reporter = (item as any).reporter
                if (reporter) seen.add(String(reporter))
                    ; (item.tags || []).forEach((tag) => {
                        if (!tag) return
                        const trimmed = tag.trim()
                        if (trimmed) tags.add(trimmed)
                    })
            })
            const defaults = options.defaults.value || {}
            const defaultsSet = [defaults.reporter, defaults.assignee].filter(Boolean) as string[]
            defaultsSet.forEach((value) => seen.add(value))
            if (whoami.value) {
                seen.add(whoami.value)
            }
            knownUsers.value = Array.from(seen).filter(Boolean).sort((a, b) => a.localeCompare(b))
            options.mergeKnownTags(Array.from(tags))
        } catch {
            knownUsers.value = []
        }
    }

    function resetOwnership() {
        reporterMode.value = 'select'
        assigneeMode.value = 'select'
        reporterCustom.value = ''
        assigneeCustom.value = ''
    }

    function applyWhoamiShortcut(field: 'reporter' | 'assignee', value: string | undefined | null) {
        if (!value) {
            return false
        }
        const trimmed = value.trim()
        if (trimmed.toLowerCase() === '@me' && whoami.value) {
            const resolved = whoami.value
            if (resolved !== options.form[field]) {
                options.form[field] = resolved
                if (options.mode.value === 'edit' && options.ready.value && !options.suppressWatch.value) {
                    nextTick(() => options.updateField(field))
                }
            }
            return true
        }
        return false
    }

    watch(
        () => options.form.reporter,
        (next, prev) => {
            if (options.mode.value !== 'edit' || !options.ready.value || options.suppressWatch.value) return
            const current = (next ?? '').trim()
            const previous = (prev ?? '').trim()
            if (current === previous) return
            nextTick(() => options.updateField('reporter'))
        },
    )

    watch(
        () => options.form.assignee,
        (next, prev) => {
            if (options.mode.value !== 'edit' || !options.ready.value || options.suppressWatch.value) return
            const current = (next ?? '').trim()
            const previous = (prev ?? '').trim()
            if (current === previous) return
            nextTick(() => options.updateField('assignee'))
        },
    )

    watch(
        () => options.form.assignee,
        (value) => {
            if (!applyWhoamiShortcut('assignee', value)) {
                syncAssigneeControl()
            }
        },
    )

    watch(
        () => options.form.reporter,
        (value) => {
            if (!applyWhoamiShortcut('reporter', value)) {
                syncReporterControl()
            }
        },
    )

    watch([knownUsers, whoami], () => {
        syncOwnershipControls()
    })

    // Fetch whoami once per panel lifecycle
    api
        .whoami()
        .then((value) => {
            const identity = typeof value === 'string' ? value.trim() : ''
            if (!identity) {
                return
            }
            whoami.value = identity
            const reporterResolved = applyWhoamiShortcut('reporter', options.form.reporter)
            const assigneeResolved = applyWhoamiShortcut('assignee', options.form.assignee)
            if (!reporterResolved) {
                syncReporterControl()
            }
            if (!assigneeResolved) {
                syncAssigneeControl()
            }
        })
        .catch(() => { })

    return {
        knownUsers,
        whoami,
        reporterMode,
        assigneeMode,
        reporterCustom,
        assigneeCustom,
        orderedKnownUsers,
        reporterSelection,
        assigneeSelection,
        setReporterSelection,
        setAssigneeSelection,
        setReporterCustom,
        setAssigneeCustom,
        commitReporterCustom,
        commitAssigneeCustom,
        resetReporterSelection,
        resetAssigneeSelection,
        preloadPeople,
        resetOwnership,
        syncOwnershipControls,
    }
}
