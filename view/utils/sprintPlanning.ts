import type { TaskDTO } from '../api/types'
import { fromDateTimeInputValue, safeTimestamp, toDateTimeInputValue } from './date'

// -- Form values -------------------------------------------------------------------

/** Any value as a string ('' for null/undefined or unprintable values). */
export function toStringValue(value: unknown): string {
    if (typeof value === 'string') return value
    if (value === null || value === undefined) return ''
    try {
        return String(value)
    } catch {
        return ''
    }
}

export function toTrimmedString(value: unknown): string {
    return toStringValue(value).trim()
}

/** A finite number from a form input, or null when blank or invalid. */
export function parseNumberInput(value: unknown): number | null {
    const trimmed = toTrimmedString(value)
    if (!trimmed) return null
    const parsed = Number(trimmed)
    return Number.isFinite(parsed) ? parsed : null
}

// -- Durations ("2w", "3d 4h", "90 minutes") ------------------------------------------

const DURATION_UNIT_MS: Record<string, number> = {
    ms: 1,
    millisecond: 1,
    milliseconds: 1,
    s: 1_000,
    sec: 1_000,
    secs: 1_000,
    second: 1_000,
    seconds: 1_000,
    m: 60_000,
    min: 60_000,
    mins: 60_000,
    minute: 60_000,
    minutes: 60_000,
    h: 3_600_000,
    hr: 3_600_000,
    hrs: 3_600_000,
    hour: 3_600_000,
    hours: 3_600_000,
    d: 86_400_000,
    day: 86_400_000,
    days: 86_400_000,
    w: 604_800_000,
    week: 604_800_000,
    weeks: 604_800_000,
}

/** Plan length units, largest first: weeks, days, hours, minutes. */
const PLAN_LENGTH_LABELS = ['w', 'd', 'h', 'm']

const ELAPSED_UNITS = [
    { label: 'd', size: 86_400_000 },
    { label: 'h', size: 3_600_000 },
    { label: 'm', size: 60_000 },
]

const DAY_MS = 86_400_000

/**
 * A duration split into calendar days (weeks and days, possibly fractional)
 * and clock time (hours and smaller), so it can be added to a local date
 * without daylight-saving changes shifting the time of day.
 */
function parseDurationParts(value: string | null | undefined): { days: number; ms: number } | null {
    if (!value) return null
    const text = value.trim().toLowerCase()
    if (!text) return null
    const pattern = /([0-9]*\.?[0-9]+)\s*([a-z]+)/g
    let days = 0
    let ms = 0
    let matched = false
    let match: RegExpExecArray | null
    while ((match = pattern.exec(text)) !== null) {
        matched = true
        const amount = Number.parseFloat(match[1] ?? '')
        if (!Number.isFinite(amount)) continue
        const unitRaw = match[2] ?? ''
        const unitMs = DURATION_UNIT_MS[unitRaw] ?? DURATION_UNIT_MS[unitRaw.replace(/s$/, '')]
        if (!unitMs) continue
        if (unitMs >= DAY_MS) days += (amount * unitMs) / DAY_MS
        else ms += amount * unitMs
    }
    if (!matched || days * DAY_MS + ms <= 0) return null
    return { days, ms }
}

/** Total milliseconds of a duration like "2w 3d"; null when nothing positive parses. */
export function parseDurationToMs(value: string | null | undefined): number | null {
    const parts = parseDurationParts(value)
    return parts ? parts.days * DAY_MS + parts.ms : null
}

/** `timestamp` moved by `days` local calendar days, keeping the time of day. */
function addLocalDays(timestamp: number, days: number): number {
    const date = new Date(timestamp)
    date.setDate(date.getDate() + days)
    return date.getTime()
}

/**
 * `start` plus a plan length. Weeks and days count calendar days, so "1w"
 * from 09:00 ends at 09:00 even across a daylight-saving change; hours and
 * minutes are elapsed time. Null when the length does not parse.
 */
export function addPlanLength(start: number, value: string | null | undefined): number | null {
    const parts = parseDurationParts(value)
    if (!parts || !Number.isFinite(start)) return null
    const wholeDays = Math.trunc(parts.days)
    return addLocalDays(start, wholeDays) + (parts.days - wholeDays) * DAY_MS + parts.ms
}

