import { describe, expect, it } from 'vitest'
import {
    computeBoardLanes,
    migrateLegacySettings,
    parseBoardLayout,
    reconcileColumns,
    type BoardColumnConfig,
} from '../composables/useBoardLayout'

const STATUSES = ['Todo', 'Doing', 'Review', 'QA', 'Done', 'WontDo'].map((label) => ({
    key: label.toLowerCase(),
    label,
}))
const col = (key: string, synthetic = false) => ({ key, label: key, synthetic })
const keys = (columns: BoardColumnConfig[]) => columns.map((column) => column.statuses.join('+'))

describe('board layout model (DEV-130)', () => {
    it('reconciles a stored layout with the configured statuses', () => {
        const stored: BoardColumnConfig[] = [
            { statuses: ['done', 'wontdo'], name: 'Closed', wipLimit: 3 },
            { statuses: ['todo', 'gone'] },
            { statuses: ['removed'] },
            { statuses: ['todo'] },
        ]
        // Unknown keys and empty columns drop, duplicates keep their first spot,
        // missing statuses land right after their configured predecessor.
        expect(keys(reconcileColumns(stored, STATUSES))).toEqual(['done+wontdo', 'todo', 'doing', 'review', 'qa'])
        expect(reconcileColumns(stored, STATUSES)[0]).toMatchObject({ name: 'Closed', wipLimit: 3 })
        expect(keys(reconcileColumns([], STATUSES))).toEqual(['todo', 'doing', 'review', 'qa', 'done', 'wontdo'])
    })

    it('places a newly configured status next to its configured neighbour', () => {
        const stored: BoardColumnConfig[] = [
            { statuses: ['todo'] },
            { statuses: ['review', 'doing'] },
            { statuses: ['done'] },
        ]
        const withBlocked = [
            STATUSES[0]!,
            STATUSES[1]!,
            { key: 'blocked', label: 'Blocked' },
            STATUSES[2]!,
            STATUSES[4]!,
        ]
        expect(keys(reconcileColumns(stored, withBlocked))).toEqual(['todo', 'review+doing', 'blocked', 'done'])
    })

    it('arranges lanes in layout order with any statuses stacked and hidden ones left out', () => {
        const columns = [...STATUSES.map((s) => col(s.key)), col('__other__', true)]
        const lanes = computeBoardLanes(columns, {
            columns: [
                { statuses: ['todo', 'qa'] },
                { statuses: ['doing'] },
                { statuses: ['wontdo', 'done'], name: 'Closed' },
                { statuses: ['review'] },
            ],
            hidden: ['review', 'qa'],
        })
        expect(lanes.map((lane) => lane.key)).toEqual(['todo', 'doing', 'wontdo+done', '__other__'])
        expect(lanes.map((lane) => lane.title)).toEqual(['todo', 'doing', 'Closed', '__other__'])
        expect(lanes.map((lane) => lane.stacked)).toEqual([false, false, true, false])
    })

    it('migrates stacking, hiding, WIP limits and Done-column rules from the earlier settings', () => {
        const layout = migrateLegacySettings(STATUSES, {
            stack: { hidden: ['doing'], stacked: ['qa', 'wontdo'] },
            wip: { Review: 2, QA: 1, Todo: 5, Doing: 'x' },
            done: { statuses: ['Done'], maxAgeDays: 14, maxVisible: '20' },
        })
        expect(keys(layout.columns)).toEqual(['todo', 'doing', 'review+qa', 'done+wontdo'])
        expect(layout.hidden).toEqual(['doing'])
        expect(layout.columns.map((column) => column.wipLimit)).toEqual([5, undefined, 3, undefined])
        expect(layout.columns[3]).toMatchObject({ maxAgeDays: 14, maxVisible: 20 })
        expect(layout.columns[0]!.maxAgeDays).toBeUndefined()
        // Done statuses without limits never create card rules.
        const plain = migrateLegacySettings(STATUSES, { done: { statuses: ['Done'] } })
        expect(plain.columns.some((column) => column.maxAgeDays || column.maxVisible)).toBe(false)
    })

    it('only accepts the current storage format and cleans malformed values', () => {
        expect(parseBoardLayout(null)).toBeNull()
        expect(parseBoardLayout({ hidden: [], stacked: ['qa'] })).toBeNull()
        expect(
            parseBoardLayout({
                version: 2,
                columns: [
                    { statuses: ['done', 3, ''], name: '  ', wipLimit: -1, maxVisible: '4' },
                    { statuses: [] },
                    'bad',
                ],
                hidden: 'qa',
            }),
        ).toEqual({ columns: [{ statuses: ['done'], maxVisible: 4 }], hidden: [] })
    })
})
