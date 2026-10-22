import { describe, expect, it } from 'vitest';
import type { TaskDTO } from '../api/types';
import { findLastStatusChangeAt, statusEnteredAt } from '../utils/taskHistory';

function makeTask(history: Array<{ at: string; old?: string | null; new?: string | null }> = []): TaskDTO {
    return {
        id: 'FOO-1',
        title: 'Example',
        status: 'Todo',
        priority: 'P1',
        task_type: 'Bug',
        created: new Date().toISOString(),
        modified: new Date().toISOString(),
        tags: [],
        relationships: {},
        comments: [],
        references: [],
        sprints: [],
        history: history.map((entry) => ({
            at: entry.at,
            changes: [{ field: 'status', old: entry.old ?? null, new: entry.new ?? null }],
        })),
        custom_fields: {},
    }
}

describe('findLastStatusChangeAt', () => {
    it('returns null when history is empty', () => {
        const task = makeTask([])
        expect(findLastStatusChangeAt(task, 'Done')).toBeNull()
    })

    it('returns the newest timestamp for the requested status', () => {
        const task = makeTask([
            { at: '2024-01-01T00:00:00Z', new: 'In Progress' },
            { at: '2024-01-05T00:00:00Z', new: 'Done' },
            { at: '2024-01-10T00:00:00Z', new: 'Reopen' },
            { at: '2024-01-12T00:00:00Z', new: 'Done' },
        ])
        const ts = findLastStatusChangeAt(task, 'Done')
        expect(ts).toBe(Date.parse('2024-01-12T00:00:00Z'))
    })

    it('falls back when status is omitted', () => {
        const task = makeTask([
            { at: '2024-02-01T00:00:00Z', new: 'Review' },
            { at: '2024-02-05T00:00:00Z', new: 'Done' },
        ])
        const ts = findLastStatusChangeAt(task)
        expect(ts).toBe(Date.parse('2024-02-05T00:00:00Z'))
    })

    it('ignores unparsable timestamps', () => {
        const task = makeTask([
            { at: 'invalid-date', new: 'Done' },
            { at: '2024-03-01T00:00:00Z', new: 'Done' },
        ])
        const ts = findLastStatusChangeAt(task, 'Done')
        expect(ts).toBe(Date.parse('2024-03-01T00:00:00Z'))
    })
})

describe('statusEnteredAt (DEV-133)', () => {
    const base = (overrides: Partial<TaskDTO>, history: TaskDTO['history'] = []): TaskDTO => ({
        ...makeTask([]),
        created: '2026-01-01T00:00:00Z',
        status: 'Done',
        history,
        ...overrides,
    })
    const statusChange = (at: string, value: string) => ({ at, changes: [{ field: 'status', old: null, new: value }] })

    it('uses the latest change into the status, ignoring case and separators', () => {
        const task = base({}, [
            statusChange('2026-01-02T00:00:00Z', 'Done'),
            statusChange('2026-01-03T00:00:00Z', 'In Progress'),
            statusChange('2026-01-09T00:00:00Z', 'done'),
        ])
        expect(statusEnteredAt(task, 'Done')).toBe(Date.parse('2026-01-09T00:00:00Z'))
        expect(statusEnteredAt(base({ status: 'InProgress' }, [statusChange('2026-01-04T00:00:00Z', 'in_progress')]), 'InProgress'))
            .toBe(Date.parse('2026-01-04T00:00:00Z'))
    })

    it('falls back to creation time when the task never changed status', () => {
        const task = base({}, [{ at: '2026-01-05T00:00:00Z', changes: [{ field: 'title', old: 'a', new: 'b' }] }])
        expect(statusEnteredAt(task, 'Done')).toBe(Date.parse('2026-01-01T00:00:00Z'))
        expect(statusEnteredAt(base({}, []), 'Done')).toBe(Date.parse('2026-01-01T00:00:00Z'))
    })

    it('uses the latest status change when none targets the current label', () => {
        const task = base({ status: 'Shipped' }, [
            statusChange('2026-01-02T00:00:00Z', 'Review'),
            statusChange('2026-01-06T00:00:00Z', 'Released'),
        ])
        expect(statusEnteredAt(task, 'Shipped')).toBe(Date.parse('2026-01-06T00:00:00Z'))
    })

    it('returns null when nothing can be dated', () => {
        expect(statusEnteredAt(base({ created: 'not a date' }, []), 'Done')).toBeNull()
        expect(statusEnteredAt(null, 'Done')).toBeNull()
    })
})
