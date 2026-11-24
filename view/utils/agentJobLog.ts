import type { AgentJobLogEntry, AgentJobTool } from '../api/types'
import { describeLogEntry, formatDuration, type LogEntryView } from './agentJobs'

/** What a log entry is about; an entry can be in more than one (a failed tool is a tool and a problem). */
export type LogCategory = 'conversation' | 'tools' | 'thinking' | 'lotar' | 'problems'

/** The views of a log: everything, or one category. Thinking is a separate toggle. */
export type LogView = 'all' | 'conversation' | 'tools' | 'lotar' | 'problems'

export const LOG_VIEWS: { value: LogView; label: string }[] = [
    { value: 'all', label: 'All' },
    { value: 'conversation', label: 'Conversation' },
    { value: 'tools', label: 'Tools' },
    { value: 'lotar', label: 'lotar' },
    { value: 'problems', label: 'Problems' },
]

export interface ToolCall {
    /** Kind of call for people: "Run", "Read", … */
    label: string
    glyph: string
    /** ACP kind, or "other"; old "Tool: …" lines have none. */
    kind: string
    /** The command, the path or the title. */
    what: string
    /** `logged`: an old "Tool: …" line, which has no status. */
    status: 'running' | 'completed' | 'failed' | 'logged'
    startedAt?: string
    duration: string
    /** The arguments as JSON, when they say more than `what`. */
    input?: string
    output?: string
    /** The output was cut by the server; its full size. */
    outputBytes?: number
}

interface LeafBase {
    key: string
    sequence: number
    at: string
    categories: LogCategory[]
    /** Lower-case text a search looks in. */
    search: string
}

export interface EntryLeaf extends LeafBase {
    type: 'entry'
    kind: string
    view: LogEntryView
}

export interface ToolLeaf extends LeafBase {
    type: 'tool'
    call: ToolCall
}

export interface QuestionLeaf extends LeafBase {
    type: 'question'
    title: string
    options: string[]
    /** The call that asked for permission: the newest one still running when the question came. */
    call?: ToolCall
}

/** The job's end (Done, Failed, Stopped, Paused) with the agent's final reply above it. */
export interface ResultLeaf extends LeafBase {
    type: 'result'
    view: LogEntryView
    reply: string
}

export type LogLeaf = EntryLeaf | ToolLeaf | QuestionLeaf | ResultLeaf

export interface ToolGroup {
    type: 'tools'
    key: string
    at: string
    calls: ToolLeaf[]
    summary: string
    failed: number
}

export interface SetupGroup {
    type: 'setup'
    key: string
    at: string
    entries: EntryLeaf[]
}

export type LogItem = LogLeaf | ToolGroup | SetupGroup

const GLYPHS: Record<string, string> = {
    execute: '$',
    read: '≡',
    edit: '✎',
    delete: '✕',
    move: '⇄',
    search: '⌕',
    fetch: '⇣',
    think: '…',
}

const LABELS: Record<string, string> = {
    execute: 'Run',
    read: 'Read',
    edit: 'Edit',
    delete: 'Delete',
    move: 'Move',
    search: 'Search',
    fetch: 'Fetch',
    think: 'Think',
}

// Nouns for the folded summary, in this order: "9 commands · 2 reads · 1 edit".
const NOUNS: [string, string, string][] = [
    ['execute', 'command', 'commands'],
    ['read', 'read', 'reads'],
    ['edit', 'edit', 'edits'],
    ['search', 'search', 'searches'],
    ['fetch', 'fetch', 'fetches'],
    ['delete', 'delete', 'deletes'],
    ['move', 'move', 'moves'],
    ['other', 'tool call', 'tool calls'],
]

const ESC = 27
const BEL = 7

