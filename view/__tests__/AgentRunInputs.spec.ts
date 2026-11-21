import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentJobRunInputs } from '../api/types'
import AgentRunInputs from '../components/AgentRunInputs.vue'

vi.mock('../api/client', () => ({
    api: { getAgentJobInputs: vi.fn() },
}))

const toast = vi.hoisted(() => vi.fn())
vi.mock('../components/toast', () => ({ showToast: toast }))

const { api } = await import('../api/client')
const getInputs = vi.mocked(api.getAgentJobInputs)

// "Agent instructions" holds a non-BMP character, so a slice by UTF-16 units would cut the next section.
const prompt = '## Agent instructions\nWork 🚀 well.\n\n## This job\nDEV-1'
const firstLength = Array.from('## Agent instructions\nWork 🚀 well.\n\n').length

const recorded: AgentJobRunInputs = {
    at: '2026-11-21T10:00:00Z',
    prompt,
    sections: [
        { heading: 'Agent instructions', start: 0, length: firstLength },
        { heading: 'This job', start: firstLength, length: Array.from(prompt).length - firstLength },
    ],
    instructions_source: { base: { kind: 'builtin', path: 'docs/help/agent-instructions.md' } },
    settings: {
        runner: 'harnessmux',
        agent_profile: 'implement',
        harness_agent: 'claude',
        model: 'opus',
        permissions: 'auto',
        rules: { deny_tools: ['WebFetch'], confine_writes: true },
        mcp_servers: [{ name: 'lotar', command: 'lotar', args: ['mcp'], env: ['LOTAR_TOKEN'] }],
        session: { name: 'DEV-1', mode: 'new' },
    },
    env: ['LOTAR_JOB_ID', 'SECRET_TOKEN'],
}

describe('AgentRunInputs', () => {
    const writeText = vi.fn()

    beforeEach(() => {
        getInputs.mockReset()
        toast.mockReset()
        writeText.mockReset().mockResolvedValue(undefined)
        Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    })

    afterEach(() => {
        vi.clearAllMocks()
    })

    it('stays collapsed and loads the inputs only when first opened', async () => {
        getInputs.mockResolvedValue({ inputs: recorded })
        const wrapper = mount(AgentRunInputs, { props: { jobId: 'job-1' } })
        const toggle = wrapper.get('button')
        expect(toggle.text()).toBe('Instructions')
        expect(toggle.attributes('aria-expanded')).toBe('false')
        expect(getInputs).not.toHaveBeenCalled()

        await toggle.trigger('click')
        await flushPromises()
        expect(getInputs).toHaveBeenCalledWith('job-1')
        expect(toggle.attributes('aria-expanded')).toBe('true')

        const rows = wrapper.findAll('tr').map((row) => row.text())
        expect(rows).toContain('Modelopus')
        expect(rows).toContain('Denied toolsWebFetch')
        expect(rows).toContain('Writes confinedyes')
        expect(rows).toContain('MCP lotarlotar mcp; env: LOTAR_TOKEN')
        expect(rows).toContain('Instructionsbuiltin: docs/help/agent-instructions.md')
        expect(rows).toContain('Environment (names only)LOTAR_JOB_ID, SECRET_TOKEN')

        const sections = wrapper.findAll('details')
        expect(sections.map((section) => section.get('summary').text())).toEqual([
            `Agent instructions (${firstLength} characters)`,
            `This job (${recorded.sections[1]!.length} characters)`,
        ])
        expect(sections[0]!.get('pre').text()).toBe('## Agent instructions\nWork 🚀 well.')
        expect(sections[1]!.get('pre').text()).toBe('## This job\nDEV-1')

        await toggle.trigger('click')
        await toggle.trigger('click')
        await flushPromises()
        expect(getInputs).toHaveBeenCalledTimes(1)
    })

    it('copies the exact prompt and everything as JSON', async () => {
        getInputs.mockResolvedValue({ inputs: recorded })
        const wrapper = mount(AgentRunInputs, { props: { jobId: 'job-1' } })
        await wrapper.get('button').trigger('click')
        await flushPromises()

        const button = (label: string) => wrapper.findAll('button').find((b) => b.text() === label)!
        await button('Copy prompt').trigger('click')
        await flushPromises()
        expect(writeText).toHaveBeenLastCalledWith(prompt)
        expect(toast).toHaveBeenLastCalledWith('Copied prompt')

        await button('Copy all').trigger('click')
        await flushPromises()
        expect(JSON.parse(writeText.mock.lastCall![0] as string)).toEqual(recorded)
    })

    it('says when a job has no record', async () => {
        getInputs.mockResolvedValue({ inputs: null })
        const wrapper = mount(AgentRunInputs, { props: { jobId: 'old' } })
        await wrapper.get('button').trigger('click')
        await flushPromises()
        expect(wrapper.text()).toContain('Not recorded for this job.')
        expect(wrapper.find('table').exists()).toBe(false)
    })

    it('shows a failed load and retries', async () => {
        getInputs.mockRejectedValueOnce(new Error('Server unavailable')).mockResolvedValueOnce({ inputs: null })
        const wrapper = mount(AgentRunInputs, { props: { jobId: 'job-1' } })
        await wrapper.get('button').trigger('click')
        await flushPromises()
        expect(wrapper.text()).toContain('Server unavailable')

        await wrapper
            .findAll('button')
            .find((b) => b.text() === 'Retry')!
            .trigger('click')
        await flushPromises()
        expect(getInputs).toHaveBeenCalledTimes(2)
        expect(wrapper.text()).toContain('Not recorded for this job.')
    })
})
