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

export function compareTaskDates(a?: string | null, b?: string | null): number {
    const at = parseTaskDateToMillis(a)
    const bt = parseTaskDateToMillis(b)
    if (at === null && bt === null) return 0
    if (at === null) return -1
    if (bt === null) return 1
    return at - bt
}

export function isDateWithinRange(value: string | null | undefined, start: Date, end: Date): boolean {
    const parsed = parseTaskDate(value)
    if (!parsed) return false
    return parsed >= start && parsed <= end
}
