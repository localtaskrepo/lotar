import type { TagProperties } from '../api/types'

/**
 * Tag colours (DEV-20). A tag's colour comes from `issue.tags` objects
 * (`- { name: ui, color: "#0ea5e9" }`) or a project's `issue.tag_properties`;
 * the server merges global and project entries per tag. Values are `#rgb`,
 * `#rrggbb` or one of these names; anything else shows the tag uncoloured.
 */
export const TAG_COLOR_NAMES: Readonly<Record<string, string>> = {
    red: '#ef4444',
    orange: '#f97316',
    amber: '#f59e0b',
    yellow: '#eab308',
    lime: '#84cc16',
    green: '#22c55e',
    teal: '#14b8a6',
    cyan: '#06b6d4',
    sky: '#0ea5e9',
    blue: '#3b82f6',
    indigo: '#6366f1',
    violet: '#8b5cf6',
    purple: '#a855f7',
    pink: '#ec4899',
    rose: '#f43f5e',
    brown: '#a16207',
    gray: '#6b7280',
    grey: '#6b7280',
}

/** A CSS colour for a configured value, or null when it is not understood. */
export function resolveTagColor(value: unknown): string | null {
    if (typeof value !== 'string') return null
    const trimmed = value.trim()
    if (/^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i.test(trimmed)) return trimmed.toLowerCase()
    return TAG_COLOR_NAMES[trimmed.toLowerCase()] ?? null
}

/** The colour of `tag`: an exact entry first, then one differing only in case. */
export function tagColorOf(
    properties: Readonly<Record<string, TagProperties>> | null | undefined,
    tag: string,
): string | null {
    if (!properties || !tag) return null
    const exact = properties[tag]
    if (exact) return resolveTagColor(exact.color)
    const lower = tag.toLowerCase()
    for (const [name, props] of Object.entries(properties)) {
        if (name.toLowerCase() === lower) return resolveTagColor(props?.color)
    }
    return null
}

/** Attributes for a `.tag` chip: a tint class and the colour variable. */
export function tagChipAttrs(color: string | null): { class?: string; style?: Record<string, string> } {
    return color ? { class: 'tag--colored', style: { '--tag-color': color } } : {}
}
