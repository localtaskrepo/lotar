import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'

vi.mock('../api/client', () => ({
    api: {
        activityFeed: vi.fn(),
    },
}))

describe('useActivity feed integration', () => {
    beforeEach(() => {
        vi.resetModules()
    })

    afterEach(() => {
        vi.clearAllMocks()
    })

    it('loads feed items from the API', async () => {
        const sample = [
            {
                id: 'commit-1',
                date: '2024-06-01T10:00:00.000Z',
                author: 'Ada Lovelace',
                summary: 'Add new workflow',
                commits: [
                    {
                        sha: 'abc123',
                        message: 'feat: add activity feed',
                        url: 'https://example.com/repo/commit/abc123',
                        additions: 10,
                        deletions: 2,
                        files: 3,
                        task_history: [
                            {
                                ticket_id: 'PROJ-1',
                                ticket_summary: 'Improve tracking',
                                kind: 'update',
                                field: 'status',
                                previous: 'todo',
                                current: 'in-progress',
                            },
                        ],
                    },
                ],
            },
        ]

        const { api } = await import('../api/client')
        const feedMock = api.activityFeed as unknown as Mock
        feedMock.mockResolvedValue(sample)

        const { useActivity } = await import('../composables/useActivity')
        const { feed, feedError, feedLoading, refreshFeed } = useActivity()

        expect(feed.value).toEqual([])

        await refreshFeed({ project: 'proj-123', limit: 10 })

        expect(feedMock).toHaveBeenCalledWith({ project: 'proj-123', limit: 10 })
        expect(feed.value).toEqual(sample)
        expect(feedError.value).toBeNull()
        expect(feedLoading.value).toBe(false)
    })

    it('captures failures and clears feed', async () => {
        const { api } = await import('../api/client')
        const feedMock = api.activityFeed as unknown as Mock
        feedMock.mockRejectedValueOnce(new Error('network down'))

        const { useActivity } = await import('../composables/useActivity')
        const { feed, feedError, feedLoading, refreshFeed } = useActivity()

        await refreshFeed({ project: 'proj-999' })

        expect(feedMock).toHaveBeenCalledWith({ project: 'proj-999' })
        expect(feed.value).toEqual([])
        expect(feedError.value).toBe('network down')
        expect(feedLoading.value).toBe(false)
    })
})
