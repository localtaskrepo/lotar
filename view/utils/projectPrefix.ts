import type { ProjectDTO } from '../api/types'

const MAX_PREFIX_LENGTH = 20
const MAX_NAME_LENGTH = 100

function stripLeadingDots(value: string): string {
    return value.replace(/^\.+/, '')
}

export function generateProjectPrefix(name: string): string {
    const cleaned = stripLeadingDots(name ?? '').trim()
    if (!cleaned) {
        return ''
    }
    if (cleaned.length <= 4) {
        return cleaned.toUpperCase()
    }
    const normalized = cleaned.toUpperCase()
    if (/[\-_.\s]/.test(normalized)) {
        return normalized
            .split(/[-_.\s]+/)
            .filter((segment) => segment.length > 0)
            .map((segment) => segment.charAt(0))
            .join('')
            .slice(0, 4)
    }
    return normalized.slice(0, 4)
}

export function normalizePrefixInput(prefix: string): string {
    const upper = (prefix ?? '').toUpperCase()
    const cleaned = upper.replace(/[^A-Z0-9_-]/g, '')
    return cleaned.slice(0, MAX_PREFIX_LENGTH)
}

function prefixConflicts(prefix: string, projects: ProjectDTO[]): boolean {
    const upper = prefix.toUpperCase()
    if (!upper) return true
    return projects.some((project) => {
        const nameUpper = (project.name ?? '').toUpperCase()
        const prefixUpper = (project.prefix ?? '').toUpperCase()
        return prefixUpper === upper || nameUpper === upper
    })
}

export function suggestUniquePrefix(name: string, projects: ProjectDTO[]): string {
    const base = normalizePrefixInput(generateProjectPrefix(name))
    if (!base) {
        return ''
    }
    if (!prefixConflicts(base, projects)) {
        return base
    }
    let attempt = 1
    while (attempt < 1000) {
        attempt += 1
        const suffix = attempt.toString()
        const baseSlice = base.slice(0, Math.max(0, MAX_PREFIX_LENGTH - suffix.length))
        const candidate = `${baseSlice}${suffix}`
        const normalized = normalizePrefixInput(candidate)
        if (!prefixConflicts(normalized, projects)) {
            return normalized
        }
    }
    return base
}

export function validateProjectName(name: string, projects: ProjectDTO[]): string | null {
    const trimmed = name.trim()
    if (!trimmed) {
        return 'Project name is required.'
    }
    if (trimmed.length > MAX_NAME_LENGTH) {
        return `Project name cannot exceed ${MAX_NAME_LENGTH} characters.`
    }
    const upper = trimmed.toUpperCase()
    if (projects.some((project) => project.name.trim().toUpperCase() === upper)) {
        return 'A project with this name already exists.'
    }
    if (projects.some((project) => project.prefix.trim().toUpperCase() === upper)) {
        return 'Project name cannot match an existing project prefix.'
    }
    return null
}

export function validateProjectPrefix(prefix: string): string | null {
    const normalized = prefix.trim()
    if (!normalized) {
        return 'Project prefix is required.'
    }
    if (normalized.length > MAX_PREFIX_LENGTH) {
        return `Project prefix cannot exceed ${MAX_PREFIX_LENGTH} characters.`
    }
    if (!/^[A-Z0-9_-]+$/.test(normalized)) {
        return 'Use only letters, numbers, hyphens, or underscores for the prefix.'
    }
    return null
}

export function detectPrefixConflict(prefix: string, projects: ProjectDTO[]): string | null {
    const upper = prefix.trim().toUpperCase()
    if (!upper) {
        return 'Project prefix is required.'
    }
    for (const project of projects) {
        if (project.prefix.trim().toUpperCase() === upper) {
            return `Prefix ${upper} is already used by ${project.name || project.prefix}.`
        }
        if (project.name.trim().toUpperCase() === upper) {
            return `Prefix ${upper} conflicts with the existing project name ${project.name}.`
        }
    }
    return null
}
