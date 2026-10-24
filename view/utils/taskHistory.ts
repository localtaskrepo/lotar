import type { TaskDTO } from '../api/types'

export function findLastStatusChangeAt(task: TaskDTO | null | undefined, status?: string): number | null {
    if (!task || !Array.isArray(task.history) || !task.history.length) return null
    for (let idx = task.history.length - 1; idx >= 0; idx -= 1) {
        const entry = task.history[idx]
        if (!entry || !Array.isArray(entry.changes)) continue
        const hit = entry.changes.find((change) => change?.field === 'status' && (!status || change.new === status))
        if (hit) {
            const ts = Date.parse(entry.at)
            return Number.isNaN(ts) ? null : ts
        }
    }
    return null
}

const statusKey = (value: unknown) =>
    typeof value === 'string'
        ? value
              .trim()
              .toLowerCase()
              .replace(/[\s_-]+/g, '')
        : ''

/**
 * When the task entered `status` (its current status), for age-based board
 * rules (DEV-133). Uses, in order:
 * 1. the latest recorded change into `status` (case and separator
 *    insensitive, like board columns);
 * 2. the creation time when the task never changed status (it was created in
 *    this status);
 * 3. the latest recorded status change (history that renamed or skipped the
 *    current label still bounds when the task got there).
 * Returns null when nothing can be dated, so callers keep the card visible.
 */
export function statusEnteredAt(task: TaskDTO | null | undefined, status: string): number | null {
    if (!task) return null
    const wanted = statusKey(status)
    let latestInto: number | null = null
    let latestAny: number | null = null
    let sawStatusChange = false
    for (const entry of Array.isArray(task.history) ? task.history : []) {
        if (!entry || !Array.isArray(entry.changes)) continue
        const changes = entry.changes.filter((change) => change?.field === 'status')
        if (!changes.length) continue
        sawStatusChange = true
        const at = Date.parse(entry.at)
        if (Number.isNaN(at)) continue
        latestAny = latestAny === null ? at : Math.max(latestAny, at)
        if (changes.some((change) => statusKey(change.new) === wanted)) {
            latestInto = latestInto === null ? at : Math.max(latestInto, at)
        }
    }
    if (latestInto !== null) return latestInto
    if (!sawStatusChange) {
        const created = Date.parse(task.created)
        return Number.isNaN(created) ? null : created
    }
    return latestAny
}
