import { computed, onMounted, ref, type Ref } from 'vue'
import { api } from '../api/client'
import type { ReleaseListItem } from '../api/types'
import { errorMessage } from '../utils/errors'

// One release list shared by every page and the task panel (DEV-393).
const releases = ref<ReleaseListItem[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const loaded = ref(false)
let pending: Promise<void> | null = null

let generation = 0

async function load(force = false): Promise<void> {
    if (pending && !force) return pending
    const request = ++generation
    pending = (async () => {
        loading.value = true
        error.value = null
        try {
            const response = await api.releaseList()
            releases.value = Array.isArray(response.releases) ? response.releases : []
        } catch (err: unknown) {
            error.value = errorMessage(err) || 'Failed to load releases'
            if (force) throw err
        } finally {
            loading.value = false
            loaded.value = true
            if (request === generation) pending = null
        }
    })()
    return pending
}

export function useReleases() {
    onMounted(() => {
        if (!loaded.value && !pending) void load()
    })
    return {
        releases,
        loading,
        error,
        loaded,
        refresh: (force = false) => load(force),
    }
}

/** A release's name for chips and menus: `0.15.0` or `0.15.0 (Spring)`. */
export function releaseLabel(release: Pick<ReleaseListItem, 'version' | 'label' | 'display_name'>): string {
    return release.display_name || (release.label ? `${release.version} (${release.label})` : release.version)
}

/** Value-source entries for the search grammar (`release:0.15.0`). */
export function useReleaseFilterOptions(list: Ref<ReleaseListItem[]>) {
    return computed(() => list.value.map((release) => ({ version: release.version, label: releaseLabel(release) })))
}
