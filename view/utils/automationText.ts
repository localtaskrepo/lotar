import type { TaskDTO } from '../api/types'
import type { AutomationEventKey, AutomationRecord } from './automationRules'

/**
 * Plain-language summaries of automation rules and help with `${{...}}`
 * placeholders for the rule editor (DEV-241).
 */

const TRIGGER_TEXT: Record<AutomationEventKey, string> = {
    start: 'a task is created or changed',
    created: 'a task is created',
    updated: 'a task changes',
    assigned: 'a task is assigned',
    commented: 'someone comments',
    sprint_changed: 'a task moves between sprints',
    job_start: 'an agent job starts',
    complete: 'an agent job finishes',
    error: 'an agent job fails',
    cancel: 'an agent job is cancelled',
    manual: 'it is run manually or from cron',
}

const HOOK_TO_EVENT: Record<string, AutomationEventKey> = {
    start: 'start',
    match: 'start',
    created: 'created',
    updated: 'updated',
    assigned: 'assigned',
    commented: 'commented',
    sprint_changed: 'sprint_changed',
    job_start: 'job_start',
    job_started: 'job_start',
    complete: 'complete',
    success: 'complete',
    job_success: 'complete',
    job_completed: 'complete',
    error: 'error',
    failure: 'error',
    job_failure: 'error',
    job_failed: 'error',
    cancel: 'cancel',
    job_cancel: 'cancel',
    job_cancelled: 'cancel',
    manual: 'manual',
}

const FIELD_NAMES: Record<string, string> = {
    status: 'status',
    priority: 'priority',
    type: 'type',
    task_type: 'type',
    assignee: 'assignee',
    reporter: 'reporter',
    tags: 'tags',
    labels: 'tags',
    title: 'title',
    description: 'description',
    due_date: 'due date',
    effort: 'effort',
    created: 'created',
    modified: 'last changed',
    status_changed: 'time in current status',
    sprint: 'sprint',
    sprints: 'sprint',
}

export function triggerText(event: AutomationEventKey): string {
    return TRIGGER_TEXT[event]
}

function asList(value: unknown): string[] {
    if (Array.isArray(value)) return value.map(String)
    if (value == null) return []
    return [String(value)]
}

function quote(text: string, max = 60): string {
    const single = text.replace(/\s+/g, ' ').trim()
    return `“${single.length > max ? `${single.slice(0, max - 1)}…` : single}”`
}

/** One condition (`field: value` or `field: { operator: value }`) in words. */
export function describeCondition(field: string, condition: unknown): string {
    const name = field.startsWith('custom_fields.')
        ? field.slice('custom_fields.'.length)
        : (FIELD_NAMES[field] ?? field)
    if (typeof condition === 'string') return `${name} is ${condition}`
    if (Array.isArray(condition)) return `${name} is ${condition.join(' or ')}`
    if (!condition || typeof condition !== 'object') return `${name} ${String(condition)}`
    const map = condition as AutomationRecord
    const pastField = field === 'created' || field === 'modified' || field === 'status_changed'
    const parts: string[] = []
    if (map.equals != null) parts.push(`${name} is ${String(map.equals)}`)
    if (map.in != null) parts.push(`${name} is ${asList(map.in).join(' or ')}`)
    if (map.contains != null) parts.push(`${name} contains ${String(map.contains)}`)
    if (map.any != null) parts.push(`${name} include any of ${asList(map.any).join(', ')}`)
    if (map.all != null) parts.push(`${name} include all of ${asList(map.all).join(', ')}`)
    if (map.none != null) parts.push(`${name} include none of ${asList(map.none).join(', ')}`)
    if (map.starts_with != null) parts.push(`${name} starts with ${String(map.starts_with)}`)
    if (map.matches != null) parts.push(`${name} matches /${String(map.matches)}/`)
    if (map.exists != null) parts.push(map.exists ? `${name} is set` : `${name} is empty`)
    if (map.before != null) parts.push(`${name} is before ${String(map.before)}`)
    if (map.older_than != null) {
        parts.push(
            field === 'status_changed'
                ? `in its current status for more than ${String(map.older_than)}`
                : `${name} more than ${String(map.older_than)} ago`,
        )
    }
    if (map.within != null) {
        parts.push(
            pastField
                ? field === 'status_changed'
                    ? `entered its current status in the last ${String(map.within)}`
                    : `${name} in the last ${String(map.within)}`
                : `${name} within the next ${String(map.within)}`,
        )
    }
    return parts.join(' and ') || `${name} (condition)`
}

