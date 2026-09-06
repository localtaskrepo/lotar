/**
 * Safe localStorage access. All reads/writes tolerate unavailable storage
 * (SSR, privacy modes, quota errors) so callers never need try/catch.
 */

export function storageGet(key: string): string | null {
    try {
        return typeof localStorage !== 'undefined' ? localStorage.getItem(key) : null
    } catch {
        return null
    }
}

export function storageSet(key: string, value: string): void {
    try {
        if (typeof localStorage !== 'undefined') {
            localStorage.setItem(key, value)
        }
    } catch {
        // ignore quota / availability errors
    }
}

export function storageGetJson<T>(key: string): T | null {
    const raw = storageGet(key)
    if (!raw) return null
    try {
        return JSON.parse(raw) as T
    } catch {
        return null
    }
}

export function storageSetJson(key: string, value: unknown): void {
    try {
        storageSet(key, JSON.stringify(value))
    } catch {
        // ignore serialization errors
    }
}

export function storageGetFlag(key: string, fallback = false): boolean {
    const raw = storageGet(key)
    if (raw === 'true') return true
    if (raw === 'false') return false
    return fallback
}

export function storageSetFlag(key: string, value: boolean): void {
    storageSet(key, value ? 'true' : 'false')
}

export function storageRemove(key: string): void {
    try {
        if (typeof localStorage !== 'undefined') {
            localStorage.removeItem(key)
        }
    } catch {
        // ignore availability errors
    }
}
