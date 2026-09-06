export const MS_PER_DAY = 24 * 60 * 60 * 1000

/**
 * Whether a task is overdue: has a parseable due date strictly before the
 * start of the local day and is not done. Date-only values count as local
 * midnight, so a task due today is never overdue.
 */
export function isTaskOverdue(task: {
    status?: string | null
    due_date?: string | null
}): boolean {
    const status = (task.status || '').toLowerCase()
    if (!task.due_date || status === 'done') return false
    const due = parseTaskDateToMillis(task.due_date)
    if (due === null) return false
    return due < startOfLocalDay(new Date()).getTime()
}

const DATE_ONLY_REGEX = /^([0-9]{4})-([0-9]{2})-([0-9]{2})$/

export function parseTaskDate(value?: string | null): Date | null {
    if (!value) return null
    const trimmed = value.trim()
    if (!trimmed) return null
    const match = DATE_ONLY_REGEX.exec(trimmed)
    if (match) {
        const year = Number(match[1])
        const month = Number(match[2]) - 1
        const day = Number(match[3])
        const local = new Date(year, month, day)
        if (Number.isNaN(local.getTime())) {
            return null
        }
        local.setHours(0, 0, 0, 0)
        return local
    }
    const parsed = new Date(trimmed)
    if (Number.isNaN(parsed.getTime())) {
        return null
    }
    return parsed
}

export function parseTaskDateToMillis(value?: string | null): number | null {
    const date = parseTaskDate(value)
    return date ? date.getTime() : null
}

export function startOfLocalDay(date: Date): Date {
    return new Date(date.getFullYear(), date.getMonth(), date.getDate())
}

export function toDateKey(date: Date): string {
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}

export function formatTaskDate(value?: string | null, options?: Intl.DateTimeFormatOptions): string {
    const parsed = parseTaskDate(value)
    if (!parsed) return value ?? ''
    return parsed.toLocaleDateString(undefined, options)
}

export function safeTimestamp(value?: string | null): number | null {
    if (!value) return null
    const trimmed = value.trim()
    if (!trimmed) return null
    const parsed = Date.parse(trimmed)
    return Number.isFinite(parsed) ? parsed : null
}

function pad2(value: number): string {
    return String(value).padStart(2, '0')
}

export function toDateInputValue(value?: string | null): string {
    if (!value) return ''
    const parsed = parseTaskDate(value)
    if (!parsed) {
        return value.trim()
    }
    return toDateKey(parsed)
}

export function fromDateInputValue(value: unknown): string | null {
    if (value === null || value === undefined) return null
    const trimmed = typeof value === 'string' ? value.trim() : String(value).trim()
    if (!trimmed) return null
    if (DATE_ONLY_REGEX.test(trimmed)) {
        return trimmed
    }
    const parsed = parseTaskDate(trimmed)
    if (!parsed) {
        return trimmed
    }
    return toDateKey(parsed)
}

export function toDateTimeInputValue(value?: string | null): string {
    if (!value) return ''
    const trimmed = value.trim()
    if (!trimmed) return ''
    const timestamp = safeTimestamp(trimmed)
    if (timestamp === null) {
        return trimmed
    }
    const date = new Date(timestamp)
    return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}T${pad2(date.getHours())}:${pad2(date.getMinutes())}`
}

export function fromDateTimeInputValue(value: unknown): string | null {
    if (value === null || value === undefined) return null
    const trimmed = typeof value === 'string' ? value.trim() : String(value).trim()
    if (!trimmed) return null
    const parsed = Date.parse(trimmed)
    if (!Number.isFinite(parsed)) {
        return trimmed
    }
    return new Date(parsed).toISOString()
}

/**
 * Format a timestamp as a localized date-time string.
 * Unparseable string values are returned unchanged; empty values yield
 * `opts.empty` (default '').
 */
export function formatDateTime(
    value?: string | Date | null,
    opts?: { empty?: string; mediumShort?: boolean },
): string {
    if (value === null || value === undefined || value === '') return opts?.empty ?? ''
    const date = typeof value === 'string' ? new Date(value) : value
    if (Number.isNaN(date.getTime())) return typeof value === 'string' ? value : String(value)
    if (opts?.mediumShort) {
        return date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
    }
    return date.toLocaleString()
}

const relativeTimeFormatter =
    typeof Intl !== 'undefined' && (Intl as any).RelativeTimeFormat
        ? new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
        : null

const relativeUnits: Array<{ unit: Intl.RelativeTimeFormatUnit; ms: number }> = [
    { unit: 'year', ms: 1000 * 60 * 60 * 24 * 365 },
    { unit: 'month', ms: 1000 * 60 * 60 * 24 * 30 },
    { unit: 'week', ms: 1000 * 60 * 60 * 24 * 7 },
    { unit: 'day', ms: 1000 * 60 * 60 * 24 },
    { unit: 'hour', ms: 1000 * 60 * 60 },
    { unit: 'minute', ms: 1000 * 60 },
    { unit: 'second', ms: 1000 },
]

/**
 * Format a timestamp as a localized relative time ("3 days ago", "in 2 hours").
 * Unparseable values are returned unchanged; empty values yield `opts.empty`
 * (default '').
 */
export function formatRelativeTime(
    value?: string | null,
    opts?: { empty?: string },
): string {
    if (!value) return opts?.empty ?? ''
    const timestamp = safeTimestamp(value)
    if (timestamp === null) return value
    const diff = timestamp - Date.now()
    if (!relativeTimeFormatter) return new Date(timestamp).toLocaleString()
    for (const { unit, ms } of relativeUnits) {
        if (Math.abs(diff) >= ms || unit === 'second') {
            const amount = Math.round(diff / ms)
            return relativeTimeFormatter.format(amount, unit)
        }
    }
    return new Date(timestamp).toLocaleString()
}
