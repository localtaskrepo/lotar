import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentJobT3Thread } from '../api/types'
import AgentT3Thread from '../components/AgentT3Thread.vue'

vi.mock('../api/client', () => ({
    api: { getAgentJobT3Thread: vi.fn() },
}))

const { api } = await import('../api/client')
const getThread = vi.mocked(api.getAgentJobT3Thread)

const waiting: AgentJobT3Thread = {
    thread_id: 'thread-1',
    link: 'https://t3.example.com/threads/env-1/thread-1',
    environment_id: 'env-1',
    state: 'waiting',
    t3_status: 'running',
    checked_at: new Date().toISOString(),
}

function stateText(wrapper: ReturnType<typeof mount>) {
    return wrapper.get('[data-testid="agent-t3-thread-state"]').text()
}

describe('AgentT3Thread', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        getThread.mockReset()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('shows why the state is unavailable, without a link when there is none', async () => {
        getThread.mockResolvedValue({
            thread: {
                state: 'unavailable',
                reason: 'The job recorded no T3 Code thread.',
                checked_at: new Date().toISOString(),
            },
        })
        const wrapper = mount(AgentT3Thread, { props: { jobId: 'job-1' } })
        await flushPromises()

        expect(getThread).toHaveBeenCalledWith('job-1')
        expect(stateText(wrapper)).toBe('Unavailable')
        expect(wrapper.text()).toContain('The job recorded no T3 Code thread.')
        expect(wrapper.find('a').exists()).toBe(false)
        wrapper.unmount()
    })

    it('asks again every 15 seconds and keeps the last answer when a later ask fails', async () => {
        getThread.mockResolvedValueOnce({ thread: waiting }).mockRejectedValueOnce(new Error('lotar is offline'))
        const wrapper = mount(AgentT3Thread, { props: { jobId: 'job-1' } })
        await flushPromises()
        expect(stateText(wrapper)).toBe('Waits for an answer')
        expect(wrapper.get('a').attributes('href')).toBe(waiting.link)

        await vi.advanceTimersByTimeAsync(15000)
        await flushPromises()
        expect(getThread).toHaveBeenCalledTimes(2)
        expect(stateText(wrapper)).toBe('Waits for an answer')
        expect(wrapper.text()).not.toContain('lotar is offline')

        wrapper.unmount()
        await vi.advanceTimersByTimeAsync(30000)
        expect(getThread).toHaveBeenCalledTimes(2)
    })

    it('names a T3 status it does not know, and says why the first ask failed', async () => {
        getThread.mockRejectedValueOnce(new Error('lotar is offline')).mockResolvedValueOnce({
            thread: { ...waiting, state: 'unknown', t3_status: 'paused' },
        })
        const wrapper = mount(AgentT3Thread, { props: { jobId: 'job-1' } })
        await flushPromises()
        expect(wrapper.text()).toContain('lotar is offline')

        await wrapper.get('button').trigger('click')
        await flushPromises()
        expect(stateText(wrapper)).toBe('T3: paused')
        wrapper.unmount()
    })
})
