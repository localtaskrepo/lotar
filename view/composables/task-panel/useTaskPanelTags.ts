import type { ComputedRef, Ref } from 'vue'
import { computed, ref, watch } from 'vue'

interface UseTaskPanelTagsOptions {
    form: { tags: string[] }
    mode: ComputedRef<'create' | 'edit'>
    ready: Ref<boolean>
    suppressWatch: Ref<boolean>
    configTags: Ref<string[] | undefined>
    applyPatch: (patch: Record<string, unknown>) => Promise<void>
}

export interface TaskPanelTagsApi {
    knownTags: Ref<string[]>
    allowCustomTags: ComputedRef<boolean>
    configTagOptions: ComputedRef<string[]>
    mergeKnownTags: (tags: Array<string | null | undefined>) => void
    setTags: (tags: string[]) => void
    startWatchers: () => void
}

export function useTaskPanelTags(options: UseTaskPanelTagsOptions): TaskPanelTagsApi {
    const knownTags = ref<string[]>([])
    let watcherStarted = false

    const allowCustomTags = computed(() => (options.configTags.value || []).includes('*'))

    const configTagOptions = computed(() => {
        const map = new Map<string, string>()
        for (const raw of (options.configTags.value || []).filter((tag) => tag !== '*')) {
            const trimmed = (raw || '').trim()
            if (!trimmed) continue
            const key = trimmed.toLowerCase()
            if (!map.has(key)) {
                map.set(key, trimmed)
            }
        }
        return Array.from(map.values())
    })

    const mergeKnownTags = (input: Array<string | null | undefined>) => {
        if (!input?.length) return
        const map = new Map<string, string>()
        knownTags.value.forEach((tag) => {
            const trimmed = tag.trim()
            if (!trimmed) return
            map.set(trimmed.toLowerCase(), trimmed)
        })
        input.forEach((raw) => {
            if (!raw) return
            const trimmed = raw.trim()
            if (!trimmed) return
            const key = trimmed.toLowerCase()
            if (!map.has(key)) {
                map.set(key, trimmed)
            }
        })
        knownTags.value = Array.from(map.values()).sort((a, b) => a.localeCompare(b))
    }

    const setTags = (tags: string[]) => {
        options.form.tags = [...tags]
        mergeKnownTags(tags)
    }

    const startWatchers = () => {
        if (watcherStarted) {
            return
        }
        watcherStarted = true
        watch(
            () => options.form.tags.slice(),
            async (tags, prev = []) => {
                if (!options.ready.value || options.suppressWatch.value || options.mode.value !== 'edit') {
                    return
                }
                const normalized = tags.map((tag) => tag.trim()).filter((tag) => tag.length > 0)
                const previous = prev.map((tag) => tag.trim()).filter((tag) => tag.length > 0)
                if (
                    normalized.length === previous.length &&
                    normalized.every((tag, index) => tag === previous[index])
                ) {
                    return
                }
                await options.applyPatch({ tags: normalized })
            },
        )
    }

    return {
        knownTags,
        allowCustomTags,
        configTagOptions,
        mergeKnownTags,
        setTags,
        startWatchers,
    }
}
