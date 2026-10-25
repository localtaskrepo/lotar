/**
 * Insights metrics (DEV-91), computed from the task list and each task's
 * embedded history. Pure functions: callers pass "now" and the completion
 * policy so every number is reproducible in tests.
 */
import type { TaskDTO } from '../api/types'
import { parseEffort } from './taskSort'

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

/** Status comparison key: case, spaces, `_` and `-` are ignored. */
export function statusKey(value: string | null | undefined): string {
    return (value ?? '')
        .trim()
        .toLowerCase()
        .replace(/[\s_-]+/g, '')
}

export interface StatusChange {
    at: number
    old: string | null
    next: string | null
}

export interface TaskTimeline {
    task: TaskDTO
    created: number | null
    /** Status changes, oldest first. */
    changes: StatusChange[]
    /** Every history entry (time and actor), for activity metrics. */
    entries: Array<{ at: number; actor: string }>
    /** Key of the project's starting status (e.g. "todo"), when known. */
    initialKey: string | null
    /** First move into an active status (neither starting nor done). */
    startedAt: number | null
    /** Start of the final done stretch (null while open). */
    doneAt: number | null
    /** Moves from a done status back to an open one. */
    reopenedAt: number[]
    isDone: boolean
    /** Started, not done, and not moved back to the starting status. */
    inProgress: boolean
}

export interface CompletionLookup {
    isTaskDone(task: TaskDTO): boolean
    /** Done status labels for this task (its own policy, as `isTaskDone` uses). */
    doneLabels(task: TaskDTO): readonly string[]
}

export function projectOf(id: string): string {
    const dash = id.lastIndexOf('-')
    return dash > 0 ? id.slice(0, dash) : ''
}

function readHistory(task: TaskDTO): { changes: StatusChange[]; entries: TaskTimeline['entries'] } {
    const changes: StatusChange[] = []
    const entries: TaskTimeline['entries'] = []
    for (const entry of Array.isArray(task.history) ? task.history : []) {
        const at = parseTime(entry?.at)
        if (at === null) continue
        entries.push({ at, actor: (entry.actor ?? '').trim() })
        for (const change of Array.isArray(entry.changes) ? entry.changes : []) {
            if (change?.field === 'status') changes.push({ at, old: change.old ?? null, next: change.new ?? null })
        }
    }
    changes.sort((a, b) => a.at - b.at)
    return { changes, entries }
}

/**
 * Each project's starting status, inferred from history: the status most
 * tasks were in before their first recorded move.
 */
export function inferInitialStatuses(tasks: readonly TaskDTO[]): Map<string, string> {
    const votes = new Map<string, Map<string, number>>()
    for (const task of tasks) {
        let first: { at: number; old: string | null } | null = null
        for (const entry of Array.isArray(task.history) ? task.history : []) {
            const at = parseTime(entry?.at)
            if (at === null) continue
            for (const change of Array.isArray(entry.changes) ? entry.changes : []) {
                if (change?.field === 'status' && change.old && (!first || at < first.at))
                    first = { at, old: change.old }
            }
        }
        if (!first) continue
        const project = projectOf(task.id)
        const tally = votes.get(project) ?? new Map<string, number>()
        const key = statusKey(first.old)
        tally.set(key, (tally.get(key) ?? 0) + 1)
        votes.set(project, tally)
    }
    const out = new Map<string, string>()
    for (const [project, tally] of votes) {
        const best = [...tally.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0]
        if (best) out.set(project, best[0])
    }
    return out
}

const timelineCache = new WeakMap<TaskDTO, { signature: string; timeline: TaskTimeline }>()

/**
 * Timeline of one task. Cached per task object: the task store replaces the
 * object when a task changes, so unchanged tasks are never re-parsed.
 */
