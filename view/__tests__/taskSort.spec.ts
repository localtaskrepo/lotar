import { describe, expect, it } from 'vitest'
import type { TaskDTO } from '../api/types'
import {
    colKeyToSortBy,
    compareTasks,
    customFieldValue,
    customValueToString,
    normalizeSortBy,
    normalizeSortOrder,
    parseEffort,
    sortByToColKey,
    sortTasks,
} from '../utils/taskSort'

function task(overrides: Partial<TaskDTO>): TaskDTO {
    return {
        id: 'A-1',
        title: 't',
        status: 'open',
        priority: 'med',
        task_type: 'task',
        reporter: null,
        assignee: null,
        created: '2026-01-01T10:00:00Z',
        modified: '2026-01-01T11:00:00Z',
        due_date: null,
        effort: null,
        tags: [],
        relationships: {},
        comments: [],
        references: [],
        history: [],
        custom_fields: {},
        sprints: [],
        ...overrides,
    }
}

describe('normalizeSortBy', () => {
    it('accepts the CLI builtin sort keys', () => {
        for (const key of ['priority', 'status', 'effort', 'due', 'created', 'modified', 'assignee', 'type', 'project', 'id']) {
            expect(normalizeSortBy(key)).toBe(key)
        }
    })

    it('normalizes aliases', () => {
        expect(normalizeSortBy('due-date')).toBe('due')
        expect(normalizeSortBy(' Modified ')).toBe('modified')
        expect(normalizeSortBy('field:Risk')).toBe('custom:Risk')
        expect(normalizeSortBy('custom:Risk')).toBe('custom:Risk')
        expect(normalizeSortBy('CUSTOM:Risk')).toBe('custom:Risk')
    })

    it('accepts the restored vector and string keys', () => {
        for (const key of ['title', 'tags', 'sprints', 'reporter']) {
            expect(normalizeSortBy(key)).toBe(key)
        }
    })

    it('rejects absent or invalid values', () => {
        expect(normalizeSortBy('')).toBeNull()
        expect(normalizeSortBy('  ')).toBeNull()
        expect(normalizeSortBy('bogus')).toBeNull()
        expect(normalizeSortBy('custom:')).toBeNull()
        expect(normalizeSortBy(undefined)).toBeNull()
        expect(normalizeSortBy(42)).toBeNull()
    })

    it('normalizes order with desc default', () => {
        expect(normalizeSortOrder('asc')).toBe('asc')
        expect(normalizeSortOrder('desc')).toBe('desc')
        expect(normalizeSortOrder('garbage')).toBe('desc')
        expect(normalizeSortOrder(undefined)).toBe('desc')
    })
})

describe('column mapping', () => {
    it('maps table columns onto server sort keys', () => {
        expect(colKeyToSortBy('task_type')).toBe('type')
        expect(colKeyToSortBy('due_date')).toBe('due')
        expect(colKeyToSortBy('modified')).toBe('modified')
        expect(colKeyToSortBy('custom:Risk')).toBe('custom:Risk')
    })

    it('maps every builtin table column onto a server sort key', () => {
        for (const col of ['title', 'tags', 'sprints', 'reporter', 'id', 'status', 'priority', 'task_type', 'assignee', 'effort', 'due_date', 'created', 'modified']) {
            expect(colKeyToSortBy(col)).not.toBeNull()
        }
        expect(colKeyToSortBy('nonexistent-column')).toBeNull()
    })

    it('maps sort keys back to columns', () => {
        expect(sortByToColKey('type')).toBe('task_type')
        expect(sortByToColKey('due')).toBe('due_date')
        expect(sortByToColKey('custom:Risk')).toBe('custom:Risk')
        expect(sortByToColKey('project')).toBeNull()
    })
})

