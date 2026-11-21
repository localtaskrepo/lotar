import { describe, expect, it } from 'vitest'
import {
    describeLogEntry,
    describeUsage,
    formatDuration,
    jobDuration,
    runSettings,
    visibleLogEntries,
} from '../utils/agentJobs'
import type { AgentJob, AgentJobLogEntry, AgentJobTool } from '../api/types'

const entry = (kind: string, message: string | null = null) => ({ kind, sequence: 1, at: '', message })

describe('agent job helpers', () => {
    it('labels log entries by what they are', () => {
        expect(describeLogEntry(entry('agent_job_progress', 'Bash: npm test'))).toMatchObject({
            label: 'Tool',
            mono: true,
        })
        expect(describeLogEntry(entry('agent_job_progress', 'Worktree: /tmp/x')).label).toBe('Note')
        expect(
            describeLogEntry(entry('agent_job_progress', '1 tool call(s) were denied by the permission settings')),
        ).toMatchObject({ label: 'Denied', tone: 'warning' })
        expect(describeLogEntry(entry('agent_job_message', 'Done.')).label).toBe('Agent')
        expect(describeLogEntry(entry('agent_job_input', 'Also add tests')).label).toBe('You')
        expect(describeLogEntry(entry('agent_job_started')).text).toBe('Job started')
        expect(describeLogEntry(entry('agent_job_result')).text).toBe('')
        expect(describeLogEntry(entry('agent_job_failed', 'exit 1'))).toMatchObject({ label: 'Failed', tone: 'danger' })
        expect(describeLogEntry(entry('agent_job_paused', 'Paused, not failed'))).toMatchObject({
            label: 'Paused',
            tone: 'warning',
        })
    })

    it('shows a tool call by its command or path and a failed one with its output', () => {
        const tool = (sequence: number, fields: Partial<AgentJobTool>): AgentJobLogEntry => ({
            kind: 'agent_job_tool',
            sequence,
            at: '',
            message: 'line',
            tool: { id: 't1', title: 'Terminal', status: 'running', ...fields },
        })
        expect(describeLogEntry(tool(1, { kind: 'execute', input: 'npm test' }))).toMatchObject({
            label: 'Run',
            text: 'npm test · running',
            tone: 'tool',
            mono: true,
        })
        expect(describeLogEntry(tool(2, { kind: 'read', paths: ['src/a.rs'], status: 'completed' }))).toMatchObject({
            label: 'Read',
            text: 'src/a.rs',
        })
        expect(describeLogEntry(tool(3, { status: 'completed' })).text).toBe('Terminal')
        const failed = describeLogEntry(
            tool(4, {
                kind: 'execute',
                status: 'failed',
                output: Array.from({ length: 10 }, (_, i) => `l${i}`).join('\n'),
            }),
        )
        expect(failed).toMatchObject({ label: 'Run', text: 'Terminal · failed', tone: 'danger' })
        expect(failed.detail).toBe('l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7\n…')
        expect(describeLogEntry(entry('agent_job_tool', 'Terminal'))).toMatchObject({ label: 'Tool', text: 'Terminal' })
        expect(describeLogEntry(entry('agent_job_thought', 'Look first.'))).toMatchObject({
            label: 'Thinking',
            folded: true,
        })
        expect(describeLogEntry(entry('agent_job_progress', 'Failed: toolu_0111'))).toMatchObject({ label: 'Tool' })
    })

    it('leaves out a tool call start once its end is in the log', () => {
        const start = {
            kind: 'agent_job_tool',
            sequence: 1,
            at: '',
            tool: { id: 'a', title: 'T', status: 'running' as const },
        }
        const end = { ...start, sequence: 3, tool: { ...start.tool, status: 'completed' as const } }
        const other = { ...start, sequence: 2, tool: { ...start.tool, id: 'b' } }
        expect(visibleLogEntries([start, other, end]).map((e) => e.sequence)).toEqual([2, 3])
    })

    it('formats durations compactly', () => {
        expect(formatDuration(45_000)).toBe('45s')
        expect(formatDuration(185_000)).toBe('3m 05s')
        expect(formatDuration(3_720_000)).toBe('1h 02m')
        const job = { started_at: '2026-01-01T00:00:00Z', finished_at: '2026-01-01T00:00:18Z' } as AgentJob
        expect(jobDuration(job, 0)).toBe('18s')
        expect(jobDuration({ ...job, finished_at: null }, Date.parse('2026-01-01T00:01:00Z'))).toBe('1m 00s')
    })

    it('describes run settings', () => {
        expect(runSettings('sonnet', 'high')).toBe('sonnet, high effort')
        expect(runSettings(null, 'low')).toBe('low effort')
        expect(runSettings(null, null)).toBe('')
    })

    it('summarizes usage per CLI', () => {
        expect(
            describeUsage({
                input_tokens: 107028,
                cached_input_tokens: 93837,
                output_tokens: 588,
                cost_usd: 0.004,
                turns: 4,
            }),
        ).toMatch(/^\$0\.0040 · 108K tokens · 4 turns$/)
        expect(describeUsage({ premium_requests: 1, lines_added: 3, lines_removed: 1 })).toBe(
            '1 premium request · +3 −1 lines',
        )
        expect(describeUsage(null)).toBe('')
    })
})
