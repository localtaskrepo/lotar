/** Capitalize the first character of a string. */
export function titleCase(value: string): string {
    return value ? value.charAt(0).toUpperCase() + value.slice(1) : value
}

/** Longest project prefix accepted in a task ID, in bytes (backend grammar). */
const MAX_TASK_ID_PREFIX_BYTES = 64

/** Largest numeric task suffix supported (u64::MAX); compared exactly via BigInt. */
const MAX_TASK_NUMBER = 18446744073709551615n

const ASCII_DIGITS = /^[0-9]+$/
const UNICODE_ALPHANUMERIC = /[\p{Alphabetic}\p{Number}]/u

/** Canonical task ID parts: exact-case project prefix plus numeric suffix. */
export interface TaskIdParts {
    /** Project prefix; may itself contain dashes (`ABC-OPS` in `ABC-OPS-12`). */
    project: string
    /** Canonical numeric suffix without padding (`TP-001` → `1`), kept as a string so u64 values never lose JS number precision. */
    number: string
}

function isValidProjectPrefix(prefix: string): boolean {
    if (!prefix) return false
    if (new TextEncoder().encode(prefix).length > MAX_TASK_ID_PREFIX_BYTES) return false
    const characters = Array.from(prefix)
    const first = characters[0]
    if (!first || !UNICODE_ALPHANUMERIC.test(first)) return false
    return characters.every((char) => UNICODE_ALPHANUMERIC.test(char) || char === '_' || char === '-')
}

/**
 * Parse a task ID the way the backend does: the FINAL dash-separated segment
 * is the numeric suffix and the rest is the project prefix. Exact case, no
 * trimming, padded spellings collapse to their canonical number. Malformed IDs
 * (empty, no dash, non-ASCII-digit suffix, `+`/junk suffix, invalid or
 * overlong prefix, u64 overflow) return null so callers fail closed instead
 * of deriving a bogus project prefix.
 *
 * Prefix characters follow Rust's `char::is_alphanumeric` exactly: Unicode
 * `Alphabetic` (letters plus Other_Alphabetic marks) or `Number` (Nd/Nl/No),
 * so `\p{Alphabetic}`/`\p{Number}` are used rather than `\p{L}`/`\p{N}`,
 * which would wrongly reject marks the backend accepts. The numeric suffix
 * stays ASCII-digits-only regardless.
 */
export function parseTaskId(id: string | null | undefined): TaskIdParts | null {
    const value = id || ''
    const dash = value.lastIndexOf('-')
    if (dash <= 0 || dash === value.length - 1) return null
    const project = value.slice(0, dash)
    const suffix = value.slice(dash + 1)
    if (!isValidProjectPrefix(project)) return null
    if (!ASCII_DIGITS.test(suffix)) return null
    let number: bigint
    try {
        number = BigInt(suffix)
    } catch {
        return null
    }
    if (number > MAX_TASK_NUMBER) return null
    return { project, number: number.toString() }
}

/** Project prefix of a task ID (`ABC-OPS-12` → `ABC-OPS`); empty when malformed. */
export function projectOf(id: string | null | undefined): string {
    return parseTaskId(id)?.project ?? ''
}

/** Canonical numeric suffix of a task ID (`ABC-OPS-12` → `12`, `TP-001` → `1`); empty when malformed. */
export function numericOf(id: string | null | undefined): string {
    return parseTaskId(id)?.number ?? ''
}

/** Exact-case project prefix for config/sync/attachment requests, or null when the ID is malformed. */
export function projectPrefixOfTaskId(id: string | null | undefined): string | null {
    return parseTaskId(id)?.project ?? null
}