describe('parseEffort (backend parity)', () => {
    it('parses single tokens', () => {
        expect(parseEffort('2h')).toEqual({ kind: 'hours', value: 2, canonical: '2.00h' })
        expect(parseEffort('90m')).toEqual({ kind: 'hours', value: 1.5, canonical: '1.50h' })
        expect(parseEffort('1.5d')).toEqual({ kind: 'hours', value: 12, canonical: '12.00h' })
        expect(parseEffort('1w')).toEqual({ kind: 'hours', value: 40, canonical: '40.00h' })
        expect(parseEffort('3pt')).toEqual({ kind: 'points', value: 3, canonical: '3pt' })
        expect(parseEffort('5')).toEqual({ kind: 'points', value: 5, canonical: '5pt' })
        expect(parseEffort('2.5pts')).toEqual({ kind: 'points', value: 2.5, canonical: '2.50pt' })
    })

    it('parses combined tokens and number/unit pairs', () => {
        expect(parseEffort('1d 2h')).toEqual({ kind: 'hours', value: 10, canonical: '10.00h' })
        expect(parseEffort('2 h')).toEqual({ kind: 'hours', value: 2, canonical: '2.00h' })
        expect(parseEffort('1 hour 30 minutes')).toEqual({ kind: 'hours', value: 1.5, canonical: '1.50h' })
    })

    it('rejects mixed and invalid input like the backend', () => {
        expect(parseEffort('1h 3pt')).toBeNull()
        expect(parseEffort('abc')).toBeNull()
        expect(parseEffort('')).toBeNull()
        expect(parseEffort(null)).toBeNull()
    })
})

describe('custom field values', () => {
    it('serializes values like the backend', () => {
        expect(customValueToString(null)).toBe('')
        expect(customValueToString(undefined)).toBe('')
        expect(customValueToString(true)).toBe('true')
        expect(customValueToString(7)).toBe('7')
        expect(customValueToString('high')).toBe('high')
        expect(customValueToString(['a', 'b'])).toBe('[array]')
        expect(customValueToString({ a: 1 })).toBe('{object}')
    })

    it('reads exact names first, then case-insensitive fallback', () => {
        const fields = { Risk: 'high', OTHER: 'x' }
        expect(customFieldValue(fields, 'Risk')).toBe('high')
        expect(customFieldValue(fields, 'risk')).toBe('high')
        expect(customFieldValue(fields, 'other')).toBe('x')
        expect(customFieldValue(fields, 'missing')).toBeNull()
        expect(customFieldValue(undefined, 'Risk')).toBeNull()
    })

    it('distinguishes missing from present-but-empty like the backend Option ordering', () => {
        const tasks = [
            task({ id: 'A-2', custom_fields: { Risk: '' } }),
            task({ id: 'A-1' }),
        ]
        // Missing (None) sorts before a present empty string (Some("")).
        expect(sortTasks(tasks, 'custom:Risk', 'asc').map((t) => t.id)).toEqual(['A-1', 'A-2'])
    })
})

