import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentJob, AgentJobLogEntry, AgentJobTool } from '../api/types'
import AgentJobLog from '../components/AgentJobLog.vue'

vi.mock('../api/client', () => ({
    api: { getAgentJobInputs: vi.fn().mockResolvedValue({ inputs: null }) },
}))

const at = '2026-11-21T12:00:00Z'

function tool(sequence: number, fields: Partial<AgentJobTool> & { id: string }): AgentJobLogEntry {
    return {
        kind: 'agent_job_tool',
        sequence,
        at,
        message: fields.title ?? 'Terminal',
        tool: { title: 'Terminal', kind: 'execute', status: 'completed', ...fields },
    }
}

const note = (sequence: number, message: string, kind = 'agent_job_progress'): AgentJobLogEntry => ({
    kind,
    sequence,
    at,
    message,
})

const job = (fields: Partial<AgentJob> = {}): AgentJob => ({
    id: 'job-1',
    ticket_id: 'DEV-1',
    runner: 'claude',
    status: 'running',
    created_at: at,
    started_at: at,
    ...fields,
})

const chip = (wrapper: ReturnType<typeof mount>, label: string) => {
    const button = wrapper.findAll('.job-log__chip').find((candidate) => candidate.text().startsWith(label))
    if (!button) throw new Error(`no chip ${label}`)
    return button
}