/** The `when:` block as a list of readable conditions, plus how they combine. */
export function describeConditions(when: unknown): { join: 'all' | 'any'; items: string[] } {
    if (!when || typeof when !== 'object' || Array.isArray(when)) return { join: 'all', items: [] }
    const record = when as AutomationRecord
    const join: 'all' | 'any' = Array.isArray(record.any) ? 'any' : 'all'
    const items: string[] = []
    const walk = (value: unknown) => {
        if (!value || typeof value !== 'object' || Array.isArray(value)) return
        const entry = value as AutomationRecord
        for (const [key, inner] of Object.entries(entry)) {
            if (key === 'all' || key === 'any') {
                if (Array.isArray(inner)) inner.forEach(walk)
                continue
            }
            if (key === 'not') {
                items.push('not (…)')
                continue
            }
            if (key === 'changes' && inner && typeof inner === 'object') {
                for (const [field, change] of Object.entries(inner as AutomationRecord)) {
                    const changeRecord = (change ?? {}) as AutomationRecord
                    const from = changeRecord.from != null ? ` from ${asList(changeRecord.from).join(' or ')}` : ''
                    const to = changeRecord.to != null ? ` to ${asList(changeRecord.to).join(' or ')}` : ''
                    items.push(`${FIELD_NAMES[field] ?? field} changes${from}${to}`)
                }
                continue
            }
            if (key === 'custom_fields' && inner && typeof inner === 'object') {
                for (const [field, condition] of Object.entries(inner as AutomationRecord)) {
                    items.push(describeCondition(`custom_fields.${field}`, condition))
                }
                continue
            }
            items.push(describeCondition(key, inner))
        }
    }
    walk(record)
    return { join, items }
}

/** One action (`on.<hook>`) as readable steps. */
export function describeActionSteps(action: unknown): string[] {
    if (!action || typeof action !== 'object' || Array.isArray(action)) return []
    const record = action as AutomationRecord
    const steps: string[] = []
    if (record.set && typeof record.set === 'object') {
        for (const [key, value] of Object.entries(record.set as AutomationRecord)) {
            if (value == null) continue
            if (key === 'custom_fields' && typeof value === 'object') {
                for (const [field, fieldValue] of Object.entries(value as AutomationRecord)) {
                    steps.push(`set ${field} to ${String(fieldValue)}`)
                }
                continue
            }
            if (key === 'tags' || key === 'labels') {
                steps.push(`replace tags with ${asList(value).join(', ')}`)
                continue
            }
            const name = FIELD_NAMES[key] ?? key
            steps.push(
                key === 'description' || key === 'title'
                    ? `set ${name} to ${quote(String(value))}`
                    : `set ${name} to ${String(value)}`,
            )
        }
    }
    for (const [verb, section] of [
        ['add', record.add],
        ['remove', record.remove],
    ] as const) {
        if (!section || typeof section !== 'object') continue
        for (const [key, value] of Object.entries(section as AutomationRecord)) {
            const values = asList(value).join(', ')
            if (key === 'tags' || key === 'labels') steps.push(`${verb} tag ${values}`)
            else if (key === 'sprint')
                steps.push(verb === 'add' ? `add to sprint ${values}` : `remove from sprint ${values}`)
            else if (key === 'depends_on' || key === 'blocked_by') steps.push(`${verb} “depends on” ${values}`)
            else if (key === 'blocks') steps.push(`${verb} “blocks” ${values}`)
            else steps.push(`${verb} ${key.replace('_', ' ')} ${values}`)
        }
    }
    if (typeof record.comment === 'string' && record.comment.trim()) steps.push(`comment ${quote(record.comment)}`)
    if (record.run) {
        if (typeof record.run === 'string') steps.push(`run ${quote(record.run, 40)}`)
        else if (typeof record.run === 'object') {
            const run = record.run as AutomationRecord
            const line = [run.command, ...(Array.isArray(run.args) ? run.args : [])].filter(Boolean).join(' ')
            steps.push(line ? `run ${quote(line, 40)}` : 'run a command')
        }
    }
    if (record.agent && typeof record.agent === 'object') {
        const agent = record.agent as AutomationRecord
        const settings = [
            agent.model ? `model ${String(agent.model)}` : '',
            agent.effort ? `effort ${String(agent.effort)}` : '',
            agent.prompt ? 'a custom prompt' : '',
        ].filter(Boolean)
        if (settings.length) steps.push(`run the agent with ${settings.join(', ')}`)
    }
    if (record.archive === true) steps.push('archive the task')
    if (record.archive === false) steps.push('bring the task back from the archive')
    return steps
}

