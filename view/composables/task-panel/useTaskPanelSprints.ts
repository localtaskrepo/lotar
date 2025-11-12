import type { ComputedRef, Ref } from 'vue'
import { computed, ref } from 'vue'
import type { SprintListItem, TaskDTO } from '../../api/types'

export interface UseTaskPanelSprintsOptions {
    form: { sprints: number[] }
    sprintList: Ref<SprintListItem[]>
    missingSprints: Ref<readonly number[] | undefined>
    activeSprints: Ref<SprintListItem[]>
}

export interface TaskPanelSprintsApi {
    sprintLookup: ComputedRef<Record<number, { label: string; state: string }>>
    assignedSprints: ComputedRef<Array<{ id: number; label: string; state: string; missing: boolean }>>
    hasAssignedSprints: ComputedRef<boolean>
    assignedSprintNotice: ComputedRef<string>
    sprintOptions: ComputedRef<Array<{ value: string; label: string }>>
    hasSprints: ComputedRef<boolean>
    resetSprintsState: () => void
    applyTaskSprints: (task: TaskDTO) => void
}

function toSprintLabel(base: SprintListItem): string {
    const name = base.label || base.display_name || `Sprint ${base.id}`
    return `#${base.id} ${name}`.trim()
}

function directoryMissingSet(values: readonly number[] | undefined) {
    const set = new Set<number>()
    if (!values) return set
    values.forEach((value) => {
        const id = Number(value)
        if (Number.isFinite(id)) {
            set.add(id)
        }
    })
    return set
}

export function useTaskPanelSprints(options: UseTaskPanelSprintsOptions): TaskPanelSprintsApi {
    const taskSprintOrder = ref<Record<number, number>>({})

    const sprintLookup = computed(() => {
        const lookup: Record<number, { label: string; state: string }> = {}
        for (const sprint of options.sprintList.value) {
            const baseLabel = sprint.label || sprint.display_name || `Sprint ${sprint.id}`
            lookup[sprint.id] = {
                label: `#${sprint.id} ${baseLabel}`.trim(),
                state: sprint.state || 'unknown',
            }
        }
        return lookup
    })

    const assignedSprints = computed(() => {
        const rawMembership = Array.isArray(options.form.sprints) ? options.form.sprints : []
        if (!rawMembership.length) return []

        const membership: number[] = []
        const membershipSet = new Set<number>()
        for (const raw of rawMembership) {
            const id = Number(raw)
            if (!Number.isFinite(id) || id <= 0 || membershipSet.has(id)) continue
            membershipSet.add(id)
            membership.push(id)
        }

        const missing = directoryMissingSet(options.missingSprints.value)

        const orderedEntries: Array<{ id: number; order: number }> = []
        const orderSource = taskSprintOrder.value || {}
        Object.entries(orderSource).forEach(([rawId, rawOrder]) => {
            const id = Number(rawId)
            const order = Number(rawOrder)
            if (!Number.isFinite(id) || id <= 0) return
            orderedEntries.push({ id, order: Number.isFinite(order) ? order : Number.MAX_SAFE_INTEGER })
        })

        orderedEntries.sort((a, b) => {
            if (a.order !== b.order) return a.order - b.order
            return a.id - b.id
        })

        const idOrder: number[] = []
        const seen = new Set<number>()
        for (const entry of orderedEntries) {
            if (!membershipSet.has(entry.id) || seen.has(entry.id)) continue
            idOrder.push(entry.id)
            seen.add(entry.id)
        }

        if (seen.size !== membershipSet.size) {
            const remainder = membership
                .filter((id) => !seen.has(id))
                .sort((a, b) => a - b)
            idOrder.push(...remainder)
        }

        const result: Array<{ id: number; label: string; state: string; missing: boolean }> = []
        idOrder.forEach((id) => {
            const entry = sprintLookup.value[id]
            const state = (entry?.state || 'unknown').toLowerCase()
            const baseLabel = entry?.label ?? `#${id}`
            const isMissing = !entry || missing.has(id)
            result.push({
                id,
                label: isMissing ? `${baseLabel} (missing)` : baseLabel,
                state,
                missing: isMissing,
            })
        })

        return result
    })

    const hasAssignedSprints = computed(() => assignedSprints.value.length > 0)

    const assignedSprintNotice = computed(() => {
        const missing = assignedSprints.value.filter((item) => item.missing)
        if (!missing.length) return ''
        const formatted = missing.map((item) => `#${item.id}`).join(', ')
        return `Sprint metadata missing for ${formatted}.`
    })

    const sprintOptions = computed(() => {
        const optionsList: Array<{ value: string; label: string }> = []
        const activeList = options.activeSprints.value ?? []
        const activeLabel = (() => {
            if (!activeList.length) return 'Auto (requires an active sprint)'
            if (activeList.length === 1) {
                return `Auto (active: ${toSprintLabel(activeList[0])})`
            }
            return 'Auto (multiple active sprints – specify one)'
        })()
        optionsList.push({ value: 'active', label: activeLabel })
        optionsList.push({ value: 'next', label: 'Next sprint' })
        optionsList.push({ value: 'previous', label: 'Previous sprint' })

        const sorted = [...(options.sprintList.value ?? [])].sort((a, b) => a.id - b.id)
        sorted.forEach((item) => {
            const name = item.label || item.display_name || `Sprint ${item.id}`
            const state = item.state.charAt(0).toUpperCase() + item.state.slice(1)
            optionsList.push({ value: String(item.id), label: `#${item.id} ${name} (${state})` })
        })
        return optionsList
    })

    const hasSprints = computed(() => (options.sprintList.value?.length ?? 0) > 0)

    const resetSprintsState = () => {
        taskSprintOrder.value = {}
        options.form.sprints = []
    }

    const applyTaskSprints = (data: TaskDTO) => {
        const normalizedSprints: number[] = []
        const orderPairs: Array<[number, number]> = []
        const orderSource = data.sprint_order as Record<string, number> | undefined
        if (orderSource && typeof orderSource === 'object') {
            Object.entries(orderSource).forEach(([rawId, rawOrder]) => {
                const id = Number(rawId)
                if (!Number.isFinite(id) || id <= 0) return
                const orderValue = Number(rawOrder)
                const order = Number.isFinite(orderValue) ? orderValue : Number.MAX_SAFE_INTEGER
                orderPairs.push([id, order])
            })
        }

        orderPairs.sort((a, b) => {
            if (a[1] !== b[1]) return a[1] - b[1]
            return a[0] - b[0]
        })

        for (const [id] of orderPairs) {
            if (!normalizedSprints.includes(id)) {
                normalizedSprints.push(id)
            }
        }

        if (!normalizedSprints.length && Array.isArray(data.sprints)) {
            for (const value of data.sprints) {
                const id = Number(value)
                if (!Number.isFinite(id) || id <= 0) continue
                if (!normalizedSprints.includes(id)) {
                    normalizedSprints.push(id)
                }
            }
        }

        if (!orderPairs.length && normalizedSprints.length) {
            normalizedSprints.forEach((id, index) => {
                orderPairs.push([id, index + 1])
            })
        }

        taskSprintOrder.value = orderPairs.reduce<Record<number, number>>((acc, [id, order]) => {
            acc[id] = order
            return acc
        }, {})

        options.form.sprints = normalizedSprints
    }

    return {
        sprintLookup,
        assignedSprints,
        hasAssignedSprints,
        assignedSprintNotice,
        sprintOptions,
        hasSprints,
        resetSprintsState,
        applyTaskSprints,
    }
}
