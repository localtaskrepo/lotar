import type { TaskDTO } from '../api/types'
import type { ColKey } from '../composables/useColumns'
import { projectOf } from './text'

/**
 * Task sort contract shared with the backend query executor (DEV-57).
 *
 * `sort_by` accepts the CLI sort fields plus `custom:<name>` (the CLI spells
 * the custom form `field:<name>`; both prefixes are accepted here). `order`
 * is `asc` or `desc` and defaults to `desc` with `sort_by=modified`. Ties
 * always break by canonical task ID lexical ASC, independent of `order`.
 *
 * The comparator mirrors the backend field-by-field, including where missing
 * values land (effort and due dates sort after present values in `asc`;
 * missing assignees and reporters sort first; missing tags/sprints compare
 * as empty vectors, which sort first in `asc`).
 *
 * Datetime parity: RFC3339 timestamps are compared as UTC instants parsed to
 * nanosecond precision (JS `Date` only keeps milliseconds, which would
 * misorder `.123456789` vs `.123457` fractions). Values without a valid
 * RFC3339 offset fall back to raw lexical order exactly like the backend's
 * `compare_parsed_instant`. Known cross-timezone edge: the backend resolves
 * naive datetimes and date-only values against the SERVER's local timezone;
 * a browser cannot observe that timezone, so this comparator approximates
 * them as UTC. The tasks page therefore does not rely on this comparator for
 * pagination — it renders the authoritative server response order (see
 * `useTaskStore`'s hydration ledger) — the comparator only backs standalone
 * tables and grouping views.
 */

export type TaskSortOrder = 'asc' | 'desc'
export type TaskSortBuiltin =
    | 'priority'
    | 'status'
    | 'effort'
    | 'due'
    | 'created'
    | 'modified'
    | 'assignee'
    | 'reporter'
    | 'type'
    | 'project'
    | 'id'
    | 'title'
    | 'tags'
    | 'sprints'
export type TaskSortBy = TaskSortBuiltin | `custom:${string}`

export const DEFAULT_TASK_SORT_BY: TaskSortBy = 'modified'
export const DEFAULT_TASK_SORT_ORDER: TaskSortOrder = 'desc'

const TASK_SORT_BUILTINS = new Set<string>([
    'priority',
    'status',
    'effort',
    'due',
    'created',
    'modified',
    'assignee',
    'reporter',
    'type',
    'project',
    'id',
    'title',
    'tags',
    'sprints',
])

const BUILTIN_ALIASES: Record<string, TaskSortBuiltin> = {
    'due-date': 'due',
}

/** Normalize a `sort_by` wire value; `null` when absent or invalid. */
export function normalizeSortBy(value: unknown): TaskSortBy | null {
    if (typeof value !== 'string') return null
    const trimmed = value.trim()
    if (!trimmed) return null
    const prefix = /^(?:custom|field):/i.exec(trimmed)
    if (prefix) {
        const name = trimmed.slice(prefix[0].length).trim()
        return name ? (`custom:${name}` as TaskSortBy) : null
    }
    const lower = trimmed.toLowerCase()
    const aliased = BUILTIN_ALIASES[lower]
    if (aliased) return aliased
    return TASK_SORT_BUILTINS.has(lower) ? (lower as TaskSortBuiltin) : null
}

export function normalizeSortOrder(value: unknown): TaskSortOrder {
    return value === 'asc' ? 'asc' : 'desc'
}

const COL_KEY_TO_SORT_BY: Record<string, TaskSortBy> = {
    id: 'id',
    title: 'title',
    status: 'status',
    priority: 'priority',
    task_type: 'type',
    assignee: 'assignee',
    reporter: 'reporter',
    effort: 'effort',
    tags: 'tags',
    sprints: 'sprints',
    due_date: 'due',
    created: 'created',
    modified: 'modified',
}

/**
 * Map a table column key onto its server sort key. Every builtin table column
 * has a server-side sort; `custom:<name>` passes through. Unknown columns
 * return null.
 */
