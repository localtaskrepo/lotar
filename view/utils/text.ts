/** Capitalize the first character of a string. */
export function titleCase(value: string): string {
    return value ? value.charAt(0).toUpperCase() + value.slice(1) : value
}

/** Project prefix of a task ID (`DEV-123` → `DEV`). */
export function projectOf(id: string | null | undefined): string {
    return (id || '').split('-')[0] ?? ''
}

/** Numeric (or dashed remainder) part of a task ID (`DEV-123` → `123`). */
export function numericOf(id: string | null | undefined): string {
    return (id || '').split('-').slice(1).join('-')
}
