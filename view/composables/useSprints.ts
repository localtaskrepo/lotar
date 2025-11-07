import { computed, onMounted, ref } from 'vue'
import { api } from '../api/client'
import type { SprintIntegrityDiagnostics, SprintListItem } from '../api/types'

const sprints = ref<SprintListItem[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const integrity = ref<SprintIntegrityDiagnostics | null>(null)
const missingSprints = ref<number[]>([])
let initialized = false
let pending: Promise<void> | null = null

async function load(force = false): Promise<void> {
    if (pending && !force) {
        return pending
    }
    pending = (async () => {
        loading.value = true
        error.value = null
        try {
            const response = await api.sprintList()
            const normalized = Array.isArray(response.sprints)
                ? response.sprints.map((item) => ({
                    ...item,
                    warnings: Array.isArray((item as any).warnings) ? item.warnings : [],
                }))
                : []
            sprints.value = normalized
            missingSprints.value = Array.isArray(response.missing_sprints) ? response.missing_sprints : []
            integrity.value = response.integrity ?? null
        } catch (err: any) {
            error.value = err?.message || 'Failed to load sprints'
            if (force) {
                throw err
            }
        } finally {
            loading.value = false
            pending = null
            initialized = true
        }
    })()
    return pending
}

export function useSprints() {
    async function refresh(force = false) {
        await load(force)
    }

    onMounted(() => {
        if (!initialized && !pending) {
            void load()
        }
    })

    const active = computed(() => sprints.value.filter((item) => item.state === 'active' || item.state === 'overdue'))
    const defaultSprintId = computed<number | undefined>(() => (active.value.length === 1 ? active.value[0].id : undefined))
    const hasMissing = computed(() => missingSprints.value.length > 0)

    return {
        sprints,
        loading,
        error,
        refresh,
        active,
        defaultSprintId,
        integrity,
        missingSprints,
        hasMissing,
    }
}
