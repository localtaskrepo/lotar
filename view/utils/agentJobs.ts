import type { AgentJob, AgentJobLogEntry, AgentJobUsage } from '../api/types'

export type LogTone = 'neutral' | 'tool' | 'agent' | 'user' | 'note' | 'warning' | 'success' | 'danger'

export interface LogEntryView {
    /** Short label shown before the entry ("Tool", "Agent", …). */
    label: string
    /** Text of the entry; empty when the entry should not be shown. */
    text: string
    tone: LogTone
    /** Commands and raw output read better in a monospace font. */
    mono: boolean
    /** Shown below the text in a monospace block: the start of a failed tool's output. */
    detail?: string
    /** Shown folded behind its label (the agent's thinking). */
    folded?: boolean
}

const TOOL_LABELS: Record<string, string> = {
    execute: 'Run',
    read: 'Read',
    edit: 'Edit',
    delete: 'Delete',
    move: 'Move',
    search: 'Search',
    fetch: 'Fetch',
    think: 'Think',
}

/** Lines of a failed tool's output shown in the log. */
const FAILED_OUTPUT_LINES = 8

function describeTool(entry: AgentJobLogEntry, text: string): LogEntryView {
    const tool = entry.tool
    // An entry without its tool (written by hand, or cut) reads like an old tool line.
    if (!tool) return { label: 'Tool', text, tone: 'tool', mono: true }
    const label = (tool.kind && TOOL_LABELS[tool.kind]) || 'Tool'
    const what = tool.input?.trim() || tool.paths?.[0] || tool.title
    if (tool.status === 'failed') {
        const lines = (tool.output ?? '').trimEnd().split('\n')
        const excerpt = lines.slice(0, FAILED_OUTPUT_LINES).join('\n')
        const more = lines.length > FAILED_OUTPUT_LINES || tool.output_bytes ? '\n…' : ''
        return {
            label,
            text: `${what} · failed`,
            tone: 'danger',
            mono: true,
            detail: excerpt ? excerpt + more : undefined,
        }
    }
    return { label, text: tool.status === 'running' ? `${what} · running` : what, tone: 'tool', mono: true }
}

/**
 * The entries a log shows: of a tool call's entries only the newest, since it holds all that came
 * before: the end (DEV-766), else the newest running entry, which names Claude's command (DEV-774).
 */
export function visibleLogEntries(entries: AgentJobLogEntry[]): AgentJobLogEntry[] {
    const newest = new Map<string, number>()
    entries.forEach((entry, index) => {
        if (entry.kind === 'agent_job_tool' && entry.tool) newest.set(entry.tool.id, index)
    })
    return entries.filter(
        (entry, index) => !(entry.kind === 'agent_job_tool' && entry.tool && newest.get(entry.tool.id) !== index),
    )
}

// Tool calls are logged as "Tool: <title>" (agent_job_service::drive_session).
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
        case 'agent_job_tool':
            return describeTool(entry, text)
        case 'agent_job_thought':
            return { label: 'Thinking', text, tone: 'note', mono: false, folded: true }
        case 'agent_job_plan':
            return { label: 'Plan', text: text || 'Plan cleared', tone: 'note', mono: false }
        case 'agent_job_input':
            return { label: 'You', text, tone: 'user', mono: false }
        case 'agent_job_question':
            return { label: 'Question', text, tone: 'warning', mono: false }
        case 'agent_job_result':
            return { label: 'Result', text, tone: 'neutral', mono: false }
        case 'agent_job_completed':
            return { label: 'Done', text: text || 'Job completed', tone: 'success', mono: false }
        case 'agent_job_failed':
            return { label: 'Failed', text: text || 'Job failed', tone: 'danger', mono: false }
        case 'agent_job_cancelled':
            return { label: 'Stopped', text: text || 'Job cancelled', tone: 'danger', mono: false }
        case 'agent_job_paused':
            return { label: 'Paused', text: text || 'Job paused', tone: 'warning', mono: false }
        case 'agent_job_transcript':
            return { label: 'Session', text, tone: 'note', mono: false }
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

const compactNumber = new Intl.NumberFormat(undefined, { notation: 'compact', maximumSignificantDigits: 3 })

export function formatCost(cost: number): string {
    return cost >= 1 ? `$${cost.toFixed(2)}` : `$${cost.toFixed(cost >= 0.01 ? 3 : 4)}`
}

/** Short usage line for a job card: "$0.0040 · 107K tokens · 4 turns". */
export function describeUsage(usage?: AgentJobUsage | null): string {
    if (!usage) return ''
    const parts: string[] = []
    if (usage.cost_usd !== undefined) parts.push(formatCost(usage.cost_usd))
    const tokens = (usage.input_tokens ?? 0) + (usage.output_tokens ?? 0)
    if (tokens) parts.push(`${compactNumber.format(tokens)} tokens`)
    if (usage.turns !== undefined) parts.push(`${usage.turns} ${usage.turns === 1 ? 'turn' : 'turns'}`)
    if (usage.premium_requests !== undefined)
        parts.push(`${usage.premium_requests} premium ${usage.premium_requests === 1 ? 'request' : 'requests'}`)
    if (usage.lines_added !== undefined || usage.lines_removed !== undefined)
        parts.push(`+${usage.lines_added ?? 0} −${usage.lines_removed ?? 0} lines`)
    return parts.join(' · ')
}

/** Full detail for a tooltip. */
export function usageDetail(usage?: AgentJobUsage | null): string {
    if (!usage) return ''
    const lines: string[] = []
    if (usage.input_tokens !== undefined)
        lines.push(
            `Input: ${usage.input_tokens.toLocaleString()} tokens` +
                (usage.cached_input_tokens ? ` (${usage.cached_input_tokens.toLocaleString()} cached)` : ''),
        )
    if (usage.output_tokens !== undefined) lines.push(`Output: ${usage.output_tokens.toLocaleString()} tokens`)
    if (usage.cost_usd !== undefined) lines.push(`Cost: $${usage.cost_usd.toFixed(4)}`)
    return lines.join('\n')
}