export function colKeyToSortBy(col: string): TaskSortBy | null {
    if (col.startsWith('custom:')) {
        const name = col.slice('custom:'.length).trim()
        return name ? (`custom:${name}` as TaskSortBy) : null
    }
    return COL_KEY_TO_SORT_BY[col] ?? null
}

/** Reverse of {@link colKeyToSortBy}; null when no table column represents the key. */
export function sortByToColKey(sortBy: TaskSortBy): ColKey | null {
    if (sortBy.startsWith('custom:')) return sortBy as ColKey
    for (const [col, key] of Object.entries(COL_KEY_TO_SORT_BY)) {
        if (key === sortBy) return col as ColKey
    }
    return null
}

// ---------------------------------------------------------------------------
// Effort parsing (parity with src/utils/effort.rs)
// ---------------------------------------------------------------------------

export interface EffortParsed {
    kind: 'hours' | 'points'
    value: number
    canonical: string
}

const POINTS_SUFFIXES = ['points', 'point', 'pts', 'pt', 'p']
const TIME_WORD_FACTORS: Array<[string, number]> = [
    ['minutes', 1 / 60],
    ['minute', 1 / 60],
    ['mins', 1 / 60],
    ['min', 1 / 60],
    ['hours', 1],
    ['hour', 1],
    ['hrs', 1],
    ['hr', 1],
    ['days', 8],
    ['day', 8],
    ['weeks', 40],
    ['week', 40],
    ['wks', 40],
    ['wk', 40],
]

/** Rust `str::parse::<f64>()` approximation; null when the token is not a plain finite number. */
function tryNumber(token: string): number | null {
    if (!token.trim()) return null
    const n = Number(token)
    if (!Number.isFinite(n)) return null
    return n
}

function parseEffortToken(raw: string): { kind: 'hours' | 'points'; value: number } | null {
    const t = raw.trim().toLowerCase()
    if (!t) return null
    for (const suffix of POINTS_SUFFIXES) {
        if (t.endsWith(suffix)) {
            const n = tryNumber(t.slice(0, t.length - suffix.length))
            if (n === null || n < 0) return null
            return { kind: 'points', value: n }
        }
    }
    for (const [suffix, factor] of TIME_WORD_FACTORS) {
        if (t.endsWith(suffix)) {
            const n = tryNumber(t.slice(0, t.length - suffix.length))
            if (n === null || n < 0) return null
            return { kind: 'hours', value: n * factor }
        }
    }
    const last = t.charAt(t.length - 1)
    if (['m', 'h', 'd', 'w'].includes(last)) {
        const n = tryNumber(t.slice(0, t.length - 1))
        if (n !== null && n >= 0) {
            const factor = last === 'm' ? 1 / 60 : last === 'h' ? 1 : last === 'd' ? 8 : 40
            return { kind: 'hours', value: n * factor }
        }
        return null
    }
    const plain = tryNumber(t)
    if (plain !== null && plain >= 0) return { kind: 'points', value: plain }
    return null
}

function timeWordUnitFactor(unit: string): number | null {
    for (const [word, factor] of TIME_WORD_FACTORS) {
        if (word === unit) return factor
    }
    return unit === 'm' ? 1 / 60 : unit === 'h' ? 1 : unit === 'd' ? 8 : unit === 'w' ? 40 : null
}

function isPointsUnitWord(unit: string): boolean {
    return ['p', 'pt', 'pts', 'point', 'points'].includes(unit)
}

function trimFloat(n: number): string {
    return Number.isInteger(n) ? String(n) : n.toFixed(2)
}

function finishEffort(kind: 'hours' | 'points', value: number): EffortParsed {
    return kind === 'hours'
        ? { kind, value, canonical: `${value.toFixed(2)}h` }
        : { kind, value, canonical: `${trimFloat(value)}pt` }
}

