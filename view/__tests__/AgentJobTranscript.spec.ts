import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentJobTranscript } from '../api/types'
import AgentJobTranscriptView from '../components/AgentJobTranscript.vue'

vi.mock('../api/client', () => ({
    api: { getAgentJobTranscript: vi.fn() },
}))

const { api } = await import('../api/client')
const getTranscript = vi.mocked(api.getAgentJobTranscript)

const page = (overrides: Partial<AgentJobTranscript>): { transcript: AgentJobTranscript } => ({
    transcript: {
        state: 'ok',
        source: 'reference',
        session: 'lotar-1-DEV-1-fix',
        agent: 'claude',
        turns: [2],
        session_turns: 2,
        tool_events: true,
        events: [],
        ...overrides,
    },
})

const entry = (sequence: number, kind: string, message: string) => ({
    kind,
    sequence,
    at: '2026-11-26T10:00:00Z',
    message,
})

async function open(wrapper: ReturnType<typeof mount>) {
    await wrapper.get('button').trigger('click')
    await flushPromises()
}

describe('AgentJobTranscript', () => {
    beforeEach(() => {
        getTranscript.mockReset()
    })

    it('loads nothing until opened, then the turn and the next page on Load more', async () => {
        getTranscript
            .mockResolvedValueOnce(page({ events: [entry(2, 'agent_job_input', 'Rework it')], next_after: 2 }))
            .mockResolvedValueOnce(page({ events: [entry(3, 'agent_job_message', 'Reworked.')] }))
        const wrapper = mount(AgentJobTranscriptView, { props: { jobId: 'job-1' } })
        await flushPromises()
        expect(getTranscript).not.toHaveBeenCalled()

        await open(wrapper)
        expect(getTranscript).toHaveBeenCalledWith('job-1', undefined)
        expect(wrapper.text()).toContain('Turn 2 of 2 in the claude session lotar-1-DEV-1-fix')
        expect(wrapper.text()).toContain('Rework it')
        expect(wrapper.text()).not.toContain('Tool calls are in T3')

        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Load more')!
            .trigger('click')
        await flushPromises()
        expect(getTranscript).toHaveBeenLastCalledWith('job-1', 2)
        expect(wrapper.text()).toContain('Rework it')
        expect(wrapper.text()).toContain('Reworked.')
        expect(wrapper.findAll('button').some((button) => button.text() === 'Load more')).toBe(false)

        // Closing and opening again keeps what was read.
        await open(wrapper)
        await open(wrapper)
        expect(getTranscript).toHaveBeenCalledTimes(2)
    })

    it('links a t3 thread and says where its tool calls are', async () => {
        getTranscript.mockResolvedValue(
            page({
                agent: 't3',
                link: 'https://t3.test/threads/e/t',
                tool_events: false,
                turns: [1],
                session_turns: 1,
            }),
        )
        const wrapper = mount(AgentJobTranscriptView, { props: { jobId: 'job-1' } })
        await open(wrapper)
        expect(wrapper.get('a').text()).toBe('Open in T3')
        expect(wrapper.get('a').attributes('href')).toBe('https://t3.test/threads/e/t')
        expect(wrapper.text()).toContain('Tool calls are in T3.')
    })

    it('says an expired conversation is gone, also for a job that is gone', async () => {
        getTranscript.mockResolvedValueOnce(page({ state: 'expired', reason: 'gone' }))
        const expired = mount(AgentJobTranscriptView, { props: { jobId: 'job-1' } })
        await open(expired)
        expect(expired.get('[data-testid="agent-job-transcript-expired"]').text()).toContain(
            'The conversation is no longer on this machine.',
        )

        getTranscript.mockRejectedValueOnce(Object.assign(new Error('not found'), { status: 404 }))
        const missing = mount(AgentJobTranscriptView, { props: { jobId: 'job-2' } })
        await open(missing)
        expect(missing.find('[data-testid="agent-job-transcript-expired"]').exists()).toBe(true)
    })

    it('offers a retry when the server is unreachable', async () => {
        getTranscript.mockRejectedValueOnce(new TypeError('Failed to fetch')).mockResolvedValueOnce(page({}))
        const wrapper = mount(AgentJobTranscriptView, { props: { jobId: 'job-1' } })
        await open(wrapper)
        expect(wrapper.get('[data-testid="agent-job-transcript-error"]').text()).toContain('Could not reach LoTaR')

        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Retry')!
            .trigger('click')
        await flushPromises()
        expect(wrapper.text()).toContain('Turn 2 of 2')
    })

    it('shows the harnessmux command for a conversation it can not read', async () => {
        getTranscript.mockResolvedValue(
            page({ state: 'unsupported', reason: 'The event log has format 2; this LoTaR reads format 1.' }),
        )
        const wrapper = mount(AgentJobTranscriptView, { props: { jobId: 'job-1' } })
        await open(wrapper)
        expect(wrapper.text()).toContain('format 2')
        expect(wrapper.get('[data-testid="agent-job-transcript-attach"]').text()).toBe(
            'harnessmux attach lotar-1-DEV-1-fix --from-start',
        )
    })
})
