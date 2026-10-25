/**
 * Insights metrics (DEV-91), computed from the task list and each task's
 * embedded history. Pure functions: callers pass "now" and the completion
 * policy so every number is reproducible in tests.
 */
import type { TaskDTO } from '../api/types'
import { parseEffort } from './taskSort'
import { statusEnteredAt } from './taskHistory'

export const DAY_MS = 86_400_000

export type PeriodKey = '7d' | '30d' | '90d' | '180d' | '365d' | 'all'

export const PERIOD_OPTIONS: Array<{ value: PeriodKey; label: string; days: number | null }> = [
    { value: '7d', label: 'Last 7 days', days: 7 },
    { value: '30d', label: 'Last 30 days', days: 30 },
    { value: '90d', label: 'Last 90 days', days: 90 },
    { value: '180d', label: 'Last 6 months', days: 180 },
    { value: '365d', label: 'Last 12 months', days: 365 },
    { value: 'all', label: 'All time', days: null },
]

export function isPeriodKey(value: unknown): value is PeriodKey {
    return PERIOD_OPTIONS.some((option) => option.value === value)
}

/** Half-open time range [start, end) in epoch milliseconds. */
export interface Range {
    start: number
    end: number
}

export function inRange(at: number | null | undefined, range: Range): boolean {
    return at != null && at >= range.start && at < range.end
}

function parseTime(value: string | null | undefined): number | null {
    if (!value) return null
    const at = Date.parse(value)
    return Number.isNaN(at) ? null : at
}

/**
 * The selected period ending now. "All time" starts at the earliest task
 * creation (or 30 days back when there are no tasks).
 */
export function periodRange(key: PeriodKey, now: number, tasks: readonly TaskDTO[] = []): Range {
    const days = PERIOD_OPTIONS.find((option) => option.value === key)?.days
    if (days) return { start: now - days * DAY_MS, end: now }
    let earliest = now - 30 * DAY_MS
    for (const task of tasks) {
        const created = parseTime(task.created)
        if (created !== null && created < earliest) earliest = created
    }
    return { start: earliest, end: now }
}

/** The equally long period just before `range`, for comparisons. */
export function previousRange(range: Range): Range {
    const length = range.end - range.start
    return { start: range.start - length, end: range.start }
}

// ---------------------------------------------------------------------------
// Task timelines
// ---------------------------------------------------------------------------

export interface TaskTimeline {
    task: TaskDTO
    created: number | null
    /** First status change: when work left its initial status. */
    startedAt: number | null
    /** When the task last entered a done status (null while open). */
    doneAt: number | null
    isDone: boolean
}

export interface CompletionLookup {
    isTaskDone(task: TaskDTO): boolean
    isDoneStatus(projectPrefix: string, status: string | null | undefined): boolean
}

export function projectOf(id: string): string {
    const dash = id.lastIndexOf('-')
    return dash > 0 ? id.slice(0, dash) : ''
}

function statusChanges(task: TaskDTO): Array<{ at: number; old: string | null; next: string | null }> {
    const out: Array<{ at: number; old: string | null; next: string | null }> = []
    for (const entry of Array.isArray(task.history) ? task.history : []) {
        const at = parseTime(entry?.at)
        if (at === null || !Array.isArray(entry.changes)) continue
        for (const change of entry.changes) {
            if (change?.field === 'status') out.push({ at, old: change.old ?? null, next: change.new ?? null })
        }
    }
    return out.sort((a, b) => a.at - b.at)
}

export function buildTimeline(task: TaskDTO, completion: CompletionLookup): TaskTimeline {
    const created = parseTime(task.created)
    const changes = statusChanges(task)
    const isDone = completion.isTaskDone(task)
    const project = projectOf(task.id)
    let doneAt: number | null = null
    if (isDone) {
        for (const change of changes) {
            if (completion.isDoneStatus(project, change.next)) doneAt = change.at
        }
        // Done without a recorded transition: created done, or edited by hand.
        doneAt ??= parseTime(task.modified) ?? created
    }
    const startedAt = changes.length ? changes[0]!.at : null
    return { task, created, startedAt, doneAt, isDone }
}

/** Cycle time (start of work to done) in days, when both ends are known. */
export function cycleDays(timeline: TaskTimeline): number | null {
    const { startedAt, doneAt } = timeline
    if (startedAt === null || doneAt === null || doneAt < startedAt) return null
    return (doneAt - startedAt) / DAY_MS
}

/** Lead time (creation to done) in days. */
export function leadDays(timeline: TaskTimeline): number | null {
    const { created, doneAt } = timeline
    if (created === null || doneAt === null || doneAt < created) return null
    return (doneAt - created) / DAY_MS
}

// ---------------------------------------------------------------------------
// Aggregates
// ---------------------------------------------------------------------------

