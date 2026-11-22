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

        // A job that pauses the queue (DEV-604) has ended too.
        handlers.get('agent_job_paused')!(
            new MessageEvent('agent_job_paused', { data: JSON.stringify({ id: 'job-4' }) }),
        )
        await flushPromises()
        expect(three.find('[data-testid="agent-activity"]').exists()).toBe(false)

        one.unmount()
        two.unmount()
        three.unmount()
    })

    // DEV-765: jobs of other processes reach the stream from their logs.
    it('shows a question, skips events seen already and drops a job whose event says it ended', async () => {
        const card = mount(AgentActivityIndicator, { props: { taskId: 'DEV-7', label: true } })
        await flushPromises()
        const pill = () => card.find('[data-testid="agent-activity"]')

        applyAgentJobEvent('agent_job_started', { id: 'job-7', ticket_id: 'DEV-7', agent: 'fix', sequence: 1 })
        handlers.get('agent_job_question')!(
            new MessageEvent('agent_job_question', {
                data: JSON.stringify({
                    id: 'job-7',
                    ticket_id: 'DEV-7',
                    status: 'running',
                    sequence: 2,
                    message: 'Run git push? (Allow, Deny)',
                }),
            }),
        )
        await flushPromises()
        expect(pill().text()).toContain('Asks')
        expect(pill().attributes('title')).toBe('fix asks: Run git push? (Allow, Deny)')

        // The same event again, or an older one, changes nothing.
        applyAgentJobEvent('agent_job_progress', { id: 'job-7', ticket_id: 'DEV-7', sequence: 2, message: 'old' })
        await flushPromises()
        expect(pill().attributes('title')).toBe('fix asks: Run git push? (Allow, Deny)')

        // The agent's next event says it went on.
        applyAgentJobEvent('agent_job_progress', {
            id: 'job-7',
            ticket_id: 'DEV-7',
            sequence: 3,
            message: 'Bash: git push',
        })
        await flushPromises()
        expect(pill().text()).toContain('Working')
        expect(pill().attributes('title')).toBe('fix is working on this: Bash: git push')

        // A note logged after the end carries the ended status.
        applyAgentJobEvent('agent_job_progress', {
            id: 'job-7',
            ticket_id: 'DEV-7',
            status: 'completed',
            sequence: 4,
            message: 'Moved',
        })
        await flushPromises()
        expect(pill().exists()).toBe(false)
        card.unmount()
    })

    it('keeps the line when a tool finishes, as the server does', async () => {
        const card = mount(AgentActivityIndicator, { props: { taskId: 'DEV-8', label: true } })
        await flushPromises()
        const title = () => card.find('[data-testid="agent-activity"]').attributes('title')
        const tool = (status: string, message: string, sequence: number) =>
            applyAgentJobEvent('agent_job_tool', {
                id: 'job-8',
                ticket_id: 'DEV-8',
                agent: 'fix',
                sequence,
                message,
                tool: { status },
            })

        tool('running', 'Bash: npm test', 1)
        await flushPromises()
        expect(title()).toBe('fix is working on this: Bash: npm test')

        tool('completed', 'Terminal', 2)
        await flushPromises()
        expect(title()).toBe('fix is working on this: Bash: npm test')

        tool('failed', 'Bash: npm test failed', 3)
        await flushPromises()
        expect(title()).toBe('fix is working on this: Bash: npm test failed')
        card.unmount()
    })
})