export function buildTimeline(
    task: TaskDTO,
    completion: CompletionLookup,
    initialStatus: string | null = null,
): TaskTimeline {
    const isDone = completion.isTaskDone(task)
    const doneKeys = new Set(completion.doneLabels(task).map(statusKey))
    const signature = `${isDone}|${[...doneKeys].join(',')}|${initialStatus ?? ''}`
    const cached = timelineCache.get(task)
    if (cached && cached.signature === signature) return cached.timeline

    const created = parseTime(task.created)
    const { changes, entries } = readHistory(task)
    const initialKey = changes.length ? statusKey(changes[0]!.old) || initialStatus : initialStatus
    const isDoneKey = (key: string) => doneKeys.has(key)
    const isActiveKey = (key: string) => !!key && key !== initialKey && !isDoneKey(key)

    let startedAt: number | null = null
    let inDone = changes.length ? isDoneKey(statusKey(changes[0]!.old)) : isDone
    let doneCandidate: number | null = inDone ? created : null
    const reopenedAt: number[] = []
    for (const change of changes) {
        const nextKey = statusKey(change.next)
        if (startedAt === null && isActiveKey(nextKey)) startedAt = change.at
        const nextDone = isDoneKey(nextKey)
        if (nextDone && !inDone) doneCandidate = change.at
        if (!nextDone && inDone) {
            doneCandidate = null
            reopenedAt.push(change.at)
        }
        inDone = nextDone
    }
    // Created straight into an active status: work started at creation.
    if (startedAt === null && !changes.length && initialKey !== null && isActiveKey(statusKey(task.status))) {
        startedAt = created
    }
    const doneAt = isDone ? (doneCandidate ?? (changes.length ? changes[changes.length - 1]!.at : created)) : null
    const currentKey = statusKey(task.status)
    const timeline: TaskTimeline = {
        task,
        created,
        changes,
        entries,
        initialKey,
        startedAt,
        doneAt,
        reopenedAt,
        isDone,
        inProgress: !isDone && startedAt !== null && currentKey !== initialKey,
    }
    timelineCache.set(task, { signature, timeline })
    return timeline
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

/** Minimum samples before a percentile is shown or used for flagging. */
export const MIN_PERCENTILE_SAMPLES = 5

/** Percentile only when there are enough samples to mean something. */
export function stablePercentile(values: readonly number[], p: number): number | null {
    return values.length >= MIN_PERCENTILE_SAMPLES ? percentile(values, p) : null
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
    /** Only part of this bucket lies inside the selected range. */
    partial: boolean
}

export type BucketUnit = 'day' | 'week' | 'month'

export function bucketUnitFor(range: Range): BucketUnit {
    const days = (range.end - range.start) / DAY_MS
    return days <= 14 ? 'day' : days <= 120 ? 'week' : 'month'
}

/** How many whole units the range spans (for "per week" rates). */
export function unitsIn(range: Range, unit: BucketUnit): number {
    const unitDays = unit === 'day' ? 1 : unit === 'week' ? 7 : 365.25 / 12
    return Math.max(1, (range.end - range.start) / DAY_MS / unitDays)
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
function bucket(start: number, end: number, label: string, range: Range): Bucket {
    return { start, end, label, partial: start < range.start || end > range.end }
}

export function bucketsFor(range: Range): Bucket[] {
    const unit = bucketUnitFor(range)
    const buckets: Bucket[] = []
    if (unit === 'day') {
        for (let at = startOfLocalDay(range.start); at < range.end;) {
            const next = new Date(at)
            next.setDate(next.getDate() + 1)
            buckets.push(bucket(at, next.getTime(), SHORT_DATE.format(at), range))
            at = next.getTime()
        }
    } else if (unit === 'week') {
        const first = new Date(startOfLocalDay(range.start))
        first.setDate(first.getDate() - ((first.getDay() + 6) % 7))
        for (let at = first.getTime(); at < range.end;) {
            const next = new Date(at)
            next.setDate(next.getDate() + 7)
            buckets.push(bucket(at, next.getTime(), SHORT_DATE.format(at), range))
            at = next.getTime()
        }
    } else {
        const first = new Date(startOfLocalDay(range.start))
        first.setDate(1)
        for (let at = first.getTime(); at < range.end;) {
            const next = new Date(at)
            next.setMonth(next.getMonth() + 1)
            buckets.push(bucket(at, next.getTime(), MONTH.format(at), range))
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
 * Average days per completed task spent in each status, for tasks completed
 * in `range`, replayed from history (creation to done). Statuses are grouped
 * case- and separator-insensitively; done statuses are excluded. `share` is
 * the fraction of completed tasks that passed through the status.
 */
export function timeInStatus(
    timelines: readonly TaskTimeline[],
    range: Range,
    completion: CompletionLookup,
): Array<{ status: string; days: number; share: number }> {
    const totals = new Map<string, { label: string; ms: number; tasks: Set<string> }>()
    let completedCount = 0
    for (const timeline of timelines) {
        if (!inRange(timeline.doneAt, range) || timeline.created === null) continue
        completedCount += 1
        const doneKeys = new Set(completion.doneLabels(timeline.task).map(statusKey))
        const { changes } = timeline
        let status = changes.length ? changes[0]!.old : timeline.task.status
        let cursor = timeline.created
        const add = (name: string | null, until: number) => {
            const key = statusKey(name)
            if (!key || doneKeys.has(key) || until <= cursor) return
            const entry = totals.get(key) ?? { label: name!, ms: 0, tasks: new Set<string>() }
            entry.ms += until - cursor
            entry.tasks.add(timeline.task.id)
            totals.set(key, entry)
        }
        for (const change of changes) {
            if (change.at > timeline.doneAt!) break
            add(status, change.at)
            status = change.next
            cursor = change.at
        }
        add(status, timeline.doneAt!)
    }
    return [...totals.values()]
        .map(({ label, ms, tasks }) => ({
            status: label,
            days: ms / DAY_MS / completedCount,
            share: tasks.size / completedCount,
        }))
        .sort((a, b) => b.days - a.days)
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
export function activityByActor(timelines: readonly TaskTimeline[], range: Range): CountEntry[] {
    const counts = new Map<string, number>()
    for (const timeline of timelines) {
        for (const entry of timeline.entries) {
            if (!inRange(entry.at, range)) continue
            const actor = entry.actor || '(unknown)'
            counts.set(actor, (counts.get(actor) ?? 0) + 1)
        }
    }
    return [...counts.entries()]
        .map(([key, count]) => ({ key, count }))
        .sort((a, b) => b.count - a.count || a.key.localeCompare(b.key))
}

/**
 * Open tasks at the end of each bucket (created before it, not yet done),
 * for backlog trends. Reopened work counts as open from its reopening.
 */
export function openAtBucketEnds(
    timelines: readonly TaskTimeline[],
    buckets: readonly Bucket[],
    now: number,
): number[] {
    return buckets.map((bucket) => {
        const at = Math.min(bucket.end, now)
        let open = 0
        for (const timeline of timelines) {
            if (timeline.created === null || timeline.created >= at) continue
            if (timeline.doneAt !== null && timeline.doneAt < at) continue
            open += 1
        }
        return open
    })
}

/** Parse a due value; bare dates are local calendar days. */
export function parseDue(value: string | null | undefined): number | null {
    if (!value) return null
    if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return new Date(`${value}T00:00:00`).getTime()
    return parseTime(value)
}

/** Bug-like task types (Bug, Defect, Incident, Regression...). */
export function isBugType(type: string | null | undefined): boolean {
    return /\b(bug|defect|incident|regression|hotfix)s?\b/i.test(type ?? '')
}

/** Format a day count compactly: "4h", "2.5d", "3w". */
export function formatDays(days: number | null | undefined): string {
    if (days == null || !Number.isFinite(days)) return '—'
    if (days * 24 < 1) return '<1h'
    if (days < 1) return `${Math.round(days * 24)}h`
    if (days < 14) return `${days < 10 ? Math.round(days * 10) / 10 : Math.round(days)}d`
    if (days < 90) return `${Math.round(days / 7)}w`
    return `${Math.round(days / 30)}mo`
}
