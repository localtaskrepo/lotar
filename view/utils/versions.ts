/**
 * Version values for `version` custom fields (DEV-373). Mirrors what
 * `Version::parse` in src/utils/version.rs accepts, so the task panel can
 * flag a value the server would reject: `MAJOR[.MINOR[.PATCH]][-PRE][+BUILD]`
 * with an optional leading `v`; `0.13.x` and `0.13.*` are the partial `0.13`.
 */
const PRE_IDENT = /^[0-9A-Za-z-]+$/
const U64_MAX = 2n ** 64n - 1n

/** One prerelease identifier: numeric ones sort before alphanumeric ones. */
type Ident = bigint | string

export interface Version {
    nums: [bigint, bigint, bigint]
    /** How many numbers were written: fewer than 3 (with no prerelease) makes a partial version. */
    given: number
    pre: Ident[] | null
    /** The value as stored: trimmed, without a leading `v`. */
    text: string
}

/** A `u64` from digits, as Rust's `parse::<u64>()` reads it; null when it overflows. */
function u64(digits: string): bigint | null {
    const value = BigInt(digits)
    return value > U64_MAX ? null : value
}

/** Parse a version value the way `Version::parse` does; null when the server would reject it. */
export function parseVersion(raw: string): Version | null {
    const trimmed = raw.trim()
    const text = /^[vV]/.test(trimmed) ? trimmed.slice(1) : trimmed
    if (!text) return null
    const core = text.split('+', 1)[0] ?? ''
    const dash = core.indexOf('-')
    const numbers = dash === -1 ? core : core.slice(0, dash)
    const preText = dash === -1 ? null : core.slice(dash + 1)
    const parts = numbers.split('.')
    while (parts.length > 1 && preText === null && ['x', 'X', '*'].includes(parts[parts.length - 1] ?? '')) parts.pop()
    if (parts.length > 3 || !parts.every((part) => /^\d+$/.test(part))) return null
    const nums: [bigint, bigint, bigint] = [0n, 0n, 0n]
    for (const [index, part] of parts.entries()) {
        const value = u64(part)
        if (value === null) return null
        nums[index] = value
    }
    let pre: Ident[] | null = null
    if (preText !== null) {
        pre = []
        for (const ident of preText.split('.')) {
            if (!PRE_IDENT.test(ident)) return null
            const value = /^\d+$/.test(ident) ? u64(ident) : ident
            if (value === null) return null
            pre.push(value)
        }
    }
    return { nums, given: parts.length, pre, text }
}

export function isVersion(raw: string): boolean {
    return parseVersion(raw) !== null
}

function cmpIdent(a: Ident, b: Ident): number {
    if (typeof a === 'bigint' && typeof b === 'bigint') return a < b ? -1 : a > b ? 1 : 0
    if (typeof a === 'bigint') return -1
    if (typeof b === 'bigint') return 1
    return a < b ? -1 : a > b ? 1 : 0
}

/**
 * The prerelease rank of the first version a value stands for: a partial
 * version starts below every prerelease of its numbers, a release sorts
 * after them.
 */
function preRank(version: Version): number {
    if (version.pre !== null) return 1
    return version.given < 3 ? 0 : 2
}

/**
 * `Version::sort_cmp`: by the first version a value stands for, so `0.13`
 * sorts just before `0.13.0`, then by the stored text.
 */
export function compareVersions(a: Version, b: Version): number {
    for (const [index, num] of a.nums.entries()) {
        const other = b.nums[index] ?? 0n
        if (num !== other) return num < other ? -1 : 1
    }
    const byRank = preRank(a) - preRank(b)
    if (byRank) return byRank
    if (a.pre && b.pre) {
        for (const [index, ident] of a.pre.slice(0, b.pre.length).entries()) {
            const ordering = cmpIdent(ident, b.pre[index] ?? '')
            if (ordering) return ordering
        }
        if (a.pre.length !== b.pre.length) return a.pre.length - b.pre.length
    }
    return a.text < b.text ? -1 : a.text > b.text ? 1 : 0
}

/**
 * The versions a custom field value holds, like `value_versions`: one for a
 * scalar, one per item for a list; unreadable items are skipped.
 */
export function valueVersions(value: unknown): Version[] {
    if (Array.isArray(value)) return value.flatMap(valueVersions)
    if (typeof value !== 'string' && typeof value !== 'number') return []
    const version = parseVersion(String(value))
    return version ? [version] : []
}

/** The lowest version a value holds, as the server sorts a version field; null when it holds none. */
export function lowestVersion(value: unknown): Version | null {
    return valueVersions(value).reduce<Version | null>(
        (low, version) => (low === null || compareVersions(version, low) < 0 ? version : low),
        null,
    )
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

/** Orders text as versions, as the Board's version lanes need; text that is not a version comes last. */
export function compareVersionText(a: string, b: string): number {
    const va = parseVersion(a)
    const vb = parseVersion(b)
    if (va && vb) return compareVersions(va, vb)
    return va ? -1 : vb ? 1 : a.localeCompare(b)
}
