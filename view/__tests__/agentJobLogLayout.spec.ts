import { describe, expect, it } from 'vitest'
import type { AgentJobLogEntry } from '../api/types'
import { buildLogLeaves, countLeaves, layoutLog, stripAnsi } from '../utils/agentJobLog'

const at = '2026-11-21T12:00:00Z'
const entry = (kind: string, sequence: number, message: string): AgentJobLogEntry => ({ kind, sequence, at, message })

describe('agentJobLog (DEV-769)', () => {
    it('removes color, cursor and link codes from terminal output', () => {
        const esc = String.fromCharCode(27)
        const bel = String.fromCharCode(7)
        expect(stripAnsi(`${esc}[1;31merror${esc}[0m: ${esc}[2Kdone`)).toBe('error: done')
        expect(stripAnsi(`${esc}]8;;https://x${bel}link${esc}]8;;${esc}\\ end`)).toBe('link end')
        expect(stripAnsi('plain')).toBe('plain')
    })

    it('keeps the command of a running entry when the end entry has none (DEV-774)', () => {
        const leaves = buildLogLeaves([
            {
                kind: 'agent_job_tool',
                sequence: 1,
                at,
                tool: { id: 'a', title: 'Terminal', status: 'running', input: 'npm test' },
            },
            {
                kind: 'agent_job_tool',
                sequence: 2,
                at,
                tool: { id: 'a', title: 'Terminal', status: 'completed', output: 'ok' },
            },
        ])
        expect(leaves).toHaveLength(1)
        expect(leaves[0]).toMatchObject({ type: 'tool', call: { what: 'npm test', output: 'ok', status: 'completed' } })
    })

    it('counts each view, and thinking only behind its toggle', () => {
        const leaves = buildLogLeaves([
            entry('agent_job_thought', 1, 'Hmm'),
            entry('agent_job_progress', 2, 'Tool: Terminal'),
            entry('agent_job_message', 3, 'Done.'),
            entry('agent_job_progress', 4, 'The check `npm test` failed'),
        ])
        expect(countLeaves(leaves)).toEqual({ all: 3, conversation: 1, tools: 1, lotar: 1, problems: 1, thinking: 1 })
        expect(layoutLog(leaves, { view: 'all', thinking: false, query: '' }).map((item) => item.type)).toEqual([
            'tools',
            'entry',
            'entry',
        ])
        expect(layoutLog(leaves, { view: 'conversation', thinking: true, query: '' })).toHaveLength(2)
    })
})
