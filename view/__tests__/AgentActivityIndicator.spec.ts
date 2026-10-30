import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import AgentActivityIndicator from '../components/AgentActivityIndicator.vue'
import { applyAgentJobEvent } from '../composables/useAgentActivity'

// DEV-249: tasks an agent is working on show an indicator that says what the
// agent last reported.

const handlers = vi.hoisted(() => new Map<string, (event: MessageEvent) => void>())

vi.mock('../api/client', () => ({
    api: {
        listAgentJobs: vi.fn(async () => ({
            jobs: [
                {
                    id: 'job-1',
                    ticket_id: 'DEV-1',
                    runner: 'claude',
                    agent: 'Implementer',
                    status: 'running',
                    created_at: '',
                    last_message: 'Reading the tests',
                },
                { id: 'job-2', ticket_id: 'DEV-2', runner: 'codex', status: 'queued', created_at: '' },
                { id: 'job-3', ticket_id: 'DEV-3', runner: 'codex', status: 'completed', created_at: '' },
            ],
        })),
    },
}))

vi.mock('../composables/useSse', () => ({
    useSse: () => ({
        on: (kind: string, handler: (event: MessageEvent) => void) => handlers.set(kind, handler),
        off: vi.fn(),
        close: vi.fn(),
    }),
}))

describe('AgentActivityIndicator', () => {
    it('shows running and queued jobs and follows live job events', async () => {
        const one = mount(AgentActivityIndicator, { props: { taskId: 'DEV-1', label: true } })
        const two = mount(AgentActivityIndicator, { props: { taskId: 'DEV-2' } })
        const three = mount(AgentActivityIndicator, { props: { taskId: 'DEV-3' } })
        await flushPromises()

        const running = one.get('[data-testid="agent-activity"]')
        expect(running.attributes('title')).toBe('Implementer is working on this: Reading the tests')
        expect(running.text()).toContain('Working')
        expect(two.get('[data-testid="agent-activity"]').attributes('title')).toBe(
            'codex is queued to work on this task',
        )
        expect(three.find('[data-testid="agent-activity"]').exists()).toBe(false)

        // Progress updates the text; a finished job removes the indicator.
        handlers.get('agent_job_progress')!(
            new MessageEvent('agent_job_progress', {
                data: JSON.stringify({ id: 'job-1', message: 'Running checks' }),
            }),
        )
        await flushPromises()
        expect(one.get('[data-testid="agent-activity"]').attributes('title')).toContain('Running checks')

        applyAgentJobEvent('agent_job_started', { id: 'job-4', ticket_id: 'DEV-3', runner: 'gemini' })
        await flushPromises()
        expect(three.get('[data-testid="agent-activity"]').attributes('title')).toBe('gemini is working on this task')

        applyAgentJobEvent('agent_job_completed', { id: 'job-1' })
        await flushPromises()
        expect(one.find('[data-testid="agent-activity"]').exists()).toBe(false)

        one.unmount()
        two.unmount()
        three.unmount()
    })
})
