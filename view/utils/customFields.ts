/**
 * Helpers for one custom field across many tasks (DEV-394). A task update's
 * `custom_fields` replaces the whole map, so a bulk set merges the one field
 * into each task's own map.
 */

function keyOf(fields: Record<string, unknown>, name: string): string | undefined {
    const wanted = name.trim().toLowerCase()
    return Object.keys(fields).find((key) => key.trim().toLowerCase() === wanted)
}

/**
 * The task's values of a field: a list gives each item, an empty value none.
 * Keys match regardless of case, as the server matches them.
 */
export function customFieldValues(fields: Record<string, unknown> | null | undefined, name: string): string[] {
    if (!fields) return []
    const key = keyOf(fields, name)
    const value = key === undefined ? undefined : fields[key]
    const items = Array.isArray(value) ? value : [value]
    return items
        .filter((item) => item !== null && item !== undefined && typeof item !== 'object')
        .map((item) => String(item).trim())
        .filter(Boolean)
}

/**
 * The map with `name` set to `value`, or without it when `value` is empty;
 * other fields stay as they are. Returns null when nothing would change.
 */
export function withCustomField(
    fields: Record<string, unknown> | null | undefined,
    name: string,
    value: string,
): Record<string, unknown> | null {
    const current = { ...fields }
    const key = keyOf(current, name)
    const next = value.trim()
    if (key !== undefined && customFieldValues(current, key).join(', ') === next) return null
    if (!next && key === undefined) return null
    const kept = Object.fromEntries(Object.entries(current).filter(([existing]) => existing !== key))
    return next ? { ...kept, [name]: next } : kept
}
