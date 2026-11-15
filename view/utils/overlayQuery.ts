import type { LocationQuery } from 'vue-router'

/**
 * Query keys of overlays that open over any page: the task panel and the file
 * viewer. Pages that read the whole query as filters skip them.
 */
export const OVERLAY_QUERY_KEYS: ReadonlySet<string> = new Set(['taskPanel', 'file'])

/** The overlay entries of a query, for a page that rewrites its own keys. */
export function overlayQuery(query: LocationQuery): Record<string, string> {
    const kept: Record<string, string> = {}
    for (const key of OVERLAY_QUERY_KEYS) {
        const value = query[key]
        if (typeof value === 'string') kept[key] = value
    }
    return kept
}