/** Plan length text such as "2w 1d 2h": whole local calendar days, then clock time. */
export function planLengthBetween(start: number, end: number): string {
    if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) return ''
    let days = Math.round((end - start) / DAY_MS)
    while (days > 0 && addLocalDays(start, days) > end) days -= 1
    while (addLocalDays(start, days + 1) <= end) days += 1
    return formatPlanLength(days, end - addLocalDays(start, days))
}

/** Days and clock time are formatted separately: on a 25-hour day `ms` can exceed 24h. */
function formatPlanLength(days: number, ms: number): string {
    const amounts = [Math.floor(days / 7), days % 7, Math.floor(ms / 3_600_000), Math.floor((ms % 3_600_000) / 60_000)]
    const parts = PLAN_LENGTH_LABELS.map((label, index) => (amounts[index]! > 0 ? `${amounts[index]}${label}` : ''))
    const text = parts.filter(Boolean).join(' ')
    return text || (days > 0 || ms > 0 ? '1m' : '')
}

/** The two largest units of an elapsed time, e.g. "3d 4h"; "<1m" below a minute. */
export function formatElapsedDuration(ms: number): string {
    if (!Number.isFinite(ms) || ms <= 0) return '<1m'
    const parts: string[] = []
    let remaining = ms
    for (const unit of ELAPSED_UNITS) {
        if (remaining < unit.size) continue
        const value = Math.floor(remaining / unit.size)
        parts.push(`${value}${unit.label}`)
        remaining -= value * unit.size
        if (parts.length === 2) break
    }
    return parts.length ? parts.join(' ') : '<1m'
}

// -- datetime-local inputs ------------------------------------------------------------

export function dateTimeInputToTimestamp(value: string | null | undefined): number | null {
    if (!value) return null
    const normalized = fromDateTimeInputValue(value)
    if (!normalized) return null
    return safeTimestamp(normalized)
}

/** A datetime-local input value for `timestamp`, rounded to the minute. */
export function timestampToDateTimeInput(timestamp: number): string {
    if (!Number.isFinite(timestamp)) return ''
    const rounded = Math.round(timestamp / 60_000) * 60_000
    return toDateTimeInputValue(new Date(rounded).toISOString())
}

// -- Task sprint membership -------------------------------------------------------------

/** Positive, unique sprint ids in ascending order. */
export function normalizeSprintMembership(raw: unknown): number[] {
    if (!Array.isArray(raw)) return []
    const seen = new Set<number>()
    const values: number[] = []
    for (const entry of raw) {
        const parsed = typeof entry === 'number' ? entry : Number(entry)
        if (!Number.isFinite(parsed) || parsed <= 0 || seen.has(parsed)) continue
        seen.add(parsed)
        values.push(parsed)
    }
    values.sort((a, b) => a - b)
    return values
}

/** A task with tags and sprint membership in a predictable shape. */
export function normalizeTaskRecord(task: TaskDTO): TaskDTO {
    const tags = Array.isArray(task.tags) ? [...task.tags] : []
    const sprints = normalizeSprintMembership(task.sprints)
    return { ...task, tags, sprints }
}

export function sprintMembershipMatches(previous: TaskDTO | null | undefined, next: TaskDTO): boolean {
    const prevMembership = normalizeSprintMembership(previous?.sprints)
    const nextMembership = normalizeSprintMembership(next.sprints)
    if (prevMembership.length !== nextMembership.length) return false
    return prevMembership.every((id, index) => id === nextMembership[index])
}

// -- Paging ------------------------------------------------------------------------------

/** Offset of `page` (1-based, clamped to the pages that exist). */
export function pageOffset(total: number, page: number, pageSize: number): number {
    const size = Math.max(1, pageSize)
    const pageCount = Math.max(1, Math.ceil(Math.max(0, total) / size))
    return (Math.max(1, Math.min(page, pageCount)) - 1) * size
}

/** `offset` moved back onto the last page when the list shrank; 0 for an empty list. */
export function clampPageOffset(offset: number, total: number, pageSize: number): number {
    if (total <= 0) return 0
    const size = Math.max(1, pageSize)
    const maxOffset = Math.floor((total - 1) / size) * size
    return Math.min(Math.max(0, offset), maxOffset)
}

/** The page of `list` starting at `offset` (the same array when it all fits). */
export function pageSlice<T>(list: T[], offset: number, pageSize: number): T[] {
    const size = Math.max(1, pageSize)
    if (offset === 0 && list.length <= size) return list
    return list.slice(offset, offset + size)
}