export interface AutomationRuleDescription {
    name: string
    enabled: boolean
    manual: boolean
    conditions: { join: 'all' | 'any'; items: string[] }
    triggers: Array<{ event: AutomationEventKey; when: string; steps: string[] }>
}

/** A rule as readable sentences: one per trigger, so each event shows only its own steps. */
export function describeAutomationRule(rule: AutomationRecord, index = 0): AutomationRuleDescription {
    const on = rule.on && typeof rule.on === 'object' && !Array.isArray(rule.on) ? (rule.on as AutomationRecord) : {}
    const triggers: AutomationRuleDescription['triggers'] = []
    for (const [hook, action] of Object.entries(on)) {
        const event = HOOK_TO_EVENT[hook]
        if (!event) continue
        triggers.push({ event, when: TRIGGER_TEXT[event], steps: describeActionSteps(action) })
    }
    return {
        name: typeof rule.name === 'string' && rule.name.trim() ? rule.name : `Rule ${index + 1}`,
        enabled: rule.enabled !== false,
        manual: triggers.some((trigger) => trigger.event === 'manual'),
        conditions: describeConditions(rule.when),
        triggers,
    }
}

// ── Placeholders ────────────────────────────────────────────────────────────

export interface TemplateVariable {
    /** What goes between `${{` and `}}`. */
    key: string
    label: string
    group: 'Task' | 'Before the change' | 'Comment' | 'Agent job'
}

const TASK_VARIABLES: Array<[string, string]> = [
    ['id', 'ID'],
    ['title', 'Title'],
    ['status', 'Status'],
    ['priority', 'Priority'],
    ['type', 'Type'],
    ['assignee', 'Assignee'],
    ['reporter', 'Reporter'],
    ['due_date', 'Due date'],
    ['effort', 'Effort'],
    ['tags[*]', 'All tags'],
    ['description', 'Description'],
    ['comments[-1].text', 'Latest comment'],
    ['references[*].link', 'All links'],
]

const CHANGE_EVENTS = new Set<AutomationEventKey>(['updated', 'assigned', 'sprint_changed', 'start'])
const JOB_EVENTS = new Set<AutomationEventKey>(['job_start', 'complete', 'error', 'cancel'])

/** The placeholders a rule can use, given the events it runs on. Values
 * only some events provide are offered when one of them is selected. */
