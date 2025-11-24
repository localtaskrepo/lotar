import type { ConfigSource } from '../api/types'

export function provenanceLabel(source: ConfigSource | undefined): string {
    if (!source) return ''
    switch (source) {
        case 'project':
            return 'Project'
        case 'global':
            return 'Global'
        case 'built_in':
            return 'Built-in'
        default:
            return ''
    }
}

export function provenanceClass(source: ConfigSource | undefined): string {
    return source ? `source-${source}` : ''
}
