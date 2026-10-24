import { ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useColumnDragReorder } from '../composables/useColumnDragReorder'
import type { ColKey } from '../composables/useColumns'

const TASKS_MIME = 'application/x-lotar-col'
const SPRINTS_MIME = 'application/x-lotar-sprints-col'

function makeDataTransfer(initial: Record<string, string> = {}) {
    const store: Record<string, string> = { ...initial }
    return {
        effectAllowed: 'none',
        setData: (type: string, value: string) => {
            store[type] = value
        },
        getData: (type: string) => store[type] || '',
        __store: store,
    }
}

function makeEvent(options: { dataTransfer?: any; clientX?: number; currentTarget?: any } = {}) {
    return {
        dataTransfer: options.dataTransfer,
        clientX: options.clientX ?? 0,
        currentTarget: 'currentTarget' in options ? options.currentTarget : null,
    } as any
}

function makeTarget(rect?: { left: number; width: number }) {
    if (!rect) return {} as any
    return { getBoundingClientRect: () => ({ left: rect.left, width: rect.width }) } as any
}

describe('useColumnDragReorder', () => {
    beforeEach(() => {
        vi.useFakeTimers({ toFake: ['Date'] })
        vi.setSystemTime(new Date('2026-01-01T00:00:00Z'))
    })
    afterEach(() => {
        vi.useRealTimers()
    })

    function setup(mime = TASKS_MIME, order: ColKey[] = ['id', 'title', 'status', 'priority', 'reporter', 'modified']) {
        const columnOrder = ref<ColKey[]>([...order])
        const onSort = vi.fn()
        const dnd = useColumnDragReorder({ mime, columnOrder, onSort })
        return { columnOrder, onSort, dnd }
    }

    it('moves a column before the drop target', () => {
        const { columnOrder, dnd } = setup()
        dnd.moveColumn('priority', 'title', 'before')
        expect(columnOrder.value).toEqual(['id', 'priority', 'title', 'status', 'reporter', 'modified'])
    })

    it('moves a column after the drop target', () => {
        const { columnOrder, dnd } = setup()
        dnd.moveColumn('id', 'status', 'after')
        expect(columnOrder.value).toEqual(['title', 'status', 'id', 'priority', 'reporter', 'modified'])
    })

    it('ignores no-op and unknown-target moves without replacing the order array', () => {
        const { columnOrder, dnd } = setup()
        const original = columnOrder.value
        dnd.moveColumn('status', 'status', 'after')
        dnd.moveColumn('title', 'custom:Missing' as ColKey, 'before')
        expect(columnOrder.value).toBe(original)
    })

    it('keeps slots for columns that are not currently visible', () => {
        const { columnOrder, dnd } = setup()
        dnd.moveColumn('modified', 'id', 'before')
        expect(columnOrder.value.filter((c) => c === 'reporter')).toEqual(['reporter'])
        expect(columnOrder.value).toHaveLength(6)
        expect(columnOrder.value).toEqual(['modified', 'id', 'title', 'status', 'priority', 'reporter'])
    })

    it('tags drag payloads with the parameterized MIME plus text/plain', () => {
        for (const mime of [TASKS_MIME, SPRINTS_MIME]) {
            const { dnd } = setup(mime)
            const dt = makeDataTransfer()
            dnd.onColDragStart('priority', makeEvent({ dataTransfer: dt }))
            expect(dt.effectAllowed).toBe('move')
            expect(dt.__store[mime]).toBe('priority')
            expect(dt.__store['text/plain']).toBe('priority')
            expect(dnd.draggingCol.value).toBe('priority')
            expect(dnd.dragOverCol.value).toBeNull()
            expect(dnd.dragOverPos.value).toBe('before')
        }
    })

    it('reads its own MIME first and falls back to text/plain on drop', () => {
        const { columnOrder, dnd } = setup()

        // Own MIME wins over the text/plain fallback.
        const ownMime = makeDataTransfer({ [TASKS_MIME]: 'priority', 'text/plain': 'title' })
        dnd.onColDragOver(
            'status',
            makeEvent({ dataTransfer: ownMime, currentTarget: makeTarget({ left: 100, width: 200 }), clientX: 0 }),
        )
        dnd.onColDrop('status', makeEvent({ dataTransfer: ownMime }))
        expect(columnOrder.value.indexOf('priority')).toBeLessThan(columnOrder.value.indexOf('status'))
        expect(columnOrder.value.indexOf('title')).toBeLessThan(columnOrder.value.indexOf('status'))

        // A foreign table's MIME tag alone is not accepted...
        const foreignOnly = makeDataTransfer({ [SPRINTS_MIME]: 'modified' })
        const afterForeign = columnOrder.value
        dnd.onColDrop('id', makeEvent({ dataTransfer: foreignOnly }))
        expect(columnOrder.value).toBe(afterForeign)

        // ...but the shared text/plain fallback still carries the drag.
        const plainOnly = makeDataTransfer({ 'text/plain': 'modified' })
        dnd.onColDragOver(
            'id',
            makeEvent({ dataTransfer: plainOnly, currentTarget: makeTarget({ left: 100, width: 200 }), clientX: 0 }),
        )
        dnd.onColDrop('id', makeEvent({ dataTransfer: plainOnly }))
        expect(columnOrder.value.indexOf('modified')).toBeLessThan(columnOrder.value.indexOf('id'))

        const empty = makeDataTransfer()
        const before = columnOrder.value
        dnd.onColDrop('title', makeEvent({ dataTransfer: empty }))
        expect(columnOrder.value).toBe(before)
    })

    it('computes before/after from the pointer position against the target midpoint', () => {
        const { dnd } = setup()
        const target = makeTarget({ left: 100, width: 200 }) // midpoint x = 200

        dnd.onColDragOver('title', makeEvent({ currentTarget: target, clientX: 199 }))
        expect(dnd.dragOverCol.value).toBe('title')
        expect(dnd.dragOverPos.value).toBe('before')

        dnd.onColDragOver('title', makeEvent({ currentTarget: target, clientX: 200 }))
        expect(dnd.dragOverPos.value).toBe('before')

        dnd.onColDragOver('title', makeEvent({ currentTarget: target, clientX: 201 }))
        expect(dnd.dragOverPos.value).toBe('after')

        // Missing target or zero-width rect falls back to 'before'.
        dnd.onColDragOver('title', makeEvent({ currentTarget: null, clientX: 999 }))
        expect(dnd.dragOverPos.value).toBe('before')
        dnd.onColDragOver('title', makeEvent({ currentTarget: makeTarget(), clientX: 999 }))
        expect(dnd.dragOverPos.value).toBe('before')
        dnd.onColDragOver('title', makeEvent({ currentTarget: makeTarget({ left: 0, width: 0 }), clientX: 0 }))
        expect(dnd.dragOverPos.value).toBe('before')
    })

    it('resets indicator state on drop and swallows the next click for 250ms', () => {
        const { onSort, dnd } = setup()
        const dt = makeDataTransfer()
        dnd.onColDragStart('priority', makeEvent({ dataTransfer: dt }))
        dnd.onColDragOver(
            'status',
            makeEvent({ dataTransfer: dt, currentTarget: makeTarget({ left: 100, width: 200 }) }),
        )
        dnd.onColDrop('status', makeEvent({ dataTransfer: dt }))

        expect(dnd.draggingCol.value).toBeNull()
        expect(dnd.dragOverCol.value).toBeNull()
        expect(dnd.dragOverPos.value).toBe('before')

        dnd.onHeaderActivate('title')
        expect(onSort).not.toHaveBeenCalled()

        vi.setSystemTime(new Date(Date.now() + 249))
        dnd.onHeaderActivate('title')
        expect(onSort).not.toHaveBeenCalled()

        vi.setSystemTime(new Date(Date.now() + 1))
        dnd.onHeaderActivate('title')
        expect(onSort).toHaveBeenCalledTimes(1)
        expect(onSort).toHaveBeenCalledWith('title')
    })

    it('aborts cleanly on dragend without reordering and keeps the click guard armed', () => {
        const { columnOrder, onSort, dnd } = setup()
        const dt = makeDataTransfer()
        dnd.onColDragStart('priority', makeEvent({ dataTransfer: dt }))
        dnd.onColDragOver(
            'status',
            makeEvent({ dataTransfer: dt, currentTarget: makeTarget({ left: 100, width: 200 }) }),
        )
        dnd.onColDragEnd()

        expect(dnd.draggingCol.value).toBeNull()
        expect(dnd.dragOverCol.value).toBeNull()
        expect(dnd.dragOverPos.value).toBe('before')
        expect(columnOrder.value.indexOf('priority')).toBeGreaterThan(columnOrder.value.indexOf('status'))

        dnd.onHeaderActivate('status')
        expect(onSort).not.toHaveBeenCalled()
        vi.setSystemTime(new Date(Date.now() + 250))
        dnd.onHeaderActivate('status')
        expect(onSort).toHaveBeenCalledWith('status')
    })
})