/** Linear-interpolated percentile (p in 0..100); null for no values. */
export function percentile(values: readonly number[], p: number): number | null {
    if (!values.length) return null
    const sorted = [...values].sort((a, b) => a - b)
    const rank = (Math.min(100, Math.max(0, p)) / 100) * (sorted.length - 1)
    const low = Math.floor(rank)
    const high = Math.ceil(rank)
    return sorted[low]! + (sorted[high]! - sorted[low]!) * (rank - low)
}

/** Relative change from `previous` to `current`; null when undefined. */
export function changeRatio(current: number, previous: number): number | null {
    if (previous === 0) return current === 0 ? 0 : null
    return (current - previous) / previous
}

export interface Bucket {
    start: number
    end: number
    label: string
}

const SHORT_DATE = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
const MONTH = new Intl.DateTimeFormat(undefined, { month: 'short', year: '2-digit' })

function startOfLocalDay(at: number): number {
    const date = new Date(at)
    date.setHours(0, 0, 0, 0)
    return date.getTime()
}

/**
 * Split a range into chart buckets: days up to two weeks, weeks (starting
 * Monday) up to ~4 months, months beyond.
 */
export function bucketsFor(range: Range): Bucket[] {
    const days = (range.end - range.start) / DAY_MS
    const buckets: Bucket[] = []
    if (days <= 14) {
        for (let at = startOfLocalDay(range.start); at < range.end;) {
            const next = new Date(at)
            next.setDate(next.getDate() + 1)
            buckets.push({ start: at, end: next.getTime(), label: SHORT_DATE.format(at) })
            at = next.getTime()
        }
    } else if (days <= 120) {
        const first = new Date(startOfLocalDay(range.start))
        first.setDate(first.getDate() - ((first.getDay() + 6) % 7))
        for (let at = first.getTime(); at < range.end;) {
            const next = new Date(at)
            next.setDate(next.getDate() + 7)
            buckets.push({ start: at, end: next.getTime(), label: SHORT_DATE.format(at) })
            at = next.getTime()
        }
    } else {
        const first = new Date(startOfLocalDay(range.start))
        first.setDate(1)
        for (let at = first.getTime(); at < range.end;) {
            const next = new Date(at)
            next.setMonth(next.getMonth() + 1)
            buckets.push({ start: at, end: next.getTime(), label: MONTH.format(at) })
            at = next.getTime()
        }
    }
    return buckets
}

/** Count timestamps per bucket. */
export function countPerBucket(times: ReadonlyArray<number | null>, buckets: readonly Bucket[]): number[] {
    return buckets.map((bucket) => times.filter((at) => at !== null && at >= bucket.start && at < bucket.end).length)
}

export interface CountEntry {
    key: string
    count: number
}

/** Count tasks per key, largest first (ties alphabetical); blanks become `blank`. */
export function countBy(
    tasks: readonly TaskDTO[],
    keyOf: (task: TaskDTO) => string | string[] | null | undefined,
    blank = '(none)',
): CountEntry[] {
    const counts = new Map<string, number>()
    for (const task of tasks) {
        const raw = keyOf(task)
        const keys = Array.isArray(raw) ? raw : [raw]
        const cleaned = keys.map((key) => (key ?? '').trim()).filter(Boolean)
        for (const key of cleaned.length ? cleaned : Array.isArray(raw) ? [] : [blank]) {
            counts.set(key, (counts.get(key) ?? 0) + 1)
        }
    }
    return [...counts.entries()]
        .map(([key, count]) => ({ key, count }))
        .sort((a, b) => b.count - a.count || a.key.localeCompare(b.key))
}

/** Order entries by a configured label order; unknown keys follow by count. */
export function orderByConfigured(entries: CountEntry[], order: readonly string[]): CountEntry[] {
    const rank = new Map(order.map((label, index) => [label.trim().toLowerCase(), index]))
    return [...entries].sort((a, b) => {
        const ra = rank.get(a.key.toLowerCase()) ?? Infinity
        const rb = rank.get(b.key.toLowerCase()) ?? Infinity
        return ra - rb || b.count - a.count || a.key.localeCompare(b.key)
    })
}

export interface HistogramBin {
    label: string
    /** Upper bound in days (exclusive); Infinity for the last bin. */
    max: number
    count: number
}

const DURATION_BINS: Array<{ label: string; max: number }> = [
    { label: '<1d', max: 1 },
    { label: '1–3d', max: 3 },
    { label: '3–7d', max: 7 },
    { label: '1–2w', max: 14 },
    { label: '2–4w', max: 28 },
    { label: '1–3mo', max: 90 },
    { label: '3mo+', max: Infinity },
]

