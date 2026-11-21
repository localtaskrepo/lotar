import { computed, nextTick, ref, watch, type ComputedRef, type Ref } from 'vue'
import { api } from '../api/client'
import type { AgentProfileInfo } from '../api/types'

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
    form: TaskPanelFormShape & Record<string, unknown>
    mode: ComputedRef<'create' | 'edit'>
    ready: Ref<boolean>
    suppressWatch: Ref<boolean>
    defaults: Ref<TaskPanelDefaultsShape>
    members: Ref<string[]>
    mergeKnownTags: (tags: Array<string | null | undefined>) => void
    updateField: (field: 'reporter' | 'assignee') => Promise<void> | void
}

export function useTaskPanelOwnership(options: UseTaskPanelOwnershipOptions) {
    const knownUsers = ref<string[]>([])
    // Custom field values the project's tasks hold, by field name: the
    // version input suggests them (DEV-395).
    const knownFieldValues = ref<Record<string, string[]>>({})
    const whoami = ref('')
    // Agent profiles; an assignee `@<profile>` starts that agent. The picker shows their runner, model and effort.
    const agentProfileInfos = ref<AgentProfileInfo[]>([])
    const agentProfiles = computed(() => agentProfileInfos.value.map((profile) => profile.name).filter(Boolean))

    const orderedKnownUsers = computed(() => {
        const base = knownUsers.value.slice().filter(Boolean)
        if (!whoami.value) return base
        const filtered = base.filter((user) => user !== whoami.value)
        return [whoami.value, ...filtered]
    })

    // People are offered without `@`, agent profiles as `@<profile>`.
    const personName = (value: string | null | undefined) => {
        const trimmed = (value || '').trim()
        const bare = trimmed.replace(/^@/, '')
        return trimmed.startsWith('@') && agentProfiles.value.includes(bare) ? trimmed : bare
    }

    const sanitizedConfiguredMembers = () => {
        return options.members.value.map(personName).filter((value) => value.length > 0)
    }

    const defaultOwnershipCandidates = () => {
        const defaults = options.defaults.value
        return [defaults.reporter, defaults.assignee].map(personName).filter((value) => value.length > 0)
    }

    const assignKnownUsersFrom = (set: Set<string>) => {
        knownUsers.value = Array.from(set)
            .filter(Boolean)
            .sort((a, b) => a.localeCompare(b))
    }

    const seedKnownUsers = () => {
        const baseline = new Set<string>()
        sanitizedConfiguredMembers().forEach((value) => baseline.add(value))
        defaultOwnershipCandidates().forEach((value) => baseline.add(value))
        agentProfiles.value.forEach((name) => baseline.add(`@${name}`))
        if (whoami.value) {
            baseline.add(whoami.value)
        }
        assignKnownUsersFrom(baseline)
        return baseline
    }

    /** A pick in the assignee or reporter picker; in edit mode the field watch saves it. */
    function setOwner(field: 'reporter' | 'assignee', value: string) {
        const next = value.trim()
        if ((options.form[field] ?? '') === next) return
        options.form[field] = next
    }

    async function loadAgentProfiles(scope?: string) {
        try {
            const response = await api.listAgentProfiles(scope)
            return response.profiles.filter((profile) => !!profile.name)
        } catch {
            return []
        }
    }

    async function loadTaskValues(scope?: string) {
        try {
            return await api.taskValues(scope)
        } catch {
            return null
        }
    }

    async function preloadPeople(project?: string | null) {
        const scope = project && project.trim() ? project.trim() : undefined
        // The distinct values only, not whole tasks (DEV-545).
        const [profiles, values] = await Promise.all([loadAgentProfiles(scope), loadTaskValues(scope)])
        agentProfileInfos.value = profiles
        const baseline = seedKnownUsers()
        if (values) {
            const seen = new Set<string>()
            values.people.forEach((value) => {
                const person = personName(value)
                if (person) seen.add(person)
            })
            knownFieldValues.value = values.custom_fields
            baseline.forEach((value) => seen.add(value))
            assignKnownUsersFrom(seen)
            options.mergeKnownTags(values.tags)
        } else {
            assignKnownUsersFrom(baseline)
        }
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
                    void nextTick(() => options.updateField(field))
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
            void nextTick(() => options.updateField('reporter'))
        },
    )

    watch(
        () => options.form.assignee,
        (next, prev) => {
            if (options.mode.value !== 'edit' || !options.ready.value || options.suppressWatch.value) return
            const current = (next ?? '').trim()
            const previous = (prev ?? '').trim()
            if (current === previous) return
            void nextTick(() => options.updateField('assignee'))
        },
    )

    watch(
        () => options.form.assignee,
        (value) => applyWhoamiShortcut('assignee', value),
    )

    watch(
        () => options.form.reporter,
        (value) => applyWhoamiShortcut('reporter', value),
    )

    // Fetch whoami once per panel lifecycle
    api.whoami()
        .then((value) => {
            const identity = typeof value === 'string' ? value.trim() : ''
            if (!identity) {
                return
            }
            whoami.value = identity
            applyWhoamiShortcut('reporter', options.form.reporter)
            applyWhoamiShortcut('assignee', options.form.assignee)
        })
        .catch(() => {})

    return {
        knownUsers,
        knownFieldValues,
        whoami,
        agentProfileInfos,
        orderedKnownUsers,
        setOwner,
        preloadPeople,
    }
}