/** Parse an effort string the way the backend does; null when unparseable. */
export function parseEffort(input: string | null | undefined): EffortParsed | null {
    const s = (input ?? '').trim()
    if (!s) return null
    const parts = s.split(/\s+/)
    if (parts.length === 1) {
        const parsed = parseEffortToken(parts[0]!)
        return parsed ? finishEffort(parsed.kind, parsed.value) : null
    }
    let totalHours = 0
    let totalPoints = 0
    let i = 0
    while (i < parts.length) {
        const tok = parts[i]!.trim()
        if (!tok) {
            i += 1
            continue
        }
        const parsed = parseEffortToken(tok)
        if (parsed) {
            if (parsed.kind === 'hours') {
                totalHours += parsed.value
                i += 1
                continue
            }
            const bare = tryNumber(tok)
            if (bare !== null && i + 1 < parts.length) {
                const unit = parts[i + 1]!.trim().toLowerCase()
                const factor = timeWordUnitFactor(unit)
                if (factor !== null) {
                    if (parsed.value < 0) return null
                    totalHours += parsed.value * factor
                    i += 2
                    continue
                }
                if (isPointsUnitWord(unit)) {
                    if (parsed.value < 0) return null
                    totalPoints += parsed.value
                    i += 2
                    continue
                }
            }
            totalPoints += parsed.value
            i += 1
            continue
        }
        const n = tryNumber(tok)
        if (n !== null && i + 1 < parts.length) {
            const unit = parts[i + 1]!.trim().toLowerCase()
            const factor = timeWordUnitFactor(unit)
            if (factor !== null) {
                if (n < 0) return null
                totalHours += n * factor
                i += 2
                continue
            }
            if (isPointsUnitWord(unit)) {
                if (n < 0) return null
                totalPoints += n
                i += 2
                continue
            }
        }
        return null
    }
    if (totalHours > 0 && totalPoints > 0) return null
    if (totalPoints > 0) return finishEffort('points', totalPoints)
    return finishEffort('hours', totalHours)
}

// ---------------------------------------------------------------------------
// Field comparators (parity with the backend sort executor)
// ---------------------------------------------------------------------------

/** Code-unit string order, matching Rust `str::cmp` byte order for the wire data. */
function cmpStr(a: string, b: string): number {
    return a < b ? -1 : a > b ? 1 : 0
}

// ---------------------------------------------------------------------------
// RFC3339 instants at nanosecond precision (JS Date only keeps milliseconds)
// ---------------------------------------------------------------------------

export interface ParsedInstant {
    /** Whole seconds since the Unix epoch. */
    secs: bigint
    /** Sub-second nanoseconds [0, 1e9). */
    nanos: bigint
}

const RFC3339_RE =
    /^(\d{4})-(\d{2})-(\d{2})[Tt ](\d{2}):(\d{2}):(\d{2})(?:\.(\d+))?([Zz]|[+-]\d{2}:?\d{2})$/

/** Days from 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm). */
function daysFromCivil(year: number, month: number, day: number): bigint {
    const y = BigInt(month <= 2 ? year - 1 : year)
    const m = BigInt(month)
    const d = BigInt(day)
    const era = (y >= 0n ? y : y - 99n) / 100n
    const yoe = y - era * 100n // [0, 99]
    const mp = (m + 9n) % 12n // [0, 11]
    const doy = (153n * mp + 2n) / 5n + d - 1n // [0, 365]
    const doe = yoe * 1461n + yoe / 4n - yoe / 100n + doy // [0, 146096]
    return era * 146097n + doe - 719468n
}

/**
 * Parse an RFC3339 timestamp (offset REQUIRED) into a nanosecond instant.
 * Returns null for naive values and anything malformed — mirroring the
 * backend, which only accepts offset-carrying RFC3339 for instant compares.
 */
export function parseRfc3339Instant(raw: string | null | undefined): ParsedInstant | null {
    if (typeof raw !== 'string') return null
    const match = RFC3339_RE.exec(raw.trim())
    if (!match) return null
    const [, ys, ms, ds, hs, mins, sss, frac, offset] = match
    const year = Number(ys)
    const month = Number(ms)
    const day = Number(ds)
    const hour = Number(hs)
    const minute = Number(mins)
    const second = Number(sss)
    if (month < 1 || month > 12 || day < 1 || day > 31) return null
    if (hour > 23 || minute > 59 || second > 60) return null
    let offsetSeconds = 0
    if (offset !== undefined && offset.toUpperCase() !== 'Z') {
        const sign = offset.startsWith('-') ? -1 : 1
        const body = offset.slice(1).replace(':', '')
        const oh = Number(body.slice(0, 2))
        const om = Number(body.slice(2, 4))
        if (oh > 23 || om > 59) return null
        offsetSeconds = sign * (oh * 3600 + om * 60)
    }
    const days = daysFromCivil(year, month, day)
    const secs = days * 86400n + BigInt(hour * 3600 + minute * 60 + second) - BigInt(offsetSeconds)
    const nanos = frac ? BigInt(frac.padEnd(9, '0').slice(0, 9)) : 0n
    return { secs, nanos }
}

