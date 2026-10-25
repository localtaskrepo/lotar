import { describe, expect, it } from 'vitest'
import type { TaskDTO, TaskHistoryEntry } from '../api/types'
import {
    activityByActor,
    bucketsFor,
    buildTimeline,
    changeRatio,
    countBy,
    countPerBucket,
    cycleDays,
    DAY_MS,
    durationHistogram,
    effortTotals,
    formatDays,
    isBugType,
    leadDays,
    metDueDate,
    orderByConfigured,
    percentile,
    periodRange,
    previousRange,
    timeInStatus,
    type CompletionLookup,
} from '../utils/insightsMetrics'

const T0 = Date.parse('2026-03-02T09:00:00Z')
const iso = (offsetDays: number) => new Date(T0 + offsetDays * DAY_MS).toISOString()

function move(day: number, from: string, to: string, actor = 'ana'): TaskHistoryEntry {
    return { at: iso(day), actor, changes: [{ field: 'status', old: from, new: to }] }
}

function task(id: string, fields: Partial<TaskDTO> = {}): TaskDTO {
    return {
        id,
        title: id,
        status: 'Todo',
        priority: 'Medium',
        task_type: 'Feature',
        created: iso(0),
        modified: iso(0),
        tags: [],
        relationships: {},
        comments: [],
        references: [],
        sprints: [],
        history: [],
        custom_fields: {},
        ...fields,
    } as TaskDTO
}

const completion: CompletionLookup = {
    isTaskDone: (t) => t.status === 'Done',
    isDoneStatus: (_project, status) => status === 'Done',
}

describe('timelines', () => {
    const done = task('A-1', {
        status: 'Done',
        history: [move(2, 'Todo', 'InProgress'), move(5, 'InProgress', 'Review'), move(6, 'Review', 'Done')],
    })

    it('reads start and done from status history', () => {
        const timeline = buildTimeline(done, completion)
        expect(timeline.isDone).toBe(true)
        expect(timeline.startedAt).toBe(T0 + 2 * DAY_MS)
        expect(timeline.doneAt).toBe(T0 + 6 * DAY_MS)
        expect(cycleDays(timeline)).toBe(4)
        expect(leadDays(timeline)).toBe(6)
    })

    it('falls back to modified for tasks done without a recorded move', () => {
        const timeline = buildTimeline(task('A-2', { status: 'Done', modified: iso(3) }), completion)
        expect(timeline.doneAt).toBe(T0 + 3 * DAY_MS)
        expect(cycleDays(timeline)).toBeNull()
        expect(leadDays(timeline)).toBe(3)
    })

    it('keeps open tasks open even if they were done before (reopened)', () => {
        const reopened = task('A-3', { history: [move(1, 'Todo', 'Done'), move(2, 'Done', 'Todo')] })
        expect(buildTimeline(reopened, completion).doneAt).toBeNull()
    })

    it('averages time in each non-done status for tasks finished in range', () => {
        const range = { start: T0, end: T0 + 10 * DAY_MS }
        const rows = timeInStatus([buildTimeline(done, completion)], range, completion)
        expect(rows).toEqual([
            { status: 'InProgress', days: 3, tasks: 1 },
            { status: 'Todo', days: 2, tasks: 1 },
            { status: 'Review', days: 1, tasks: 1 },
        ])
    })

    it('checks due dates against the end of the due day', () => {
        const onTime = buildTimeline(
            task('A-4', { status: 'Done', due_date: iso(6).slice(0, 10), modified: iso(6) }),
            completion,
        )
        const late = buildTimeline(
            task('A-5', { status: 'Done', due_date: iso(4).slice(0, 10), modified: iso(6) }),
            completion,
        )
        expect(metDueDate(onTime)).toBe(true)
        expect(metDueDate(late)).toBe(false)
        expect(metDueDate(buildTimeline(task('A-6', { status: 'Done' }), completion))).toBeNull()
    })
})

