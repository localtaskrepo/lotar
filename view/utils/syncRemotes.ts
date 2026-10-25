import { parse as parseYaml, stringify as stringifyYaml } from 'yaml'
import type { SyncFieldMapping, SyncProvider, SyncRemoteConfig } from '../api/types'

/** The remote editor's form fields, all as typed text. */
export interface RemoteFormState {
    name: string
    provider: SyncProvider
    project: string
    repo: string
    filter: string
    auth_profile: string
    mapping: string
}

export function hasValue(value?: string | null): boolean {
    return String(value ?? '').trim().length > 0
}

export function remoteProviderIcon(remote: SyncRemoteConfig): 'jira' | 'github' | 'list' {
    if (remote.provider === 'jira') return 'jira'
    if (remote.provider === 'github') return 'github'
    return 'list'
}

/** The Jira project or GitHub repo, falling back to the provider name. */
export function remoteProviderLabel(remote: SyncRemoteConfig): string {
    if (remote.provider === 'jira') return remote.project?.trim() || 'Jira'
    if (remote.provider === 'github') return remote.repo?.trim() || 'GitHub'
    return String(remote.provider)
}

/** A remote with every optional field present, for stable YAML comparisons. */
export function remoteDefFromForm(remote: SyncRemoteConfig): SyncRemoteConfig {
    return {
        provider: remote.provider,
        project: remote.project ?? null,
        repo: remote.repo ?? null,
        filter: remote.filter ?? null,
        auth_profile: remote.auth_profile ?? null,
        mapping: remote.mapping ?? {},
    }
}

// -- Field mappings (edited as YAML) ---------------------------------------------------

export function formatMapping(mapping?: Record<string, SyncFieldMapping>): string {
    if (!mapping || Object.keys(mapping).length === 0) return ''
    return stringifyYaml(mapping).trim()
}

/** The mapping in `value`, `{}` when blank, or null when it is not a YAML mapping. */
export function parseMappingInput(value: string): Record<string, SyncFieldMapping> | null {
    const trimmed = value.trim()
    if (!trimmed) return {}
    try {
        const parsed = parseYaml(trimmed)
        if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null
        return parsed as Record<string, SyncFieldMapping>
    } catch {
        return null
    }
}

const MAPPING_KEYS = new Set(['field', 'values', 'set', 'default', 'add', 'when_empty'])

/** Problems with a parsed mapping, one message per issue. */
export function validateMapping(mapping: Record<string, SyncFieldMapping>): string[] {
    const errors: string[] = []
    for (const [localField, value] of Object.entries(mapping)) {
        if (!localField.trim()) {
            errors.push('Mapping keys cannot be empty.')
            continue
        }
        if (typeof value === 'string') continue
        if (!value || typeof value !== 'object' || Array.isArray(value)) {
            errors.push(`Mapping for ${localField} must be a string or mapping.`)
            continue
        }
        const detail = value as Record<string, unknown>
        for (const key of Object.keys(detail)) {
            if (!MAPPING_KEYS.has(key)) errors.push(`Mapping for ${localField} has unsupported key '${key}'.`)
        }
        if (detail.values) {
            if (typeof detail.values !== 'object' || Array.isArray(detail.values)) {
                errors.push(`Mapping for ${localField} values must be a key/value object.`)
            } else {
                for (const [mapKey, mapValue] of Object.entries(detail.values)) {
                    if (!String(mapKey).trim() || typeof mapValue !== 'string') {
                        errors.push(`Mapping for ${localField} values must map strings to strings.`)
                        break
                    }
                }
            }
        }
        if (detail.add && (!Array.isArray(detail.add) || detail.add.some((item) => typeof item !== 'string'))) {
            errors.push(`Mapping for ${localField} add must be a list of strings.`)
        }
        if (detail.when_empty && detail.when_empty !== 'skip' && detail.when_empty !== 'clear') {
            errors.push(`Mapping for ${localField} when_empty must be 'skip' or 'clear'.`)
        }
    }
    return errors
}

/** The first three errors, plus how many more there are. */
export function formatMappingErrors(errors: string[]): string {
    const preview = errors.slice(0, 3)
    if (errors.length <= preview.length) return preview.join(' ')
    return `${preview.join(' ')} (${errors.length - preview.length} more)`
}

/** Validate the form and build the remote it describes. */
export function buildRemoteConfig(form: RemoteFormState): { config: SyncRemoteConfig | null; errors: string[] } {
    const errors: string[] = []
    const provider = form.provider
    const projectValue = form.project.trim()
    const repoValue = form.repo.trim()
    if (provider === 'jira' && !projectValue) errors.push('Jira project key is required.')
    if (provider === 'github' && !repoValue) errors.push('GitHub repository is required.')

    const mapping = parseMappingInput(form.mapping)
    if (mapping === null) {
        errors.push('Mapping must be valid YAML.')
        return { config: null, errors }
    }
    errors.push(...validateMapping(mapping))
    if (errors.length) return { config: null, errors }

    return {
        config: {
            provider,
            project: provider === 'jira' ? projectValue || null : null,
            repo: provider === 'github' ? repoValue || null : null,
            filter: form.filter.trim() || null,
            auth_profile: form.auth_profile.trim() || null,
            mapping: Object.keys(mapping).length ? mapping : {},
        },
        errors,
    }
}