export function templateVariables(events: AutomationEventKey[], customFields: string[] = []): TemplateVariable[] {
    const variables: TemplateVariable[] = TASK_VARIABLES.map(([key, label]) => ({
        key: `ticket.${key}`,
        label,
        group: 'Task',
    }))
    for (const field of customFields) {
        variables.push({ key: `ticket.field:${field}`, label: field, group: 'Task' })
    }
    if (events.some((event) => CHANGE_EVENTS.has(event))) {
        for (const [key, label] of TASK_VARIABLES.slice(0, 8)) {
            variables.push({ key: `previous.${key}`, label: `${label} before`, group: 'Before the change' })
        }
    }
    if (events.includes('commented')) {
        variables.push({ key: 'comment.text', label: 'The new comment', group: 'Comment' })
    }
    if (events.some((event) => JOB_EVENTS.has(event))) {
        for (const [key, label] of [
            ['profile', 'Agent profile'],
            ['runner', 'Runner'],
            ['job_id', 'Job ID'],
            ['worktree_branch', 'Worktree branch'],
            ['worktree_path', 'Worktree path'],
            ['cost_usd', 'Cost (USD), after the job'],
            ['turns', 'Turns, after the job'],
        ] as const) {
            variables.push({ key: `agent.${key}`, label, group: 'Agent job' })
        }
    }
    return variables
}

function readTaskValue(task: TaskDTO, path: string): string | null {
    const field = path.startsWith('field:') ? null : path
    if (path.startsWith('field:')) {
        const value = task.custom_fields?.[path.slice('field:'.length)]
        return value == null ? '' : String(value)
    }
    const listMatch = /^(tags|comments|references)\[(\*|-?\d+)\](?:\.(\w+))?$/.exec(path)
    if (listMatch) {
        const [, list, selector, prop] = listMatch
        const items: unknown[] =
            list === 'tags' ? (task.tags ?? []) : list === 'comments' ? (task.comments ?? []) : (task.references ?? [])
        const pick = (item: unknown) => {
            if (item == null) return ''
            if (typeof item !== 'object') return String(item)
            const record = item as Record<string, unknown>
            if (prop) return record[prop] == null ? '' : String(record[prop])
            if (list === 'comments') return String(record.text ?? '')
            const first = Object.values(record).find((value) => typeof value === 'string' && value)
            return first ? String(first) : ''
        }
        if (selector === '*') return items.map(pick).filter(Boolean).join(', ')
        const index = Number(selector)
        const item = index < 0 ? items[items.length + index] : items[index]
        return item === undefined ? '' : pick(item)
    }
    switch (field) {
        case 'id':
        case 'title':
        case 'status':
        case 'priority':
        case 'assignee':
        case 'reporter':
        case 'description':
        case 'due_date':
        case 'effort': {
            const value = (task as unknown as Record<string, unknown>)[field]
            return value == null ? '' : String(value)
        }
        case 'type':
            return String(task.task_type ?? '')
        case 'tags':
            return (task.tags ?? []).join(', ')
        default:
            return null
    }
}

/** Fill `${{ticket.*}}` placeholders from `task`. Values that only exist
 * when the rule runs (`previous.*`, `comment.*`, `agent.*`) stay as
 * ‹name› so the preview shows where they go. */
export function previewTemplate(text: string, task: TaskDTO | null): string {
    return text.replace(/\$\{\{\s*([^}]+?)\s*\}\}/g, (_match, key: string) => {
        if (task && key.startsWith('ticket.')) {
            const value = readTaskValue(task, key.slice('ticket.'.length))
            if (value !== null) return value
        }
        return `‹${key}›`
    })
}

/** The `${{` placeholder being typed just before `caret`, if any. */
export function openPlaceholderAt(text: string, caret: number): { start: number; query: string } | null {
    const before = text.slice(0, caret)
    const start = before.lastIndexOf('${{')
    if (start === -1) return null
    const inside = before.slice(start + 3)
    if (inside.includes('}}') || /\s/.test(inside)) return null
    return { start, query: inside }
}