function cmpInstant(a: ParsedInstant, b: ParsedInstant): number {
    if (a.secs !== b.secs) return a.secs < b.secs ? -1 : 1
    if (a.nanos !== b.nanos) return a.nanos < b.nanos ? -1 : 1
    return 0
}

/**
 * Backend `compare_parsed_instant` parity: both parseable → instant order;
 * only one parseable → the parseable one first; neither → raw lexical order.
 */
function cmpParsedInstant(a: string | null | undefined, b: string | null | undefined): number {
    const pa = parseRfc3339Instant(a)
    const pb = parseRfc3339Instant(b)
    if (pa && pb) return cmpInstant(pa, pb)
    if (pa) return -1
    if (pb) return 1
    return cmpStr(a ?? '', b ?? '')
}

/** Rust `Option<String>` ordering: missing sorts before any present value. */
function cmpOptionalStr(
    a: string | null | undefined,
    b: string | null | undefined,
): number {
    const aMissing = a === null || a === undefined
    const bMissing = b === null || b === undefined
    if (aMissing && bMissing) return 0
    if (aMissing) return -1
    if (bMissing) return 1
    return cmpStr(a as string, b as string)
}

/** Explicit missing-last ordering used for due dates, as in the backend sort. */
function cmpOptionalStrMissingLast(
    a: string | null | undefined,
    b: string | null | undefined,
): number {
    const aMissing = a === null || a === undefined
    const bMissing = b === null || b === undefined
    if (aMissing && bMissing) return 0
    if (aMissing) return 1
    if (bMissing) return -1
    return cmpDueValues(a as string, b as string)
}

/**
 * Due-date ordering. RFC3339 values compare as UTC instants (nanosecond
 * precision); date-only values compare lexically among themselves (equal to
 * chronological order) and against instants by treating the date as UTC
 * midnight. The backend resolves naive/date-only values in the SERVER's
 * timezone; a browser cannot observe it, so mixed date-only + offset-carrying
 * pairs can order differently here than server-side around midnight
 * boundaries. The tasks page avoids this by ordering from the authoritative
 * server response; this comparator serves standalone tables only.
 */
function cmpDueValues(a: string, b: string): number {
    const ia = parseRfc3339Instant(a)
    const ib = parseRfc3339Instant(b)
    if (ia && ib) return cmpInstant(ia, ib)
    const da = /^\d{4}-\d{2}-\d{2}$/.test(a.trim()) ? `${a.trim()}T00:00:00Z` : null
    const db = /^\d{4}-\d{2}-\d{2}$/.test(b.trim()) ? `${b.trim()}T00:00:00Z` : null
    if (da !== null || db !== null) {
        const pa = ia ?? (da !== null ? parseRfc3339Instant(da) : null)
        const pb = ib ?? (db !== null ? parseRfc3339Instant(db) : null)
        if (pa && pb) return cmpInstant(pa, pb)
    }
    return cmpStr(a, b)
}

/** Missing effort values sort after present ones (asc), as in the backend. */
function cmpEffort(a: string | null | undefined, b: string | null | undefined): number {
    const pa = a === null || a === undefined ? null : parseEffort(a)
    const pb = b === null || b === undefined ? null : parseEffort(b)
    if (pa && pb) {
        if (pa.kind === pb.kind) {
            return pa.value < pb.value ? -1 : pa.value > pb.value ? 1 : 0
        }
        return cmpStr(pa.canonical, pb.canonical)
    }
    if (pa) return -1
    if (pb) return 1
    return 0
}

