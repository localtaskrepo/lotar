import { afterEach, describe, expect, it, vi } from 'vitest'
import { api, describeJobOutcome } from '../api/client'
import { dismissToast, toasts } from '../components/toast'

describe('changes held back for a running agent job (DEV-320)', () => {
    afterEach(() => {
        vi.unstubAllGlobals()
        for (const toast of [...toasts.value]) dismissToast(toast.id)
    })

    it('describes each field like the CLI does', () => {
        expect(describeJobOutcome({ job_id: 'job-1', status: 'HelpNeeded', assignee: 'ravi' })).toEqual([
            'HelpNeeded will be set when job job-1 ends',
            'ravi will be assigned when job job-1 ends',
        ])
        expect(describeJobOutcome({ job_id: 'job-1', assignee: '' })).toEqual([
            'The assignee will be cleared when job job-1 ends',
        ])
    })

    it('announces a held-back status change in a toast', async () => {
        const task = { id: 'DEV-1', status: 'InProgress' }
        vi.stubGlobal(
            'fetch',
            vi.fn(
                async () =>
                    new Response(JSON.stringify({ data: task, deferred: { job_id: 'job-1', status: 'HelpNeeded' } }), {
                        status: 200,
                    }),
            ),
        )
        const result = await api.setStatus('DEV-1', 'HelpNeeded')
        expect(result).toEqual(task)
        expect(toasts.value.map((toast) => toast.message)).toEqual(['HelpNeeded will be set when job job-1 ends'])
    })

    it('stays quiet for an applied change', async () => {
        vi.stubGlobal(
            'fetch',
            vi.fn(async () => new Response(JSON.stringify({ data: { id: 'DEV-1' } }), { status: 200 })),
        )
        await api.updateTask('DEV-1', { status: 'Done' })
        expect(toasts.value).toEqual([])
    })
})
