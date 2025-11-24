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
