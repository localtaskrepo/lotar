import { storageGetJson, storageSetJson } from './storage'

// Tasks recently opened in the task panel, newest first, for the command
// palette (DEV-198). Kept per browser.

const RECENT_TASKS_KEY = 'lotar.recentTasks'
export const RECENT_TASKS_LIMIT = 8

export interface RecentTask {
    id: string
    title: string
}

export function readRecentTasks(): RecentTask[] {
    const stored = storageGetJson<unknown>(RECENT_TASKS_KEY)
    if (!Array.isArray(stored)) return []
    return stored
        .filter(
            (entry): entry is RecentTask =>
                !!entry && typeof entry.id === 'string' && !!entry.id && typeof entry.title === 'string',
        )
        .slice(0, RECENT_TASKS_LIMIT)
}

/** Move a task to the front of the list (or add it), keeping the newest title. */
export function recordRecentTask(id: string, title: string) {
    const trimmed = id.trim()
    if (!trimmed) return
    const next = [{ id: trimmed, title }, ...readRecentTasks().filter((entry) => entry.id !== trimmed)]
    storageSetJson(RECENT_TASKS_KEY, next.slice(0, RECENT_TASKS_LIMIT))
}

/** Drop a task from the list, e.g. an old ID of a moved task (DEV-183). */
export function forgetRecentTask(id: string) {
    const trimmed = id.trim()
    const current = readRecentTasks()
    const next = current.filter((entry) => entry.id !== trimmed)
    if (next.length !== current.length) storageSetJson(RECENT_TASKS_KEY, next)
}
