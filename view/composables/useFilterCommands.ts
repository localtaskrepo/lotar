import { shallowRef } from 'vue'
import type { ProjectDTO } from '../api/types'

/**
 * The page's mounted FilterBar, so the command palette can apply quick
 * filters and switch projects on whichever page is open (DEV-198).
 */
export interface FilterBarHandle {
    /** Set these filters, replacing the current value of each key. */
    apply: (filters: Record<string, string>) => void
    /** Clear search and conditions, keeping the project and sort. */
    clear: () => void
    setProject: (prefix: string) => void
    projects: () => ProjectDTO[]
    currentProject: () => string
}

const active = shallowRef<FilterBarHandle | null>(null)

/** Register the mounted FilterBar; returns the function that unregisters it. */
export function registerFilterBar(handle: FilterBarHandle): () => void {
    active.value = handle
    return () => {
        if (active.value === handle) active.value = null
    }
}

export function useActiveFilterBar() {
    return active
}
