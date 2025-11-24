import { describe, expect, it } from 'vitest';
import type { TaskDTO } from '../api/types';
import { findLastStatusChangeAt } from '../utils/taskHistory';

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
