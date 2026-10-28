import { reactive } from 'vue'
import type { TagProperties } from '../api/types'

/**
 * Per-project tag properties (DEV-20). Filled from the resolved-config
 * responses the completion-policy cache already fetches per project, so tag
 * colours follow the same fetch, refresh and invalidation without a second
 * request per project.
 */
const tagPropertiesByProject = reactive(new Map<string, Record<string, TagProperties>>())

/** Tag properties of a project prefix, once its config has loaded. */
export function tagPropertiesFor(prefix: string | null | undefined): Record<string, TagProperties> | undefined {
    return prefix ? tagPropertiesByProject.get(prefix.trim()) : undefined
}

/** Store the `tag_properties` of a resolved-config response for `prefix`. */
export function recordTagProperties(prefix: string, config: unknown): void {
    const raw = (config as Record<string, unknown> | null | undefined)?.tag_properties
    const properties =
        raw && typeof raw === 'object' && !Array.isArray(raw) ? (raw as Record<string, TagProperties>) : {}
    tagPropertiesByProject.set(prefix, properties)
}
