import { describe, expect, it } from 'vitest'
import { describeLogEntry, describeUsage, formatDuration, jobDuration, runSettings } from '../utils/agentJobs'
import type { AgentJob } from '../api/types'

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
