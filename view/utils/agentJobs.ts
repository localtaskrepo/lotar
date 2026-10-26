import type { AgentJob, AgentJobLogEntry } from '../api/types'

export type LogTone = 'neutral' | 'tool' | 'agent' | 'user' | 'note' | 'warning' | 'success' | 'danger'

export interface LogEntryView {
    /** Short label shown before the entry ("Tool", "Agent", …). */
    label: string
    /** Text of the entry; empty when the entry should not be shown. */
    text: string
    tone: LogTone
    /** Commands and raw output read better in a monospace font. */
    mono: boolean
}

// Runner tool summaries look like "Bash: npm test", "Read: src/a.rs" or
// "Run: bash -lc '…'" (see agent_runner::tool_summary).
const TOOL_LINE = /^[A-Za-z_][\w.-]*: /

/** A tool-call summary rather than prose; shown as plain text, not markdown. */
export const isToolLine = (text: string) => TOOL_LINE.test(text) && !text.startsWith('Worktree: ')

/** How one job log entry is labelled and styled in the log timeline. */
export function describeLogEntry(entry: AgentJobLogEntry): LogEntryView {
    const text = entry.message?.trim() ?? ''
    switch (entry.kind) {
        case 'agent_job_started':
            return { label: 'Started', text: text || 'Job started', tone: 'neutral', mono: false }
        case 'agent_job_init':
            return { label: 'Ready', text, tone: 'neutral', mono: false }
        case 'agent_job_message':
            return { label: 'Agent', text, tone: 'agent', mono: false }
        case 'agent_job_input':
            return { label: 'You', text, tone: 'user', mono: false }
        case 'agent_job_result':
            return { label: 'Result', text, tone: 'neutral', mono: false }
        case 'agent_job_completed':
            return { label: 'Done', text: text || 'Job completed', tone: 'success', mono: false }
        case 'agent_job_failed':
            return { label: 'Failed', text: text || 'Job failed', tone: 'danger', mono: false }
        case 'agent_job_cancelled':
            return { label: 'Stopped', text: text || 'Job cancelled', tone: 'danger', mono: false }
        case 'agent_job_raw':
            return { label: 'Output', text, tone: 'neutral', mono: true }
        case 'agent_job_progress':
            if (/denied/i.test(text)) return { label: 'Denied', text, tone: 'warning', mono: false }
            if (isToolLine(text)) return { label: 'Tool', text, tone: 'tool', mono: true }
            return { label: 'Note', text, tone: 'note', mono: false }
        default:
            return { label: entry.kind.replace(/^agent_job_/, ''), text, tone: 'neutral', mono: false }
    }
}

/** "45s", "3m 05s", "1h 02m". */
export function formatDuration(ms: number): string {
    if (!Number.isFinite(ms) || ms < 0) return ''
    const seconds = Math.floor(ms / 1000)
    if (seconds < 60) return `${seconds}s`
    const minutes = Math.floor(seconds / 60)
    if (minutes < 60) return `${minutes}m ${String(seconds % 60).padStart(2, '0')}s`
    const hours = Math.floor(minutes / 60)
    return `${hours}h ${String(minutes % 60).padStart(2, '0')}m`
}

/** How long the job ran (or has been running, measured against `now`). */
export function jobDuration(job: AgentJob, now: number): string {
    if (!job.started_at) return ''
    const start = Date.parse(job.started_at)
    const end = job.finished_at ? Date.parse(job.finished_at) : now
    if (Number.isNaN(start) || Number.isNaN(end)) return ''
    return formatDuration(end - start)
}

/** "sonnet, high effort" for the settings a job or profile runs with. */
export function runSettings(model?: string | null, effort?: string | null): string {
    return [model, effort && `${effort} effort`].filter(Boolean).join(', ')
}

/** Local time of day for a log entry ("14:30:05"). */
export function formatLogTime(value: string): string {
    const date = new Date(value)
    if (Number.isNaN(date.getTime())) return value
    return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

export const isActiveJob = (job: AgentJob) => job.status === 'running' || job.status === 'queued'
export const isFailedJob = (job: AgentJob) => job.status === 'failed' || job.status === 'cancelled'

/** Request prefilled in the Run agent dialog. */
export const DEFAULT_RUN_PROMPT =
    'Work on this ticket. Make the changes it asks for, check them, and summarize what you changed and how you verified it.'
