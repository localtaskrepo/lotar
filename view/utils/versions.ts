/**
 * Version values for `version` custom fields (DEV-373). Mirrors what
 * `Version::parse` in src/utils/version.rs accepts, so the task panel can
 * flag a value the server would reject: `MAJOR[.MINOR[.PATCH]][-PRE][+BUILD]`
 * with an optional leading `v`; `0.13.x` and `0.13.*` are the partial `0.13`.
 */
const PRE_IDENT = /^[0-9A-Za-z-]+$/

export function isVersion(raw: string): boolean {
    const trimmed = raw.trim()
    const text = /^[vV]/.test(trimmed) ? trimmed.slice(1) : trimmed
    if (!text) return false
    const core = text.split('+', 1)[0] ?? ''
    const dash = core.indexOf('-')
    const numbers = dash === -1 ? core : core.slice(0, dash)
    const pre = dash === -1 ? null : core.slice(dash + 1)
    const parts = numbers.split('.')
    while (parts.length > 1 && pre === null && ['x', 'X', '*'].includes(parts[parts.length - 1] ?? '')) parts.pop()
    if (parts.length > 3 || !parts.every((part) => /^\d+$/.test(part))) return false
    return pre === null || pre.split('.').every((ident) => PRE_IDENT.test(ident))
}

/**
 * The message for a version field value that does not read as versions:
 * one version or a comma-separated list; empty is fine (no value).
 */
export function versionValueError(value: string): string | null {
    const items = value
        .split(',')
        .map((item) => item.trim())
        .filter(Boolean)
    const bad = items.find((item) => !isVersion(item))
    return bad === undefined ? null : `"${bad}" is not a version: use numbers like 1.4.2, 0.13 or 1.0.0-rc.1`
}

/** Names of the custom fields that hold versions, from a config's `custom_field_properties`. */
export function versionFieldNames(properties: unknown): string[] {
    if (!properties || typeof properties !== 'object') return []
    return Object.entries(properties as Record<string, unknown>)
        .filter(([, props]) => !!props && typeof props === 'object' && (props as { type?: unknown }).type === 'version')
        .map(([name]) => name)
}
