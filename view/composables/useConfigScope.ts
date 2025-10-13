import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '../api/client'
import type { ConfigInspectResult } from '../api/types'
import { useProjects } from './useProjects'
import { createResource } from './useResource'

export function useConfigScope() {
    const route = useRoute()
    const router = useRouter()
    const { projects, refresh: refreshProjects } = useProjects()

    const project = ref('')
    const lastLoadedAt = ref<Date | null>(null)
    const initialized = ref(false)

    const inspectResource = createResource<ConfigInspectResult | null, [string?]>(
        async (scope?: string) => api.inspectConfig(scope),
        { initialValue: null },
    )

    const inspectData = ref<ConfigInspectResult | null>(null)

    watch(
        () => inspectResource.data.value,
        (value) => {
            if (value !== undefined) {
                inspectData.value = value
            }
        },
    )

    const loading = inspectResource.loading
    const error = computed<string | null>(() => inspectResource.error.value?.message ?? null)

    async function reload(scope?: string) {
        const nextScope = typeof scope === 'string' ? scope.trim() : project.value
        const target = nextScope ? nextScope : undefined
        const result = await inspectResource.refresh(target)
        if (result !== undefined) {
            inspectData.value = result
            lastLoadedAt.value = new Date()
        }
        return inspectData.value
    }

    watch(
        project,
        async (value, previous) => {
            if (!initialized.value && value === previous) return
            const query = { ...route.query }
            if (value) {
                query.project = value
            } else {
                delete query.project
            }
            router.replace({ query })
            await reload(value)
        },
        { flush: 'post' },
    )

    onMounted(async () => {
        await refreshProjects()
        const fromRoute = typeof route.query.project === 'string' ? route.query.project : ''
        project.value = fromRoute
        await reload(fromRoute)
        initialized.value = true
    })

    return {
        projects,
        project,
        loading,
        error,
        inspectData,
        lastLoadedAt,
        reload,
    }
}