/** Mirrors `types::custom_value_to_string` for comparable display strings. */
export function customValueToString(value: unknown): string {
    if (value === null || value === undefined) return ''
    if (typeof value === 'boolean') return value ? 'true' : 'false'
    if (typeof value === 'number') return String(value)
    if (typeof value === 'string') return value
    if (Array.isArray(value)) return '[array]'
    if (typeof value === 'object') return '{object}'
    return 'other'
}

/**
 * Exact-name lookup with case-insensitive fallback. Returns null when the
 * field is absent so callers can mirror the backend's `Option<String>`
 * ordering (missing sorts before any present value, including empty strings).
 */
export function customFieldValue(fields: Record<string, unknown> | null | undefined, name: string): string | null {
    const custom = fields ?? {}
    const exact = Object.prototype.hasOwnProperty.call(custom, name) ? custom[name] : undefined
    if (exact !== undefined) return customValueToString(exact)
    const lower = name.toLowerCase()
    for (const [key, value] of Object.entries(custom)) {
        if (key.toLowerCase() === lower) return customValueToString(value)
    }
    return null
}

/** Rust `Option<String>` ordering for custom values: missing < present. */
function cmpCustomField(
    a: Record<string, unknown> | null | undefined,
    b: Record<string, unknown> | null | undefined,
    name: string,
): number {
    const av = customFieldValue(a, name)
    const bv = customFieldValue(b, name)
    if (av === null && bv === null) return 0
    if (av === null) return -1
    if (bv === null) return 1
    return cmpStr(av, bv)
}

/** Rust `Vec<T>` Ord parity: element-wise, then shorter-prefix first. */
function cmpVec<T>(a: readonly T[], b: readonly T[], cmp: (x: T, y: T) => number): number {
    const len = Math.min(a.length, b.length)
    for (let i = 0; i < len; i += 1) {
        const ordering = cmp(a[i]!, b[i]!)
        if (ordering !== 0) return ordering
    }
    return a.length - b.length
}

/** Primary-key comparison for a sort_by; ties are handled by {@link sortTasks}. */
export function compareTasks(a: TaskDTO, b: TaskDTO, sortBy: TaskSortBy): number {
    switch (sortBy) {
        case 'priority':
            return cmpStr(a.priority ?? '', b.priority ?? '')
        case 'status':
            return cmpStr(a.status ?? '', b.status ?? '')
        case 'effort':
            return cmpEffort(a.effort, b.effort)
        case 'due':
            return cmpOptionalStrMissingLast(a.due_date, b.due_date)
        case 'created':
            return cmpParsedInstant(a.created, b.created)
        case 'modified':
            return cmpParsedInstant(a.modified, b.modified)
        case 'assignee':
            return cmpOptionalStr(a.assignee, b.assignee)
        case 'reporter':
            return cmpOptionalStr(a.reporter, b.reporter)
        case 'type':
            return cmpStr(a.task_type ?? '', b.task_type ?? '')
        case 'project':
            return cmpStr(projectOf(a.id), projectOf(b.id))
        case 'id':
            return cmpStr(a.id, b.id)
        case 'title':
            return cmpStr(a.title ?? '', b.title ?? '')
        case 'tags':
            return cmpVec(a.tags ?? [], b.tags ?? [], cmpStr)
        case 'sprints':
            return cmpVec(a.sprints ?? [], b.sprints ?? [], (x, y) => x - y)
        default: {
            const name = sortBy.slice('custom:'.length)
            return cmpCustomField(a.custom_fields, b.custom_fields, name)
        }
    }
}

/**
 * Deterministic global sort: primary key in `order` direction, ties broken by
 * canonical task ID lexical ASC regardless of direction. Returns a new array.
 */
export function sortTasks<T extends TaskDTO>(tasks: readonly T[], sortBy: TaskSortBy, order: TaskSortOrder): T[] {
    const dir = order === 'asc' ? 1 : -1
    const arr = [...tasks]
    arr.sort((a, b) => {
        const primary = compareTasks(a, b, sortBy) * dir
        if (primary !== 0) return primary
        return cmpStr(a.id, b.id)
    })
    return arr
}