describe('sortTasks', () => {
    it('defaults to modified desc in the caller and breaks ties by canonical ID lexical ASC', () => {
        const tasks = [
            task({ id: 'A-2', modified: '2026-01-01T11:00:00Z' }),
            task({ id: 'A-10', modified: '2026-01-01T11:00:00Z' }),
            task({ id: 'A-1', modified: '2026-01-02T11:00:00Z' }),
        ]
        const sorted = sortTasks(tasks, 'modified', 'desc')
        expect(sorted.map((t) => t.id)).toEqual(['A-1', 'A-10', 'A-2'])
        const asc = sortTasks(tasks, 'modified', 'asc')
        expect(asc.map((t) => t.id)).toEqual(['A-10', 'A-2', 'A-1'])
    })

    it('sorts custom fields with nested access and deterministic missing placement', () => {
        const tasks = [
            task({ id: 'A-3', custom_fields: { Risk: 'high' } }),
            task({ id: 'A-1', custom_fields: { risk: 'low' } }),
            task({ id: 'A-2' }),
        ]
        // Missing custom values compare as '' and sort first in asc;
        // 'high' < 'low' byte-wise, so A-3 precedes A-1.
        expect(sortTasks(tasks, 'custom:Risk', 'asc').map((t) => t.id)).toEqual(['A-2', 'A-3', 'A-1'])
        // In desc the primary flips; ties (none here) stay ID ASC.
        expect(sortTasks(tasks, 'custom:Risk', 'desc').map((t) => t.id)).toEqual(['A-1', 'A-3', 'A-2'])
    })

    it('sorts effort numerically within a kind and places missing values last in asc', () => {
        const tasks = [
            task({ id: 'A-1', effort: '2h' }),
            task({ id: 'A-2', effort: '1d' }),
            task({ id: 'A-3', effort: null }),
            task({ id: 'A-4', effort: '30m' }),
        ]
        expect(sortTasks(tasks, 'effort', 'asc').map((t) => t.id)).toEqual(['A-4', 'A-1', 'A-2', 'A-3'])
        expect(sortTasks(tasks, 'effort', 'desc').map((t) => t.id)).toEqual(['A-3', 'A-2', 'A-1', 'A-4'])
    })

    it('compares cross-kind efforts by canonical string', () => {
        const a = task({ id: 'A-1', effort: '5pt' })
        const b = task({ id: 'A-2', effort: '2h' })
        // '2.00h' < '5pt' byte-wise, so hours sort first in asc.
        expect(compareTasks(a, b, 'effort')).toBeGreaterThan(0)
        expect(compareTasks(b, a, 'effort')).toBeLessThan(0)
    })

    it('treats unparseable effort as missing', () => {
        const a = task({ id: 'A-1', effort: 'banana' })
        const b = task({ id: 'A-2', effort: '1h' })
        expect(sortTasks([a, b], 'effort', 'asc').map((t) => t.id)).toEqual(['A-2', 'A-1'])
    })

    it('sorts due dates lexically with missing last in asc', () => {
        const tasks = [
            task({ id: 'A-1', due_date: '2026-03-01' }),
            task({ id: 'A-2', due_date: null }),
            task({ id: 'A-3', due_date: '2026-02-01' }),
        ]
        expect(sortTasks(tasks, 'due', 'asc').map((t) => t.id)).toEqual(['A-3', 'A-1', 'A-2'])
        expect(sortTasks(tasks, 'due', 'desc').map((t) => t.id)).toEqual(['A-2', 'A-1', 'A-3'])
    })

    it('sorts missing assignees first in asc (Option ordering)', () => {
        const tasks = [
            task({ id: 'A-1', assignee: 'bob' }),
            task({ id: 'A-2', assignee: null }),
            task({ id: 'A-3', assignee: 'alice' }),
        ]
        expect(sortTasks(tasks, 'assignee', 'asc').map((t) => t.id)).toEqual(['A-2', 'A-3', 'A-1'])
    })

    it('sorts by project prefix of the canonical ID', () => {
        const tasks = [
            task({ id: 'ZZ-1' }),
            task({ id: 'AB-2' }),
            task({ id: 'AB-1' }),
        ]
        expect(sortTasks(tasks, 'project', 'asc').map((t) => t.id)).toEqual(['AB-1', 'AB-2', 'ZZ-1'])
    })

    it('sorts title and reporter like the backend', () => {
        const tasks = [
            task({ id: 'A-1', title: 'beta', reporter: 'zee' }),
            task({ id: 'A-2', title: 'alpha', reporter: null }),
            task({ id: 'A-3', title: 'gamma', reporter: 'aye' }),
        ]
        expect(sortTasks(tasks, 'title', 'asc').map((t) => t.id)).toEqual(['A-2', 'A-1', 'A-3'])
        // Reporter uses Option ordering: missing first in asc.
        expect(sortTasks(tasks, 'reporter', 'asc').map((t) => t.id)).toEqual(['A-2', 'A-3', 'A-1'])
    })

    it('sorts tags as a lex-ordered string vector with empties first', () => {
        const tasks = [
            task({ id: 'A-1', tags: ['b'] }),
            task({ id: 'A-2', tags: [] }),
            task({ id: 'A-3', tags: ['a', 'z'] }),
            task({ id: 'A-4', tags: ['a'] }),
        ]
        // Vec<String> Ord: [] < ['a'] < ['a','z'] < ['b'].
        expect(sortTasks(tasks, 'tags', 'asc').map((t) => t.id)).toEqual(['A-2', 'A-4', 'A-3', 'A-1'])
    })

    it('sorts sprints as a lex-ordered number vector with empties first', () => {
        const tasks = [
            task({ id: 'A-1', sprints: [10] }),
            task({ id: 'A-2', sprints: [] }),
            task({ id: 'A-3', sprints: [2, 9] }),
            task({ id: 'A-4', sprints: [2] }),
        ]
        // Vec<u32> Ord: [] < [2] < [2,9] < [10] (numeric, not lexical string).
        expect(sortTasks(tasks, 'sprints', 'asc').map((t) => t.id)).toEqual(['A-2', 'A-4', 'A-3', 'A-1'])
    })

    it('compares RFC3339 timestamps as instants at nanosecond precision', () => {
        const tasks = [
            task({ id: 'A-1', modified: '2026-01-01T10:00:00.123456789Z' }),
            task({ id: 'A-2', modified: '2026-01-01T10:00:00.123456790Z' }),
            task({ id: 'A-3', modified: '2026-01-01T10:00:00.12345678Z' }),
        ]
        // .123456780 < .123456789 < .123456790 — sub-millisecond fractions ordered.
        expect(sortTasks(tasks, 'modified', 'asc').map((t) => t.id)).toEqual(['A-3', 'A-1', 'A-2'])
    })

    it('treats equal instants expressed with different offsets as ties broken by ID', () => {
        const tasks = [
            task({ id: 'A-2', modified: '2026-01-01T12:00:00+02:00' }),
            task({ id: 'A-1', modified: '2026-01-01T10:00:00Z' }),
        ]
        expect(sortTasks(tasks, 'modified', 'asc').map((t) => t.id)).toEqual(['A-1', 'A-2'])
        expect(sortTasks(tasks, 'modified', 'desc').map((t) => t.id)).toEqual(['A-1', 'A-2'])
    })

    it('falls back to lexical order when a timestamp has no RFC3339 offset', () => {
        const tasks = [
            task({ id: 'A-1', modified: '2026-01-02 10:00:00' }),
            task({ id: 'A-2', modified: '2026-01-01T23:00:00Z' }),
        ]
        // The naive value is unparseable as an instant; the RFC3339 one sorts first.
        expect(compareTasks(tasks[0]!, tasks[1]!, 'modified')).toBeGreaterThan(0)
    })

    it('orders due dates with mixed formats: instants beat date-only by UTC-midnight approximation', () => {
        const tasks = [
            task({ id: 'A-1', due_date: '2026-01-05T10:00:00Z' }),
            task({ id: 'A-2', due_date: '2026-01-05' }),
            task({ id: 'A-3', due_date: '2026-01-06' }),
            task({ id: 'A-4', due_date: '2026-01-04T23:00:00-02:00' }),
        ]
        // A-4 is 2026-01-05T01:00Z after applying its -02:00 offset.
        expect(sortTasks(tasks, 'due', 'asc').map((t) => t.id)).toEqual(['A-2', 'A-4', 'A-1', 'A-3'])
    })

    it('does not mutate the input array', () => {
        const tasks = [task({ id: 'A-2' }), task({ id: 'A-1' })]
        const snapshot = tasks.map((t) => t.id)
        sortTasks(tasks, 'id', 'asc')
        expect(tasks.map((t) => t.id)).toEqual(snapshot)
    })
})
