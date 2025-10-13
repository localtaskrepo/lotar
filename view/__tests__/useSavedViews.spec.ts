import { beforeEach, describe, expect, it } from 'vitest'
import { useSavedViews } from '../composables/useSavedViews'

describe('useSavedViews', () => {
    const STORAGE_KEY = 'lotar.savedViews'

    beforeEach(() => {
        localStorage.clear()
    })

    it('creates, persists, refreshes, and removes views', () => {
        const { views, saveNew, updateExisting, remove, refresh, getById } = useSavedViews()
        expect(views.value).toHaveLength(0)

        const created = saveNew('My view', { project: 'PRJ', status: 'Open' }, { columns: ['id', 'title'], sort: { key: 'modified', dir: 'desc' } })
        expect(views.value).toHaveLength(1)
        expect(views.value[0].name).toBe('My view')
        expect(getById(created.id)?.filter.project).toBe('PRJ')

        updateExisting(created.id, { name: 'Renamed', filter: { project: 'APP' } })
        const updated = getById(created.id)
        expect(updated?.name).toBe('Renamed')
        expect(updated?.filter.project).toBe('APP')

        remove(created.id)
        expect(views.value).toHaveLength(0)
        expect(JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]')).toHaveLength(0)

        const seeded = [{ id: 'external', name: 'External', filter: { priority: 'High' }, created: 'now', updated: 'now' }]
        localStorage.setItem(STORAGE_KEY, JSON.stringify(seeded))
        refresh()
        expect(views.value).toHaveLength(1)
        expect(views.value[0].id).toBe('external')
        expect(views.value[0].filter.priority).toBe('High')
    })

    it('clones filter and extras to avoid shared references', () => {
        const { saveNew, getById, updateExisting } = useSavedViews()

        const filter = { status: 'Open' }
        const created = saveNew('Clone check', filter, { columns: ['id'], sort: { key: 'priority', dir: 'asc' } })
        filter.status = 'Closed'

        const stored = getById(created.id)
        expect(stored?.filter.status).toBe('Open')
        expect(stored?.columns).toEqual(['id'])
        expect(stored?.sort?.dir).toBe('asc')

        const updatePayload = { filter: { status: 'Doing' }, columns: ['title', 'assignee'], sort: { key: 'modified', dir: 'desc' as const } }
        updateExisting(created.id, updatePayload)
        updatePayload.filter!.status = 'Done'
        updatePayload.columns!.push('tags')

        const afterUpdate = getById(created.id)
        expect(afterUpdate?.filter.status).toBe('Doing')
        expect(afterUpdate?.columns).toEqual(['title', 'assignee'])
        expect(afterUpdate?.sort?.dir).toBe('desc')
    })
})