/** Text without terminal color and cursor codes: CSI (`ESC [ … final`) and OSC (`ESC ] … BEL`) sequences. */
export function stripAnsi(text: string): string {
    if (!text.includes(String.fromCharCode(ESC))) return text
    let out = ''
    let index = 0
    while (index < text.length) {
        if (text.charCodeAt(index) !== ESC) {
            out += text[index]
            index++
            continue
        }
        const kind = text[index + 1]
        index += 2
        if (kind === '[') {
            // Parameters and intermediates, then one final byte in @ to ~.
            while (index < text.length) {
                const code = text.charCodeAt(index++)
                if (code >= 0x40 && code <= 0x7e) break
            }
        } else if (kind === ']') {
            while (index < text.length) {
                const code = text.charCodeAt(index)
                if (code === BEL) {
                    index++
                    break
                }
                if (code === ESC && text[index + 1] === '\\') {
                    index += 2
                    break
                }
                index++
            }
        }
    }
    return out
}

const END_KINDS = new Set(['agent_job_completed', 'agent_job_failed', 'agent_job_cancelled', 'agent_job_paused'])
const CONVERSATION_KINDS = new Set(['agent_job_message', 'agent_job_input', 'agent_job_plan'])
// Notes about something that went wrong: denied calls, failed checks, errors.
const PROBLEM_NOTE = /\b(denied|failed|fails|error|could not|couldn't|exited before|not merged)\b/i

function toolCall(tool: AgentJobTool): ToolCall {
    const kind = tool.kind && LABELS[tool.kind] ? tool.kind : 'other'
    const what = tool.input?.trim() || tool.paths?.[0] || tool.title
    const started = tool.started_at ? Date.parse(tool.started_at) : NaN
    const finished = tool.finished_at ? Date.parse(tool.finished_at) : NaN
    const input = tool.raw_input?.trim()
    return {
        label: LABELS[kind] ?? 'Tool',
        glyph: GLYPHS[kind] ?? '•',
        kind,
        what,
        status: tool.status,
        startedAt: tool.started_at,
        duration: Number.isNaN(started) || Number.isNaN(finished) ? '' : formatDuration(finished - started),
        input: input && input !== what && input !== JSON.stringify(what) ? input : undefined,
        output: tool.output ? stripAnsi(tool.output).trimEnd() || undefined : undefined,
        outputBytes: tool.output_bytes,
    }
}

/** An old "Tool: Terminal" or "Bash: npm test" line, which carries only its text. */
function loggedCall(text: string): ToolCall {
    return { label: 'Tool', glyph: '•', kind: 'other', what: text, status: 'logged', duration: '' }
}

const callSearch = (call: ToolCall) => [call.what, call.input, call.output].filter(Boolean).join('\n').toLowerCase()

/**
 * Every tool call once, with the fields of all its entries: the end entry may lack the command its
 * running entry named (DEV-774), so later fields fill in over earlier ones.
 */
function mergeToolEntries(entries: AgentJobLogEntry[]): { merged: AgentJobLogEntry[]; runningAt: Map<number, string> } {
    const fields = new Map<string, AgentJobTool>()
    const newest = new Map<string, number>()
    // For each question, the call that was still running when it came.
    const runningAt = new Map<number, string>()
    const running: string[] = []
    entries.forEach((entry, index) => {
        if (entry.kind === 'agent_job_question') {
            const id = running[running.length - 1]
            if (id) runningAt.set(entry.sequence, id)
        }
        if (entry.kind !== 'agent_job_tool' || !entry.tool) return
        const tool = entry.tool
        const before = fields.get(tool.id)
        const defined = Object.fromEntries(Object.entries(tool).filter(([, value]) => value != null && value !== ''))
        fields.set(tool.id, { ...before, ...defined } as AgentJobTool)
        newest.set(tool.id, index)
        const at = running.indexOf(tool.id)
        if (at !== -1) running.splice(at, 1)
        if (tool.status === 'running') running.push(tool.id)
    })
    const merged = entries
        .filter(
            (entry, index) => !(entry.kind === 'agent_job_tool' && entry.tool && newest.get(entry.tool.id) !== index),
        )
        .map((entry) =>
            entry.kind === 'agent_job_tool' && entry.tool
                ? { ...entry, tool: fields.get(entry.tool.id) ?? entry.tool }
                : entry,
        )
    return { merged, runningAt }
}

/** "rm -rf build (Yes, No)": the question and its options. */
function parseQuestion(text: string): { title: string; options: string[] } {
    const match = /^([\s\S]*) \(([^()]*)\)$/.exec(text)
    if (!match?.[1] || match[2] === undefined) return { title: text, options: [] }
    return {
        title: match[1],
        options: match[2]
            .split(',')
            .map((option) => option.trim())
            .filter(Boolean),
    }
}

function entryCategories(kind: string, view: LogEntryView): LogCategory[] {
    if (CONVERSATION_KINDS.has(kind)) return ['conversation']
    if (kind === 'agent_job_thought') return ['thinking']
    if (kind === 'agent_job_raw') return ['tools']
    const categories: LogCategory[] = ['lotar']
    if (view.tone === 'danger' || view.tone === 'warning' || PROBLEM_NOTE.test(view.text)) categories.push('problems')
    return categories
}

/** The log's entries as leaves: one per tool call, question, note or message, the end as a result. */
export function buildLogLeaves(entries: AgentJobLogEntry[]): LogLeaf[] {
    const { merged, runningAt } = mergeToolEntries(entries)
    const calls = new Map<string, ToolCall>()
    for (const entry of merged)
        if (entry.kind === 'agent_job_tool' && entry.tool) calls.set(entry.tool.id, toolCall(entry.tool))

    const leaves: LogLeaf[] = []
    for (const entry of merged) {
        const base = { key: String(entry.sequence), sequence: entry.sequence, at: entry.at }
        if (entry.kind === 'agent_job_tool' && entry.tool) {
            const call = calls.get(entry.tool.id) ?? toolCall(entry.tool)
            const categories: LogCategory[] = call.status === 'failed' ? ['tools', 'problems'] : ['tools']
            leaves.push({ ...base, type: 'tool', call, categories, search: callSearch(call) })
            continue
        }
        const view = describeLogEntry(entry)
        if (!view.text) continue
        if (view.mono) view.text = stripAnsi(view.text)
        if (entry.kind === 'agent_job_tool' || (entry.kind === 'agent_job_progress' && view.label === 'Tool')) {
            const call = loggedCall(view.text)
            leaves.push({ ...base, type: 'tool', call, categories: ['tools'], search: callSearch(call) })
            continue
        }
        if (entry.kind === 'agent_job_question') {
            const id = runningAt.get(entry.sequence)
            const { title, options } = parseQuestion(view.text)
            leaves.push({
                ...base,
                type: 'question',
                title,
                options,
                call: id ? calls.get(id) : undefined,
                categories: ['conversation'],
                search: view.text.toLowerCase(),
            })
            continue
        }
        leaves.push({
            ...base,
            type: 'entry',
            kind: entry.kind,
            view,
            categories: entryCategories(entry.kind, view),
            search: view.text.toLowerCase(),
        })
    }
    return withResult(leaves)
}

/**
 * The last end entry becomes the result card, holding the agent's final reply, which is then not
 * repeated above it. A note the end entry repeats (a failure, a usage limit) is dropped.
 */
function withResult(leaves: LogLeaf[]): LogLeaf[] {
    let end = -1
    leaves.forEach((leaf, index) => {
        if (leaf.type === 'entry' && END_KINDS.has(leaf.kind)) end = index
    })
    const endLeaf = leaves[end]
    if (!endLeaf || endLeaf.type !== 'entry') return leaves
    const endText = endLeaf.view.text.toLowerCase()

    let start = end
    while (start > 0) {
        const before = leaves[start - 1]
        if (before?.type !== 'entry') break
        if (before.kind === 'agent_job_message') start--
        else if (before.kind === 'agent_job_progress' && repeats(endText, before.search)) start--
        else break
    }
    const above = leaves.slice(start, end) as EntryLeaf[]
    const reply = above
        .filter((leaf) => leaf.kind === 'agent_job_message')
        .map((leaf) => leaf.view.text)
        .join('\n\n')
    const failed = endLeaf.kind !== 'agent_job_completed'
    const result: ResultLeaf = {
        type: 'result',
        key: endLeaf.key,
        sequence: endLeaf.sequence,
        at: endLeaf.at,
        view: endLeaf.view,
        reply,
        categories: failed ? ['conversation', 'lotar', 'problems'] : ['conversation', 'lotar'],
        search: `${reply}\n${endLeaf.view.text}`.toLowerCase(),
    }
    return [...leaves.slice(0, start), result, ...leaves.slice(end + 1)]
}

// A note the end entry says again, in full or in part.
const repeats = (endText: string, note: string) =>
    note.length >= 12 && (endText.includes(note.trim()) || note.includes(endText.trim()))

export type LogCounts = Record<LogView | 'thinking', number>

/** How many entries each view shows: a chip's count. */
export function countLeaves(leaves: LogLeaf[]): LogCounts {
    const counts: LogCounts = { all: 0, conversation: 0, tools: 0, lotar: 0, problems: 0, thinking: 0 }
    for (const leaf of leaves) {
        if (leaf.categories.includes('thinking')) {
            counts.thinking++
            continue
        }
        counts.all++
        for (const category of leaf.categories) if (category !== 'thinking') counts[category]++
    }
    return counts
}

export interface LogLayoutOptions {
    view: LogView
    thinking: boolean
    query: string
}

function summarize(calls: ToolLeaf[]): string {
    const counts = new Map<string, number>()
    for (const leaf of calls) counts.set(leaf.call.kind, (counts.get(leaf.call.kind) ?? 0) + 1)
    return NOUNS.filter(([kind]) => counts.has(kind))
        .map(([kind, one, many]) => {
            const count = counts.get(kind) ?? 0
            return `${count} ${count === 1 ? one : many}`
        })
        .join(' · ')
}

/** The leaves a view and search keep, tool calls in a row folded together, setup notes folded in "All". */
export function layoutLog(leaves: LogLeaf[], options: LogLayoutOptions): LogItem[] {
    const query = options.query.trim().toLowerCase()
    const kept = leaves.filter((leaf) => {
        if (leaf.categories.includes('thinking')) {
            if (!options.thinking) return false
        } else if (options.view !== 'all' && !leaf.categories.includes(options.view)) {
            return false
        }
        return !query || leaf.search.includes(query)
    })

    const items: LogItem[] = []
    let index = 0
    if (options.view === 'all' && !query) {
        // Setup: the notes lotar writes before the agent's first turn.
        const setup: EntryLeaf[] = []
        for (const leaf of kept) {
            if (leaf.type !== 'entry' || leaf.categories.join() !== 'lotar') break
            setup.push(leaf)
        }
        if (setup.length > 1) {
            const first = setup[0] as EntryLeaf
            items.push({ type: 'setup', key: `setup-${first.key}`, at: first.at, entries: setup })
            index = setup.length
        }
    }
    for (; index < kept.length; index++) {
        const leaf = kept[index] as LogLeaf
        if (leaf.type !== 'tool') {
            items.push(leaf)
            continue
        }
        const last = items[items.length - 1]
        if (last?.type === 'tools') {
            last.calls.push(leaf)
            continue
        }
        items.push({ type: 'tools', key: `tools-${leaf.key}`, at: leaf.at, calls: [leaf], summary: '', failed: 0 })
    }
    for (const item of items) {
        if (item.type !== 'tools') continue
        item.summary = summarize(item.calls)
        item.failed = item.calls.filter((leaf) => leaf.call.status === 'failed').length
    }
    return items
}

/** Lines of output shown before "Show all". */
export const OUTPUT_LINES = 12

/** The start of a long output, and whether lines were left out. */
export function clipOutput(text: string, lines = OUTPUT_LINES): { text: string; clipped: boolean } {
    const all = text.split('\n')
    if (all.length <= lines) return { text, clipped: false }
    return { text: all.slice(0, lines).join('\n'), clipped: true }
}

/** What a running job does now: the newest call still running, else thinking. */
export function liveActivity(leaves: LogLeaf[]): { call?: ToolCall; thinking: boolean } {
    for (let index = leaves.length - 1; index >= 0; index--) {
        const leaf = leaves[index]
        if (leaf?.type === 'tool' && leaf.call.status === 'running') return { call: leaf.call, thinking: false }
        if (leaf?.type === 'tool') return { thinking: false }
        if (leaf?.type === 'entry' && leaf.kind === 'agent_job_thought') return { thinking: true }
        if (leaf?.type === 'entry' && leaf.kind === 'agent_job_message') return { thinking: false }
    }
    return { thinking: false }
}