describe('AgentJobLog (DEV-766, DEV-769)', () => {
    beforeEach(() => localStorage.clear())

    it('hides thinking until its chip is on, and shows a failed call open with its output', async () => {
        const call = { id: 'call_1', title: 'cargo test', input: 'cargo test', started_at: at }
        const esc = String.fromCharCode(27)
        const entries: AgentJobLogEntry[] = [
            note(1, 'Look at the parser first.\n\nThen run the tests.', 'agent_job_thought'),
            tool(2, { ...call, status: 'running' }),
            tool(3, { ...call, status: 'failed', output: `${esc}[31mtest parser ... FAILED${esc}[0m` }),
        ]
        const wrapper = mount(AgentJobLog, { props: { entries } })

        expect(wrapper.find('details.log-folded').exists()).toBe(false)
        await chip(wrapper, 'Thinking').trigger('click')
        const thought = wrapper.get('details.log-folded')
        expect(thought.attributes('open')).toBeUndefined()
        expect(thought.get('.log-folded-preview').text()).toBe('Look at the parser first.')

        const failed = wrapper.get('.tool-call--failed')
        expect(failed.attributes('open')).toBeDefined()
        expect(failed.get('.tool-call__label').text()).toBe('Run')
        expect(failed.get('.tool-call__what').text()).toBe('cargo test')
        expect(failed.get('.tool-call__pre').text()).toBe('test parser ... FAILED')
        // The choice is remembered for this viewer.
        expect(JSON.parse(localStorage.getItem('lotar.agentJobLog.agents') ?? '{}')).toEqual({
            view: 'all',
            thinking: true,
        })
    })

    it('folds tool calls between two messages into one summary row, with the failed count', () => {
        const entries: AgentJobLogEntry[] = [
            note(1, 'Reading the parser.', 'agent_job_message'),
            tool(2, { id: 'a', input: 'npm test' }),
            tool(3, { id: 'b', input: 'cargo test' }),
            tool(4, { id: 'c', kind: 'read', title: 'Read File', paths: ['src/a.rs'] }),
            tool(5, { id: 'd', input: 'false', status: 'failed', output: 'exit 1' }),
            note(6, 'All read.', 'agent_job_message'),
        ]
        const wrapper = mount(AgentJobLog, { props: { entries } })

        const group = wrapper.get('details.log-group--tools')
        expect(group.get('.log-group__text').text()).toBe('3 commands · 1 read · 1 failed')
        expect(group.findAll('.tool-call')).toHaveLength(4)
        expect(chip(wrapper, 'Tools').text()).toBe('Tools 4')
        expect(chip(wrapper, 'Problems').text()).toBe('Problems 1')
    })

    it('shows just the conversation or just the problems in one click', async () => {
        const entries: AgentJobLogEntry[] = [
            note(1, '', 'agent_job_started'),
            note(2, 'Session lotar-1 (`harnessmux attach lotar-1` follows it).'),
            note(3, 'Wrote .claude/settings.local.json in /tmp/x'),
            tool(4, { id: 'a', input: 'npm test', status: 'failed', output: 'boom' }),
            note(5, 'Fixed it.', 'agent_job_message'),
            note(6, '1 tool call(s) were denied by the permission settings'),
        ]
        const wrapper = mount(AgentJobLog, { props: { entries } })
        // Setup notes before the first turn fold into one row.
        expect(wrapper.get('.log-group--setup .log-group__text').text()).toBe('3 notes before the first turn')

        await chip(wrapper, 'Conversation').trigger('click')
        expect(wrapper.get('.log-list').text()).toContain('Fixed it.')
        expect(wrapper.find('.tool-call').exists()).toBe(false)
        expect(wrapper.get('.log-list').text()).not.toContain('denied')

        await chip(wrapper, 'Problems').trigger('click')
        const text = wrapper.get('.log-list').text()
        expect(text).toContain('npm test')
        expect(text).toContain('denied')
        expect(text).not.toContain('Fixed it.')
    })

    it('searches the log, also in tool output', async () => {
        const entries: AgentJobLogEntry[] = [
            tool(1, { id: 'a', input: 'npm test', output: 'parser ok' }),
            note(2, 'Tests pass.', 'agent_job_message'),
            tool(3, { id: 'b', input: 'git status' }),
        ]
        const wrapper = mount(AgentJobLog, { props: { entries } })
        await wrapper.get('input.job-log__search').setValue('PARSER')
        expect(wrapper.get('.job-log__matches').text()).toBe('1 match')
        expect(wrapper.findAll('.tool-call__what').map((what) => what.text())).toEqual(['npm test'])
        expect(wrapper.get('.log-list').text()).not.toContain('Tests pass.')
    })

    it('opens each call whose output holds the search, also past the clipped start', async () => {
        const long = [...Array.from({ length: 20 }, (_, index) => `line ${index}`), 'npm notice'].join('\n')
        const entries: AgentJobLogEntry[] = [
            tool(1, { id: 'a', input: 'make build', output: 'ran npm ci' }),
            tool(2, { id: 'b', input: 'make test', output: long }),
            tool(3, { id: 'c', input: 'git status', output: 'clean' }),
        ]
        const wrapper = mount(AgentJobLog, { props: { entries } })
        const calls = () => wrapper.findAll<HTMLDetailsElement>('details.tool-call')
        expect(calls().map((call) => call.element.open)).toEqual([false, false, false])

        await wrapper.get('input.job-log__search').setValue('npm')
        expect(wrapper.get('.job-log__matches').text()).toBe('2 matches')
        expect(wrapper.get<HTMLDetailsElement>('details.log-group--tools').element.open).toBe(true)
        expect(calls().map((call) => call.element.open)).toEqual([true, true])
        const outputs = wrapper.findAll('.tool-call__pre').map((pre) => pre.text())
        expect(outputs).toContain('ran npm ci')
        expect(outputs.some((text) => text.endsWith('npm notice'))).toBe(true)
        expect(wrapper.find('.tool-call__more').exists()).toBe(false)

        await wrapper.get('input.job-log__search').setValue('')
        expect(calls().map((call) => call.element.open)).toEqual([false, false, false])
    })

    it('shows the result once: the final reply in the result card, a repeated failure note dropped', () => {
        const done = mount(AgentJobLog, {
            props: {
                entries: [
                    tool(1, { id: 'a', input: 'npm test' }),
                    note(2, 'Changed the parser.', 'agent_job_message'),
                    note(3, 'Tests pass.', 'agent_job_message'),
                    note(4, '', 'agent_job_completed'),
                    note(5, 'DEV-1 is now Reviewing, assigned to @review'),
                ],
            },
        })
        const card = done.get('.log-result--success')
        expect(card.text()).toContain('Changed the parser.')
        expect(card.text()).toContain('Tests pass.')
        expect(done.findAll('.log-row').filter((row) => row.text().includes('Tests pass.'))).toHaveLength(0)

        const failed = mount(AgentJobLog, {
            props: {
                entries: [
                    note(1, 'Internal error: You have hit your weekly limit'),
                    note(
                        2,
                        'Paused, not failed: claude hit its usage limit (Internal error: You have hit your weekly limit)',
                        'agent_job_paused',
                    ),
                ],
            },
        })
        expect(failed.get('.log-result--warning').text()).toContain('Paused, not failed')
        expect(failed.findAll('.log-row')).toHaveLength(0)
    })

    it('shows a question with the call that asked and its options', () => {
        const entries: AgentJobLogEntry[] = [
            tool(1, { id: 'a', input: 'rm -rf build', status: 'running' }),
            note(2, 'rm -rf build (Yes, No)', 'agent_job_question'),
        ]
        const wrapper = mount(AgentJobLog, { props: { entries } })
        const question = wrapper.get('.log-question')
        expect(question.get('.log-question__lead').text()).toBe('Asks to run:')
        expect(question.get('.log-question__input').text()).toBe('rm -rf build')
        expect(question.findAll('.log-question__option').map((option) => option.text())).toEqual(['Yes', 'No'])
    })

    it('puts the Instructions button in the header and opens its panel full width below it', async () => {
        const wrapper = mount(AgentJobLog, { props: { entries: [note(1, 'Started')], job: job(), instructions: true } })
        const toggle = wrapper.get('.job-log__header .job-log__instructions')
        expect(toggle.attributes('aria-expanded')).toBe('false')
        expect(wrapper.find('.run-inputs__panel').exists()).toBe(false)

        await toggle.trigger('click')
        await flushPromises()
        const panel = wrapper.get('.job-log > .run-inputs__panel')
        expect(wrapper.find('.job-log__header .run-inputs__panel').exists()).toBe(false)
        expect(panel.element.previousElementSibling?.classList.contains('job-log__header')).toBe(true)
        expect(panel.attributes('id')).toBe(toggle.attributes('aria-controls'))
        expect(panel.text()).toContain('Not recorded for this job.')

        await toggle.trigger('click')
        expect(toggle.attributes('aria-expanded')).toBe('false')
        expect(wrapper.get('.run-inputs__panel').attributes('style')).toContain('display: none')
    })

    it('shows the live line while the job runs: the running command and the context used', () => {
        const entries: AgentJobLogEntry[] = [tool(1, { id: 'a', input: 'npm test', status: 'running', started_at: at })]
        const wrapper = mount(AgentJobLog, {
            props: { entries, job: job({ usage: { context_used: 50_000, context_size: 200_000 } }) },
        })
        const live = wrapper.get('[data-testid="job-log-live"]')
        expect(live.text()).toContain('Running npm test')
        expect(live.get('.job-log__context').text()).toBe('Context 25%')
        expect(wrapper.get('a.job-log__link').attributes('href')).toBe('/agents?job=job-1')

        const finished = mount(AgentJobLog, { props: { entries, job: job({ status: 'completed' }) } })
        expect(finished.find('[data-testid="job-log-live"]').exists()).toBe(false)
    })
})