/** Distribution of durations (days) over fixed, readable bins. */
export function durationHistogram(days: readonly number[]): HistogramBin[] {
    const bins = DURATION_BINS.map((bin) => ({ ...bin, count: 0 }))
    for (const value of days) {
        const bin = bins.find((candidate) => value < candidate.max)
        if (bin) bin.count += 1
    }
    return bins
}

/**
 * Average days spent in each status by tasks completed in `range`, replayed
 * from history (creation to done). Done statuses are excluded.
 */
export function timeInStatus(
    timelines: readonly TaskTimeline[],
    range: Range,
    completion: CompletionLookup,
): Array<{ status: string; days: number; tasks: number }> {
    const totals = new Map<string, { ms: number; tasks: Set<string> }>()
    for (const timeline of timelines) {
        if (!inRange(timeline.doneAt, range) || timeline.created === null) continue
        const project = projectOf(timeline.task.id)
        const changes = statusChanges(timeline.task)
        let status = changes[0]?.old ?? (changes.length ? null : timeline.task.status)
        let cursor = timeline.created
        const add = (name: string | null, until: number) => {
            if (!name || completion.isDoneStatus(project, name) || until <= cursor) return
            const entry = totals.get(name) ?? { ms: 0, tasks: new Set<string>() }
            entry.ms += until - cursor
            entry.tasks.add(timeline.task.id)
            totals.set(name, entry)
        }
        for (const change of changes) {
            if (change.at > timeline.doneAt!) break
            add(status, change.at)
            status = change.next
            cursor = change.at
        }
        add(status, timeline.doneAt!)
    }
    return [...totals.entries()]
        .map(([status, { ms, tasks }]) => ({ status, days: ms / DAY_MS / tasks.size, tasks: tasks.size }))
        .sort((a, b) => b.days - a.days)
}

/** Days since an open task entered its current status. */
export function daysInCurrentStatus(task: TaskDTO, now: number): number | null {
    const since = statusEnteredAt(task, task.status)
    return since === null ? null : Math.max(0, (now - since) / DAY_MS)
}

/** Whether a completed task met its due date (end of the due day, local). */
export function metDueDate(timeline: TaskTimeline): boolean | null {
    const due = timeline.task.due_date
    if (!due || timeline.doneAt === null) return null
    const dayOnly = /^\d{4}-\d{2}-\d{2}$/.test(due)
    const deadline = dayOnly ? new Date(`${due}T23:59:59.999`).getTime() : parseTime(due)
    return deadline === null ? null : timeline.doneAt <= deadline
}

export interface EffortTotals {
    hours: number
    points: number
    estimated: number
}

/** Sum estimates without mixing units; `estimated` counts tasks with one. */
export function effortTotals(tasks: readonly TaskDTO[]): EffortTotals {
    const totals: EffortTotals = { hours: 0, points: 0, estimated: 0 }
    for (const task of tasks) {
        const parsed = parseEffort(task.effort)
        if (!parsed) continue
        totals.estimated += 1
        if (parsed.kind === 'hours') totals.hours += parsed.value
        else totals.points += parsed.value
    }
    return totals
}

/** History entries per actor within `range` (comments, edits, moves). */
export function activityByActor(tasks: readonly TaskDTO[], range: Range): CountEntry[] {
    const counts = new Map<string, number>()
    for (const task of tasks) {
        for (const entry of Array.isArray(task.history) ? task.history : []) {
            if (!inRange(parseTime(entry?.at), range)) continue
            const actor = (entry.actor ?? '').trim() || '(unknown)'
            counts.set(actor, (counts.get(actor) ?? 0) + 1)
        }
    }
    return [...counts.entries()]
        .map(([key, count]) => ({ key, count }))
        .sort((a, b) => b.count - a.count || a.key.localeCompare(b.key))
}

/** History entry timestamps within `range`, for activity trends. */
export function activityTimes(tasks: readonly TaskDTO[], range: Range): number[] {
    const times: number[] = []
    for (const task of tasks) {
        for (const entry of Array.isArray(task.history) ? task.history : []) {
            const at = parseTime(entry?.at)
            if (inRange(at, range)) times.push(at!)
        }
    }
    return times
}

/** Bug-like task types (Bug, Defect, Incident, Regression...). */
export function isBugType(type: string | null | undefined): boolean {
    return /\b(bug|defect|incident|regression|hotfix)s?\b/i.test(type ?? '')
}

/** Format a day count compactly: "4h", "2.5d", "3w". */
export function formatDays(days: number | null | undefined): string {
    if (days == null || !Number.isFinite(days)) return '—'
    if (days < 1) return `${Math.max(1, Math.round(days * 24))}h`
    if (days < 14) return `${days < 10 ? Math.round(days * 10) / 10 : Math.round(days)}d`
    if (days < 90) return `${Math.round(days / 7)}w`
    return `${Math.round(days / 30)}mo`
}
