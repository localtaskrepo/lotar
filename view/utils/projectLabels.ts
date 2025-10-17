import type { ProjectDTO } from '../api/types'

export const PROJECT_LABEL_MAX_LENGTH = 28

function truncateName(name: string, maxLength: number): string {
    if (maxLength <= 1) {
        return name
    }
    if (name.length <= maxLength) {
        return name
    }
    const slicePoint = Math.max(0, maxLength - 1)
    const trimmed = name.slice(0, slicePoint).trimEnd()
    return `${trimmed}…`
}

export function formatProjectLabel(
    project: Pick<ProjectDTO, 'name' | 'prefix'>,
    maxLength = PROJECT_LABEL_MAX_LENGTH,
): string {
    const normalizedName = (project.name ?? '').trim()
    const normalizedPrefix = (project.prefix ?? '').trim()
    const hasName = normalizedName.length > 0

    const baseSource = hasName ? normalizedName : normalizedPrefix || 'Project'
    const base = truncateName(baseSource, maxLength)

    const suffix = normalizedPrefix
    if (!suffix) {
        return base
    }

    if (!hasName) {
        return base
    }

    return `${base} (${suffix})`
}