describe('periods and buckets', () => {
    it('builds ranges and the comparison period', () => {
        const range = periodRange('30d', T0)
        expect(range).toEqual({ start: T0 - 30 * DAY_MS, end: T0 })
        expect(previousRange(range)).toEqual({ start: T0 - 60 * DAY_MS, end: T0 - 30 * DAY_MS })
        const all = periodRange('all', T0, [task('A-1', { created: iso(-400) })])
        expect(all.start).toBe(T0 - 400 * DAY_MS)
    })

    it('uses days, weeks or months depending on the span', () => {
        expect(bucketsFor({ start: T0 - 7 * DAY_MS, end: T0 }).length).toBeGreaterThanOrEqual(7)
        const weeks = bucketsFor({ start: T0 - 30 * DAY_MS, end: T0 })
        expect(weeks.length).toBeGreaterThanOrEqual(5)
        expect(weeks.length).toBeLessThanOrEqual(6)
        expect(new Date(weeks[0]!.start).getDay()).toBe(1)
        const months = bucketsFor({ start: T0 - 365 * DAY_MS, end: T0 })
        expect(months.length).toBeGreaterThanOrEqual(12)
        expect(months.length).toBeLessThanOrEqual(13)
    })

    it('counts timestamps per bucket', () => {
        const buckets = [
            { start: 0, end: 10, label: 'a' },
            { start: 10, end: 20, label: 'b' },
        ]
        expect(countPerBucket([1, 9, 10, null, 25], buckets)).toEqual([2, 1])
    })
})

describe('aggregates', () => {
    it('computes percentiles and change ratios', () => {
        expect(percentile([], 50)).toBeNull()
        expect(percentile([4, 1, 3, 2], 50)).toBe(2.5)
        expect(percentile([1, 2, 3, 4, 5], 85)).toBeCloseTo(4.4)
        expect(changeRatio(15, 10)).toBe(0.5)
        expect(changeRatio(0, 0)).toBe(0)
        expect(changeRatio(3, 0)).toBeNull()
    })

    it('counts by key, with blanks and multi-valued keys', () => {
        const tasks = [
            task('A-1', { assignee: 'ana', tags: ['ui', 'api'] }),
            task('A-2', { assignee: 'ana', tags: ['ui'] }),
            task('A-3', { tags: [] }),
        ]
        expect(countBy(tasks, (t) => t.assignee)).toEqual([
            { key: 'ana', count: 2 },
            { key: '(none)', count: 1 },
        ])
        expect(countBy(tasks, (t) => t.tags)).toEqual([
            { key: 'ui', count: 2 },
            { key: 'api', count: 1 },
        ])
        expect(
            orderByConfigured(
                [
                    { key: 'High', count: 1 },
                    { key: 'Low', count: 5 },
                ],
                ['Low', 'High'],
            ).map((e) => e.key),
        ).toEqual(['Low', 'High'])
    })

    it('bins durations into readable ranges', () => {
        const bins = durationHistogram([0.5, 2, 2.5, 10, 200])
        expect(bins.map((bin) => bin.count)).toEqual([1, 2, 0, 1, 0, 0, 1])
    })

    it('totals effort without mixing hours and points', () => {
        const totals = effortTotals([
            task('A-1', { effort: '2d' }),
            task('A-2', { effort: '3pt' }),
            task('A-3', { effort: '' }),
        ])
        expect(totals.estimated).toBe(2)
        expect(totals.points).toBe(3)
        expect(totals.hours).toBeGreaterThan(0)
    })

    it('counts activity per actor inside the range', () => {
        const tasks = [
            task('A-1', { history: [move(1, 'Todo', 'InProgress', 'ana'), move(20, 'InProgress', 'Done', 'ben')] }),
            task('A-2', { history: [{ at: iso(2), actor: null, changes: [{ field: 'comment_added' }] }] }),
        ]
        expect(activityByActor(tasks, { start: T0, end: T0 + 10 * DAY_MS })).toEqual([
            { key: '(unknown)', count: 1 },
            { key: 'ana', count: 1 },
        ])
    })

    it('recognises bug-like types and formats durations', () => {
        expect(['Bug', 'defect', 'Incident', 'Regression'].every(isBugType)).toBe(true)
        expect(isBugType('Debugging spike')).toBe(false)
        expect(formatDays(0.25)).toBe('6h')
        expect(formatDays(2.34)).toBe('2.3d')
        expect(formatDays(21)).toBe('3w')
        expect(formatDays(120)).toBe('4mo')
        expect(formatDays(null)).toBe('—')
    })
})
