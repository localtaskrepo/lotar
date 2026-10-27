import { computed, effectScope } from 'vue'
import { formatProjectLabel } from '../utils/projectLabels'
import { useProjects } from './useProjects'

let shared: ReturnType<typeof useProjects> | null = null

/**
 * Project names by prefix, for showing "Platform" instead of "PLAT"
 * (DEV-184). One project list is shared by every caller and loaded on first
 * use; it refreshes when projects change. Unknown prefixes show as-is.
 */
export function useProjectNames() {
    if (!shared) {
        shared = effectScope(true).run(() => useProjects())!
    }
    const projects = shared
    if (projects.status?.value === 'idle') void projects.refresh?.()?.catch(() => {})

    const byPrefix = computed(
        () => new Map((projects.projects?.value ?? []).map((project) => [project.prefix, project])),
    )

    /** The project's name, or its prefix when it has none (or is unknown). */
    function projectName(prefix: string | null | undefined): string {
        const key = (prefix ?? '').trim()
        if (!key) return ''
        return byPrefix.value.get(key)?.name?.trim() || key
    }

    /** "Name (PREFIX)", for places where both help. */
    function projectLabel(prefix: string | null | undefined): string {
        const key = (prefix ?? '').trim()
        if (!key) return ''
        const project = byPrefix.value.get(key)
        return project ? formatProjectLabel(project) : key
    }

    return { projectName, projectLabel }
}
