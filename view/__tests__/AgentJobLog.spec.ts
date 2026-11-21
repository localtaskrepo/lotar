import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { AgentJobLogEntry } from '../api/types'
import AgentJobLog from '../components/AgentJobLog.vue'

const at = '2026-11-21T12:00:00Z'

describe('AgentJobLog (DEV-766)', () => {
    it('folds thinking, shows a tool call once with its command, and a failed call with its output', () => {
        const tool = { id: 'call_1', kind: 'execute', title: 'cargo test', input: 'cargo test', started_at: at }
        const entries: AgentJobLogEntry[] = [
            { kind: 'agent_job_thought', sequence: 1, at, message: 'Look at the parser first.\n\nThen run the tests.' },
            { kind: 'agent_job_tool', sequence: 2, at, message: 'cargo test', tool: { ...tool, status: 'running' } },
            {
                kind: 'agent_job_tool',
                sequence: 3,
                at,
                message: 'Failed: cargo test',
                tool: { ...tool, status: 'failed', output: 'test parser ... FAILED' },
            },
        ]
        const wrapper = mount(AgentJobLog, { props: { entries } })

        const thought = wrapper.get('details.log-folded')
        expect(thought.attributes('open')).toBeUndefined()
        expect(thought.get('.log-folded-preview').text()).toBe('Look at the parser first.')
        expect(thought.text()).toContain('Then run the tests.')

        const rows = wrapper.findAll('.log-row')
        expect(rows).toHaveLength(2)
        const call = rows[1]
        expect(call?.get('.log-kind').text()).toBe('Run')
        expect(call?.get('.log-message').text()).toBe('cargo test · failed')
        expect(call?.classes()).toContain('log-tone-danger')
        expect(call?.get('pre.log-detail').text()).toBe('test parser ... FAILED')
    })
})
