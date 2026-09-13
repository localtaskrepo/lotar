import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'

vi.mock('../api/client', () => ({
    api: {
        activityFeed: vi.fn(),
    },
}))

function deferred<T>() {
    let resolve!: (value: T) => void
    let reject!: (error: Error) => void
    const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
    return { promise, resolve, reject }
}

function feedItem(id: string, task: string): any {
    return {
        commit: `sha-${id}`,
        author: 'Ada Lovelace',
        email: 'ada@example.com',
        date: '2026-09-13T10:00:00.000Z',
        message: `commit ${id}`,
        task_id: task,
        task_title: `Task ${task}`,
        history: [],
    }
}

describe('useActivity keyed feed queries', () => {
    let feedMock: Mock
    let useActivity: typeof import('../composables/useActivity')['useActivity']
    let _resetActivityStore: typeof import('../composables/useActivity')['_resetActivityStore']

    beforeEach(async () => {
        vi.resetModules()
        const client = await import('../api/client')
        const mod = await import('../composables/useActivity')
        feedMock = client.api.activityFeed as unknown as Mock
        useActivity = mod.useActivity
        _resetActivityStore = mod._resetActivityStore
        _resetActivityStore()
    })

    afterEach(() => {
        _resetActivityStore()
        vi.clearAllMocks()
    })

    it('loads feed items through a scoped handle with a computed window', async () => {
        feedMock.mockResolvedValue([feedItem('a', 'T-1')])

        const handle = useActivity().getFeedQuery({ project: 'proj-123', windowDays: 14, limit: 50 })
        expect(handle.hasSnapshot.value).toBe(false)

        await handle.refresh()

        expect(feedMock).toHaveBeenCalledTimes(1)
        const params = feedMock.mock.calls[0]![0] as Record<string, unknown>
        expect(params.project).toBe('proj-123')
        expect(params.limit).toBe(50)
        expect(String(params.since)).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/)
        expect(String(params.until)).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/)
        expect(new Date(params.since as string).getTime()).toBeLessThanOrEqual(new Date(params.until as string).getTime())
        expect(handle.items.value).toHaveLength(1)
        expect(handle.items.value[0]!.task_id).toBe('T-1')
        expect(handle.error.value).toBeNull()
        expect(handle.loading.value).toBe(false)
        expect(handle.hasSnapshot.value).toBe(true)
        handle.release()
    })

    it('captures first-load failures without inventing data', async () => {
        feedMock.mockRejectedValueOnce(new Error('network down'))

        const handle = useActivity().getFeedQuery({ windowDays: 30, limit: 200 })
        await handle.refresh()

        expect(handle.items.value).toEqual([])
        expect(handle.error.value).toBe('network down')
        expect(handle.loading.value).toBe(false)
        expect(handle.hasSnapshot.value).toBe(false)
        handle.release()
    })

    it('preserves the last snapshot when a background refresh fails', async () => {
        feedMock.mockResolvedValueOnce([feedItem('a', 'T-1')])
        feedMock.mockRejectedValueOnce(new Error('flaky refresh'))

        const handle = useActivity().getFeedQuery({ windowDays: 30, limit: 200 })
        await handle.refresh()
        await handle.refresh()

        expect(handle.items.value).toHaveLength(1)
        expect(handle.error.value).toBe('flaky refresh')
        expect(handle.loading.value).toBe(false)
        handle.release()
    })

    it('lets the latest overlapping refresh on the same key win, in any completion order', async () => {
        const first = deferred<any[]>()
        const second = deferred<any[]>()
        feedMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)

        const handle = useActivity().getFeedQuery({ windowDays: 30, limit: 200 })
        const pendingFirst = handle.refresh()
        expect(handle.loading.value).toBe(true)
        const pendingSecond = handle.refresh()

        second.resolve([feedItem('new', 'T-2')])
        await pendingSecond
        expect(handle.items.value.map(item => item.commit)).toEqual(['sha-new'])
        expect(handle.loading.value).toBe(false)

        first.resolve([feedItem('old', 'T-1')])
        await pendingFirst
        expect(handle.items.value.map(item => item.commit)).toEqual(['sha-new'])
        expect(handle.loading.value).toBe(false)
        handle.release()
    })

    it('drops a stale failure after a newer refresh succeeded', async () => {
        const first = deferred<any[]>()
        const second = deferred<any[]>()
        feedMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)

        const handle = useActivity().getFeedQuery({ windowDays: 30, limit: 200 })
        const pendingFirst = handle.refresh()
        const pendingSecond = handle.refresh()
        second.resolve([feedItem('new', 'T-2')])
        await pendingSecond
        first.reject(new Error('old request failed'))
        await pendingFirst

        expect(handle.items.value.map(item => item.commit)).toEqual(['sha-new'])
        expect(handle.error.value).toBeNull()
        expect(handle.loading.value).toBe(false)
        handle.release()
    })

    it('isolates concurrent queries with different scopes (drawer vs insights)', async () => {
        const drawerRequest = deferred<any[]>()
        const insightsRequest = deferred<any[]>()
        feedMock.mockImplementation((params: Record<string, unknown>) =>
            params.limit === 200 ? drawerRequest.promise : insightsRequest.promise)

        const drawer = useActivity().getFeedQuery({ windowDays: 30, limit: 200 })
        const insights = useActivity().getFeedQuery({ project: 'DEV', windowDays: 60, limit: 400 })
        expect(drawer.key).not.toBe(insights.key)

        const pendingDrawer = drawer.refresh()
        const pendingInsights = insights.refresh()

        insightsRequest.resolve([feedItem('i', 'DEV-1')])
        await pendingInsights
        expect(insights.items.value).toHaveLength(1)
        expect(drawer.items.value).toEqual([])
        expect(drawer.loading.value).toBe(true)

        drawerRequest.resolve([feedItem('d', 'T-9')])
        await pendingDrawer
        expect(drawer.items.value).toHaveLength(1)
        expect(insights.items.value.map(item => item.task_id)).toEqual(['DEV-1'])
        drawer.release()
        insights.release()
    })

    it('publishes nothing from in-flight refreshes after release', async () => {
        const request = deferred<any[]>()
        feedMock.mockReturnValueOnce(request.promise)

        const handle = useActivity().getFeedQuery({ windowDays: 30, limit: 200 })
        const pending = handle.refresh()
        handle.release()
        request.resolve([feedItem('late', 'T-1')])
        await pending

        expect(handle.items.value).toEqual([])
        expect(handle.loading.value).toBe(false)
        expect(handle.hasSnapshot.value).toBe(false)
    })

    it('exposes no stale rows or errors for a freshly adopted key', async () => {
        feedMock.mockResolvedValue([feedItem('a', 'T-1')])
        const first = useActivity().getFeedQuery({ project: 'A', windowDays: 30, limit: 400 })
        await first.refresh()
        first.release()

        const second = useActivity().getFeedQuery({ project: 'B', windowDays: 30, limit: 400 })
        expect(second.items.value).toEqual([])
        expect(second.error.value).toBeNull()
        expect(second.loading.value).toBe(false)
        expect(second.hasSnapshot.value).toBe(false)
        second.release()
    })

    it('normalizes project whitespace consistently for the key and the request', async () => {
        feedMock.mockResolvedValue([feedItem('a', 'T-1')])

        const padded = useActivity().getFeedQuery({ project: '  A  ', windowDays: 30, limit: 100 })
        const plain = useActivity().getFeedQuery({ project: 'A', windowDays: 30, limit: 100 })
        expect(plain.key).toBe(padded.key)

        await padded.refresh()
        expect((feedMock.mock.calls[0]![0] as Record<string, unknown>).project).toBe('A')
        expect(plain.hasSnapshot.value).toBe(true)
        padded.release()
        plain.release()
    })

    it('never treats a literal * project as the global feed', async () => {
        feedMock.mockImplementation((params: Record<string, unknown>) => {
            if ('project' in params) return Promise.reject(new Error('invalid project'))
            return Promise.resolve([feedItem('g', 'T-0')])
        })

        const star = useActivity().getFeedQuery({ project: '*', windowDays: 30, limit: 100 })
        const global = useActivity().getFeedQuery({ windowDays: 30, limit: 100 })
        expect(star.key).not.toBe(global.key)

        await star.refresh()
        expect(star.error.value).toBe('invalid project')
        expect(star.items.value).toEqual([])

        await global.refresh()
        const params = feedMock.mock.calls[1]![0] as Record<string, unknown>
        expect(params).not.toHaveProperty('project')
        expect(global.items.value).toHaveLength(1)
        expect(global.error.value).toBeNull()
        star.release()
        global.release()
    })

    it('keeps the local activity log API for touch consumers', () => {
        const activity = useActivity()
        expect([...activity.items]).toEqual([])
        expect({ ...activity.touches }).toEqual({})
        activity.add({ kind: 'info', message: 'hello' })
        expect(activity.items).toHaveLength(1)
        activity.markTaskTouch({ id: 'T-1', kind: 'updated' })
        expect(activity.touches['T-1']!.kind).toBe('updated')
    })
})
