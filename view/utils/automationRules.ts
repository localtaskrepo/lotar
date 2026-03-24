import { parse as parseYaml, stringify as stringifyYaml } from 'yaml'

export type AutomationEventKey =
    | 'start'
    | 'created'
    | 'updated'
    | 'assigned'
    | 'commented'
    | 'sprint_changed'
    | 'job_start'
    | 'complete'
    | 'error'
    | 'cancel'

export type AutomationConditionOperator =
    | 'equals'
    | 'in'
    | 'contains'
    | 'any'
    | 'all'
    | 'none'
    | 'starts_with'
    | 'exists'
    | 'matches'
    | 'before'
    | 'within'
    | 'older_than'

export interface AutomationConditionDraft {
    id: string
    scope: 'field' | 'custom_field'
    field: string
    customFieldKey: string
    operator: AutomationConditionOperator
    value: string
}

export interface AutomationChangeDraft {
    id: string
    field: string
    from: string
    to: string
}

export interface AutomationSetFieldDraft {
    id: string
    scope: 'field' | 'custom_field'
    field: string
    customFieldKey: string
    value: string
}

export interface AutomationListFieldDraft {
    id: string
    field: 'tags' | 'labels' | 'sprint' | 'depends_on' | 'blocks' | 'related'
    value: string
}

export interface AutomationRunEnvDraft {
    id: string
    key: string
    value: string
}

export interface AutomationEventActionDraft {
    event: AutomationEventKey
    enabled: boolean
    comment: string
    setFields: AutomationSetFieldDraft[]
    addFields: AutomationListFieldDraft[]
    removeFields: AutomationListFieldDraft[]
    runMode: 'none' | 'shell' | 'command'
    runShell: string
    runCommand: string
    runArgs: string
    runCwd: string
    runWait: boolean
    runIgnoreFailure: boolean
    runEnv: AutomationRunEnvDraft[]
}

export interface AutomationRuleDraft {
    name: string
    cooldown: string
    matchMode: 'all' | 'any'
    conditions: AutomationConditionDraft[]
    changeConditions: AutomationChangeDraft[]
    eventActions: AutomationEventActionDraft[]
}

export interface AutomationRuleSummary {
    name?: string
    events: string[]
    conditions: string[]
    actions: string[]
}

export interface AutomationParsedDocument {
    rules: Record<string, any>[]
    maxIterations: string
    parseError: string | null
}

export interface AutomationEditableRuleState {
    editable: boolean
    draft: AutomationRuleDraft
    unsupportedReasons: string[]
}

type NamedOption<T extends string> = {
    value: T
    label: string
}

type ConditionFieldOptionValue =
    | 'status'
    | 'assignee'
    | 'reporter'
    | 'priority'
    | 'type'
    | 'tags'
    | 'title'
    | 'description'
    | 'due_date'
    | 'effort'
    | 'custom_field'
    | 'other'

type SetFieldOptionValue =
    | 'status'
    | 'assignee'
    | 'reporter'
    | 'priority'
    | 'type'
    | 'due_date'
    | 'effort'
    | 'title'
    | 'description'
    | 'tags'
    | 'labels'
    | 'custom_field'

let rowCounter = 0

const EVENT_LABELS: Record<AutomationEventKey, string> = {
    start: 'Match',
    created: 'Task created',
    updated: 'Task updated',
    assigned: 'Assigned',
    commented: 'Comment added',
    sprint_changed: 'Sprint changed',
    job_start: 'Job started',
    complete: 'Job completed',
    error: 'Job failed',
    cancel: 'Job cancelled',
}

const EVENT_ALIAS_TO_KEY: Record<string, AutomationEventKey> = {
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
}

const YAML_EVENT_KEYS: Record<AutomationEventKey, string> = {
    start: 'start',
    created: 'created',
    updated: 'updated',
    assigned: 'assigned',
    commented: 'commented',
    sprint_changed: 'sprint_changed',
    job_start: 'job_start',
    complete: 'complete',
    error: 'error',
    cancel: 'cancel',
}

const CONDITION_OPERATORS: AutomationConditionOperator[] = [
    'equals',
    'in',
    'contains',
    'any',
    'all',
    'none',
    'starts_with',
    'exists',
    'matches',
    'before',
    'within',
    'older_than',
]

const SUPPORTED_RULE_KEYS = new Set(['name', 'cooldown', 'when', 'triggers', 'on'])
const SUPPORTED_ACTION_KEYS = new Set(['set', 'add', 'remove', 'run', 'comment'])
const SUPPORTED_WHEN_KEYS = new Set(['all', 'any', 'changes', 'custom_fields'])
const SUPPORTED_SET_FIELDS = new Set([
    'status',
    'assignee',
    'reporter',
    'priority',
    'type',
    'due_date',
    'effort',
    'title',
    'description',
    'tags',
    'labels',
    'custom_fields',
])
const SUPPORTED_LIST_FIELDS = new Set(['tags', 'labels', 'sprint', 'depends_on', 'blocks', 'related'])

export const automationEventOptions: NamedOption<AutomationEventKey>[] = (
    Object.keys(EVENT_LABELS) as AutomationEventKey[]
).map((value) => ({ value, label: EVENT_LABELS[value] }))

export const automationConditionOperatorOptions: NamedOption<AutomationConditionOperator>[] = CONDITION_OPERATORS.map((value) => ({
    value,
    label: value.replace('_', ' '),
}))

export const automationConditionFieldOptions: NamedOption<ConditionFieldOptionValue>[] = [
    { value: 'status', label: 'Status' },
    { value: 'assignee', label: 'Assignee' },
    { value: 'reporter', label: 'Reporter' },
    { value: 'priority', label: 'Priority' },
    { value: 'type', label: 'Type' },
    { value: 'tags', label: 'Tags' },
    { value: 'title', label: 'Title' },
    { value: 'description', label: 'Description' },
    { value: 'due_date', label: 'Due date' },
    { value: 'effort', label: 'Effort' },
    { value: 'custom_field', label: 'Custom field' },
    { value: 'other', label: 'Other field' },
]

export const automationSetFieldOptions: NamedOption<SetFieldOptionValue>[] = [
    { value: 'status', label: 'Status' },
    { value: 'assignee', label: 'Assignee' },
    { value: 'reporter', label: 'Reporter' },
    { value: 'priority', label: 'Priority' },
    { value: 'type', label: 'Type' },
    { value: 'due_date', label: 'Due date' },
    { value: 'effort', label: 'Effort' },
    { value: 'title', label: 'Title' },
    { value: 'description', label: 'Description' },
    { value: 'tags', label: 'Tags' },
    { value: 'labels', label: 'Labels' },
    { value: 'custom_field', label: 'Custom field' },
]

export const automationListFieldOptions: NamedOption<AutomationListFieldDraft['field']>[] = [
    { value: 'tags', label: 'Tags' },
    { value: 'labels', label: 'Labels' },
    { value: 'sprint', label: 'Sprint' },
    { value: 'depends_on', label: 'Depends on' },
    { value: 'blocks', label: 'Blocks' },
    { value: 'related', label: 'Related' },
]

export function parseAutomationYamlDocument(yaml: string): AutomationParsedDocument {
    if (!yaml.trim()) {
        return { rules: [], maxIterations: '', parseError: null }
    }

    try {
        const parsed = parseYaml(yaml)
        if (Array.isArray(parsed)) {
            return { rules: sanitizeRuleList(parsed), maxIterations: '', parseError: null }
        }

        const automation = parsed && typeof parsed === 'object' && 'automation' in parsed
            ? (parsed as Record<string, any>).automation
            : parsed

        if (Array.isArray(automation)) {
            return { rules: sanitizeRuleList(automation), maxIterations: '', parseError: null }
        }

        const rules = Array.isArray((automation as Record<string, any> | undefined)?.rules)
            ? sanitizeRuleList((automation as Record<string, any>).rules)
            : []

        const maxIterations = (automation as Record<string, any> | undefined)?.max_iterations
        return {
            rules,
            maxIterations: maxIterations == null ? '' : String(maxIterations),
            parseError: null,
        }
    } catch (error) {
        return {
            rules: [],
            maxIterations: '',
            parseError: error instanceof Error ? error.message : String(error),
        }
    }
}

export function buildAutomationYamlDocument(rules: Record<string, any>[], maxIterations = ''): string {
    const trimmedMaxIterations = maxIterations.trim()
    if (!rules.length && !trimmedMaxIterations) {
        return ''
    }

    const automation: Record<string, any> = { rules }
    if (trimmedMaxIterations) {
        const numeric = Number(trimmedMaxIterations)
        automation.max_iterations = Number.isFinite(numeric) && `${numeric}` === trimmedMaxIterations
            ? numeric
            : trimmedMaxIterations
    }

    return stringifyYaml({ automation })
}

export function summarizeAutomationRule(rule: Record<string, any>): AutomationRuleSummary {
    const events = summarizeEvents(rule)
    const actions = summarizeActions(rule, events)
    const conditions = summarizeConditions(rule?.when)

    return {
        name: typeof rule?.name === 'string' ? rule.name : undefined,
        events: events.map((event) => EVENT_LABELS[event] ?? event),
        conditions,
        actions,
    }
}

export function getEditableRuleState(rule: Record<string, any>): AutomationEditableRuleState {
    const unsupportedReasons = collectUnsupportedRuleReasons(rule)
    return {
        editable: unsupportedReasons.length === 0,
        draft: createRuleDraftFromRule(rule),
        unsupportedReasons,
    }
}

export function createEmptyRuleDraft(): AutomationRuleDraft {
    return {
        name: '',
        cooldown: '',
        matchMode: 'all',
        conditions: [createConditionDraft()],
        changeConditions: [],
        eventActions: automationEventOptions.map((option) => createEventActionDraft(option.value)),
    }
}

export function buildAutomationRuleFromDraft(draft: AutomationRuleDraft): Record<string, any> {
    const rule: Record<string, any> = {}
    if (draft.name.trim()) {
        rule.name = draft.name.trim()
    }
    if (draft.cooldown.trim()) {
        rule.cooldown = draft.cooldown.trim()
    }

    const when = buildWhenFromDraft(draft)
    if (Object.keys(when).length) {
        rule.when = when
    }

    const on: Record<string, any> = {}
    draft.eventActions.forEach((actionDraft) => {
        if (!actionDraft.enabled) return
        const action = buildActionFromDraft(actionDraft)
        if (Object.keys(action).length === 0) return
        on[YAML_EVENT_KEYS[actionDraft.event]] = action
    })
    rule.on = on

    return rule
}

export function formatAutomationRuleLabel(summary: AutomationRuleSummary, index: number): string {
    return summary.name?.trim() || `Rule ${index + 1}`
}

function sanitizeRuleList(value: unknown[]): Record<string, any>[] {
    return value
        .filter((entry) => entry && typeof entry === 'object' && !Array.isArray(entry))
        .map((entry) => compactAutomationRecord(cloneRecord(entry as Record<string, any>)))
        .filter((entry) => Object.keys(entry).length > 0)
}

function summarizeEvents(rule: Record<string, any>): AutomationEventKey[] {
    const on = rule?.on
    if (!on || typeof on !== 'object') return []
    return Object.keys(on)
        .map((key) => normalizeEventKey(key))
        .filter((value): value is AutomationEventKey => Boolean(value))
}

function summarizeConditions(when: unknown): string[] {
    if (!when || typeof when !== 'object' || Array.isArray(when)) return []
    const conditionRecord = when as Record<string, any>
    const conditions: string[] = []

    for (const [key, value] of Object.entries(conditionRecord)) {
        if (key === 'all' || key === 'any') {
            const nested = Array.isArray(value) ? value.length : 0
            if (nested) {
                conditions.push(`${key.toUpperCase()} (${nested})`)
            }
            continue
        }
        if (key === 'custom_fields' && value && typeof value === 'object') {
            for (const [customKey, customValue] of Object.entries(value)) {
                conditions.push(`custom:${customKey} ${summarizeFieldCondition(customValue)}`)
            }
            continue
        }
        if (key === 'changes' && value && typeof value === 'object') {
            for (const [field, change] of Object.entries(value)) {
                const parts: string[] = []
                if (change && typeof change === 'object' && !Array.isArray(change)) {
                    const changeRecord = change as Record<string, any>
                    if (changeRecord.from != null) {
                        parts.push(`from ${summarizeFieldCondition(changeRecord.from)}`)
                    }
                    if (changeRecord.to != null) {
                        parts.push(`to ${summarizeFieldCondition(changeRecord.to)}`)
                    }
                }
                conditions.push(`changes ${field}${parts.length ? ` (${parts.join(', ')})` : ''}`)
            }
            continue
        }
        if (key === 'not') {
            conditions.push('NOT (...)')
            continue
        }
        conditions.push(`${key} ${summarizeFieldCondition(value)}`)
    }

    return conditions
}

function summarizeActions(rule: Record<string, any>, events: AutomationEventKey[]): string[] {
    const on = rule?.on
    if (!on || typeof on !== 'object' || !events.length) return ['No actions']
    return events.flatMap((event) => {
        const action = (on as Record<string, any>)[YAML_EVENT_KEYS[event]]
            ?? (on as Record<string, any>)[denormalizeEventKey(event)]
        const summary = summarizeAction(action)
        return summary.map((line) => `${EVENT_LABELS[event]}: ${line}`)
    })
}

function summarizeAction(action: unknown): string[] {
    if (!action || typeof action !== 'object' || Array.isArray(action)) return ['(empty)']
    const record = action as Record<string, any>
    const summary: string[] = []

    if (record.set && typeof record.set === 'object') {
        for (const [key, value] of Object.entries(record.set)) {
            if (value == null) continue
            if (key === 'custom_fields' && value && typeof value === 'object') {
                const fields = Object.keys(value as Record<string, unknown>)
                if (fields.length) {
                    summary.push(`set custom fields: ${fields.join(', ')}`)
                }
                continue
            }
            summary.push(`set ${key}=${formatValue(value)}`)
        }
    }

    for (const [prefix, section] of [
        ['add', record.add],
        ['remove', record.remove],
    ] as const) {
        if (!section || typeof section !== 'object' || Array.isArray(section)) continue
        for (const [key, value] of Object.entries(section)) {
            if (value == null) continue
            summary.push(`${prefix} ${key}: ${formatValue(value)}`)
        }
    }

    if (record.comment) {
        summary.push(`comment: ${formatValue(record.comment)}`)
    }

    if (record.run) {
        if (typeof record.run === 'string') {
            summary.push(`run: ${record.run}`)
        } else if (typeof record.run === 'object') {
            const cmd = record.run.command || ''
            const args = Array.isArray(record.run.args) ? record.run.args.join(' ') : ''
            const line = [cmd, args].filter(Boolean).join(' ')
            summary.push(line ? `run: ${line}` : 'run: command')
        }
    }

    return summary.length ? summary : ['(empty)']
}

function summarizeFieldCondition(value: unknown): string {
    if (Array.isArray(value)) {
        return `in [${value.join(', ')}]`
    }
    if (typeof value === 'string') {
        return `= ${value}`
    }
    if (!value || typeof value !== 'object') {
        return String(value)
    }

    const record = value as Record<string, any>
    for (const operator of CONDITION_OPERATORS) {
        if (!(operator in record)) continue
        return `${operator} ${formatValue(record[operator])}`
    }
    return formatValue(record)
}

function formatValue(value: unknown): string {
    if (Array.isArray(value)) return value.join(', ')
    if (value && typeof value === 'object') return JSON.stringify(value)
    return value == null ? '' : String(value)
}

function collectUnsupportedRuleReasons(rule: Record<string, any>): string[] {
    const reasons: string[] = []

    for (const key of Object.keys(rule)) {
        if (!SUPPORTED_RULE_KEYS.has(key)) {
            reasons.push(`Unsupported rule key: ${key}`)
        }
    }

    reasons.push(...collectUnsupportedWhenReasons(rule.when))

    const on = rule.on
    if (!on || typeof on !== 'object' || Array.isArray(on)) {
        reasons.push('Rule must contain an on: block.')
    } else {
        for (const [rawKey, action] of Object.entries(on)) {
            const eventKey = normalizeEventKey(rawKey)
            if (!eventKey) {
                reasons.push(`Unsupported event hook: ${rawKey}`)
                continue
            }
            reasons.push(...collectUnsupportedActionReasons(action, eventKey))
        }
    }

    return reasons
}

function collectUnsupportedWhenReasons(when: unknown, depth = 0): string[] {
    if (!when) return []
    if (typeof when !== 'object' || Array.isArray(when)) {
        return ['when must be an object.']
    }

    const reasons: string[] = []
    const record = when as Record<string, any>

    if ('not' in record) {
        reasons.push('NOT conditions are currently YAML-only.')
    }

    for (const key of Object.keys(record)) {
        if (SUPPORTED_WHEN_KEYS.has(key)) continue
        if (key !== 'not' && !isSupportedFieldCondition(record[key])) {
            reasons.push(`Condition for ${key} uses an unsupported shape.`)
        }
    }

    if (record.custom_fields && typeof record.custom_fields === 'object') {
        for (const [key, value] of Object.entries(record.custom_fields)) {
            if (!isSupportedFieldCondition(value)) {
                reasons.push(`Custom field condition for ${key} uses an unsupported shape.`)
            }
        }
    }

    if (record.changes && typeof record.changes === 'object') {
        for (const [key, value] of Object.entries(record.changes)) {
            if (!value || typeof value !== 'object' || Array.isArray(value)) {
                reasons.push(`Change condition for ${key} is invalid.`)
                continue
            }
            const changeRecord = value as Record<string, any>
            if (changeRecord.from != null && !isSupportedFieldCondition(changeRecord.from)) {
                reasons.push(`Change condition for ${key}.from uses an unsupported shape.`)
            }
            if (changeRecord.to != null && !isSupportedFieldCondition(changeRecord.to)) {
                reasons.push(`Change condition for ${key}.to uses an unsupported shape.`)
            }
        }
    }

    for (const combinator of ['all', 'any'] as const) {
        if (!(combinator in record)) continue
        const entries = record[combinator]
        if (!Array.isArray(entries)) {
            reasons.push(`${combinator} must be a list.`)
            continue
        }
        if (depth > 0) {
            reasons.push(`Nested ${combinator.toUpperCase()} groups are currently YAML-only.`)
            continue
        }
        entries.forEach((entry) => {
            reasons.push(...collectUnsupportedWhenReasons(entry, depth + 1))
        })
    }

    return reasons
}

function collectUnsupportedActionReasons(action: unknown, event: AutomationEventKey): string[] {
    if (!action || typeof action !== 'object' || Array.isArray(action)) {
        return [`${EVENT_LABELS[event]} action is invalid.`]
    }

    const record = action as Record<string, any>
    const reasons: string[] = []
    for (const key of Object.keys(record)) {
        if (!SUPPORTED_ACTION_KEYS.has(key)) {
            reasons.push(`${EVENT_LABELS[event]} uses unsupported action key: ${key}`)
        }
    }

    if (record.set && typeof record.set === 'object' && !Array.isArray(record.set)) {
        for (const [key, value] of Object.entries(record.set)) {
            if (!SUPPORTED_SET_FIELDS.has(key)) {
                reasons.push(`${EVENT_LABELS[event]} set.${key} is not editable in the builder.`)
                continue
            }
            if (key === 'custom_fields') {
                if (!value || typeof value !== 'object' || Array.isArray(value)) {
                    reasons.push(`${EVENT_LABELS[event]} custom_fields must be a map.`)
                    continue
                }
                for (const [customKey, customValue] of Object.entries(value as Record<string, unknown>)) {
                    if (!isScalarYamlValue(customValue)) {
                        reasons.push(`${EVENT_LABELS[event]} custom_fields.${customKey} must be a scalar value.`)
                    }
                }
            }
        }
    }

    for (const key of ['add', 'remove'] as const) {
        const section = record[key]
        if (!section) continue
        if (typeof section !== 'object' || Array.isArray(section)) {
            reasons.push(`${EVENT_LABELS[event]} ${key} action must be a map.`)
            continue
        }
        for (const [field, value] of Object.entries(section)) {
            if (!SUPPORTED_LIST_FIELDS.has(field)) {
                reasons.push(`${EVENT_LABELS[event]} ${key}.${field} is not editable in the builder.`)
                continue
            }
            if (!isScalarOrListValue(value)) {
                reasons.push(`${EVENT_LABELS[event]} ${key}.${field} must be a scalar or list.`)
            }
        }
    }

    if (record.run) {
        if (typeof record.run === 'string') {
            return reasons
        }
        if (typeof record.run !== 'object' || Array.isArray(record.run)) {
            reasons.push(`${EVENT_LABELS[event]} run action must be a string or command map.`)
            return reasons
        }
        for (const key of Object.keys(record.run)) {
            if (!['command', 'args', 'env', 'cwd', 'ignore_failure', 'wait'].includes(key)) {
                reasons.push(`${EVENT_LABELS[event]} run.${key} is not editable in the builder.`)
            }
        }
        if (record.run.env && typeof record.run.env === 'object' && !Array.isArray(record.run.env)) {
            for (const value of Object.values(record.run.env as Record<string, unknown>)) {
                if (typeof value !== 'string') {
                    reasons.push(`${EVENT_LABELS[event]} run.env values must be strings.`)
                    break
                }
            }
        }
    }

    return reasons
}

function isSupportedFieldCondition(value: unknown): boolean {
    if (typeof value === 'string') return true
    if (Array.isArray(value)) return value.every((entry) => typeof entry === 'string')
    if (!value || typeof value !== 'object') return false
    const record = value as Record<string, unknown>
    const active = CONDITION_OPERATORS.filter((operator) => operator in record)
    if (active.length !== 1) return false
    const currentValue = record[active[0]]
    if (active[0] === 'exists') {
        return typeof currentValue === 'boolean'
    }
    return isScalarOrListValue(currentValue)
}

function isScalarOrListValue(value: unknown): boolean {
    return typeof value === 'string' || (Array.isArray(value) && value.every((entry) => typeof entry === 'string'))
}

function isScalarYamlValue(value: unknown): boolean {
    return ['string', 'number', 'boolean'].includes(typeof value) || value == null
}

function createRuleDraftFromRule(rule: Record<string, any>): AutomationRuleDraft {
    const when = extractWhen(rule.when)
    const on = rule.on && typeof rule.on === 'object' && !Array.isArray(rule.on) ? rule.on as Record<string, any> : {}

    return {
        name: typeof rule.name === 'string' ? rule.name : '',
        cooldown: typeof rule.cooldown === 'string' ? rule.cooldown : '',
        matchMode: when.matchMode,
        conditions: when.conditions.length ? when.conditions : [createConditionDraft()],
        changeConditions: when.changeConditions,
        eventActions: automationEventOptions.map((option) => {
            const action = resolveActionForEvent(on, option.value)
            return extractEventActionDraft(option.value, action)
        }),
    }
}

function extractWhen(value: unknown): Pick<AutomationRuleDraft, 'matchMode' | 'conditions' | 'changeConditions'> {
    const conditions: AutomationConditionDraft[] = []
    const changeConditions: AutomationChangeDraft[] = []
    let matchMode: 'all' | 'any' = 'all'

    if (!value || typeof value !== 'object' || Array.isArray(value)) {
        return { matchMode, conditions, changeConditions }
    }

    const record = value as Record<string, any>
    if (Array.isArray(record.any)) {
        matchMode = 'any'
        record.any.forEach((entry) => extractLeafConditions(entry, conditions, changeConditions))
    } else if (Array.isArray(record.all)) {
        matchMode = 'all'
        record.all.forEach((entry) => extractLeafConditions(entry, conditions, changeConditions))
    } else {
        extractLeafConditions(record, conditions, changeConditions)
    }

    return { matchMode, conditions, changeConditions }
}

function extractLeafConditions(
    value: unknown,
    conditions: AutomationConditionDraft[],
    changeConditions: AutomationChangeDraft[],
) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return
    const record = value as Record<string, any>

    if (record.custom_fields && typeof record.custom_fields === 'object') {
        for (const [key, fieldCondition] of Object.entries(record.custom_fields)) {
            conditions.push(conditionToDraft(fieldCondition, key, 'custom_field'))
        }
    }

    if (record.changes && typeof record.changes === 'object') {
        for (const [field, change] of Object.entries(record.changes)) {
            if (!change || typeof change !== 'object' || Array.isArray(change)) continue
            const changeRecord = change as Record<string, unknown>
            changeConditions.push({
                id: nextRowId('change'),
                field,
                from: stringifyConditionValue(changeRecord.from),
                to: stringifyConditionValue(changeRecord.to),
            })
        }
    }

    for (const [field, fieldCondition] of Object.entries(record)) {
        if (SUPPORTED_WHEN_KEYS.has(field) || field === 'not') continue
        conditions.push(conditionToDraft(fieldCondition, field, 'field'))
    }
}

function conditionToDraft(value: unknown, field: string, scope: 'field' | 'custom_field'): AutomationConditionDraft {
    if (typeof value === 'string') {
        return {
            id: nextRowId('condition'),
            scope,
            field: scope === 'field' ? field : 'custom_field',
            customFieldKey: scope === 'custom_field' ? field : '',
            operator: 'equals',
            value,
        }
    }

    if (Array.isArray(value)) {
        return {
            id: nextRowId('condition'),
            scope,
            field: scope === 'field' ? field : 'custom_field',
            customFieldKey: scope === 'custom_field' ? field : '',
            operator: 'any',
            value: value.join(', '),
        }
    }

    const record = value && typeof value === 'object' ? value as Record<string, any> : {}
    const operator = CONDITION_OPERATORS.find((candidate) => candidate in record) ?? 'equals'
    return {
        id: nextRowId('condition'),
        scope,
        field: scope === 'field' ? field : 'custom_field',
        customFieldKey: scope === 'custom_field' ? field : '',
        operator,
        value: stringifyConditionValue(record[operator]),
    }
}

function stringifyConditionValue(value: unknown): string {
    if (value == null) return ''
    if (Array.isArray(value)) return value.join(', ')
    if (typeof value === 'boolean') return value ? 'true' : 'false'
    return String(value)
}

function extractEventActionDraft(event: AutomationEventKey, action: unknown): AutomationEventActionDraft {
    if (!action || typeof action !== 'object' || Array.isArray(action)) {
        return createEventActionDraft(event)
    }

    const record = action as Record<string, any>
    const setFields: AutomationSetFieldDraft[] = []
    const addFields: AutomationListFieldDraft[] = []
    const removeFields: AutomationListFieldDraft[] = []

    if (record.set && typeof record.set === 'object' && !Array.isArray(record.set)) {
        for (const [key, value] of Object.entries(record.set)) {
            if (key === 'custom_fields' && value && typeof value === 'object' && !Array.isArray(value)) {
                for (const [customKey, customValue] of Object.entries(value as Record<string, unknown>)) {
                    setFields.push({
                        id: nextRowId('set'),
                        scope: 'custom_field',
                        field: 'custom_field',
                        customFieldKey: customKey,
                        value: customValue == null ? '' : String(customValue),
                    })
                }
                continue
            }

            setFields.push({
                id: nextRowId('set'),
                scope: 'field',
                field: key === 'type' ? 'type' : key,
                customFieldKey: '',
                value: formatValue(value),
            })
        }
    }

    for (const [target, destination] of [
        [record.add, addFields],
        [record.remove, removeFields],
    ] as const) {
        if (!target || typeof target !== 'object' || Array.isArray(target)) continue
        for (const [key, value] of Object.entries(target)) {
            destination.push({
                id: nextRowId('list'),
                field: normalizeListField(key),
                value: formatValue(value),
            })
        }
    }

    const draft = createEventActionDraft(event)
    draft.enabled = hasActionContent(record)
    draft.comment = typeof record.comment === 'string' ? record.comment : ''
    draft.setFields = setFields
    draft.addFields = addFields
    draft.removeFields = removeFields

    if (typeof record.run === 'string') {
        draft.runMode = 'shell'
        draft.runShell = record.run
    } else if (record.run && typeof record.run === 'object' && !Array.isArray(record.run)) {
        draft.runMode = 'command'
        draft.runCommand = typeof record.run.command === 'string' ? record.run.command : ''
        draft.runArgs = Array.isArray(record.run.args) ? record.run.args.join(', ') : ''
        draft.runCwd = typeof record.run.cwd === 'string' ? record.run.cwd : ''
        draft.runWait = record.run.wait !== false
        draft.runIgnoreFailure = Boolean(record.run.ignore_failure)
        draft.runEnv = Object.entries(record.run.env || {}).map(([key, value]) => ({
            id: nextRowId('env'),
            key,
            value: typeof value === 'string' ? value : '',
        }))
    }

    return draft
}

function createEventActionDraft(event: AutomationEventKey): AutomationEventActionDraft {
    return {
        event,
        enabled: false,
        comment: '',
        setFields: [],
        addFields: [],
        removeFields: [],
        runMode: 'none',
        runShell: '',
        runCommand: '',
        runArgs: '',
        runCwd: '',
        runWait: true,
        runIgnoreFailure: false,
        runEnv: [],
    }
}

function hasActionContent(value: Record<string, any>): boolean {
    return Boolean(
        value.comment
        || value.run
        || (value.set && Object.keys(value.set).length)
        || (value.add && Object.keys(value.add).length)
        || (value.remove && Object.keys(value.remove).length),
    )
}

function buildWhenFromDraft(draft: AutomationRuleDraft): Record<string, any> {
    const entries = buildWhenEntries(draft.conditions, draft.changeConditions)
    if (!entries.length) {
        return {}
    }

    if (entries.length === 1) {
        return entries[0]
    }

    return draft.matchMode === 'any'
        ? { any: entries }
        : { all: entries }
}

function buildWhenEntries(
    conditions: AutomationConditionDraft[],
    changeConditions: AutomationChangeDraft[],
): Record<string, any>[] {
    const entries: Record<string, any>[] = []

    conditions.forEach((condition) => {
        const fieldCondition = buildFieldConditionFromDraft(condition)
        if (fieldCondition == null) return
        if (condition.scope === 'custom_field') {
            const customFieldKey = condition.customFieldKey.trim()
            if (!customFieldKey) return
            entries.push({ custom_fields: { [customFieldKey]: fieldCondition } })
            return
        }

        const fieldKey = condition.field.trim()
        if (!fieldKey || fieldKey === 'custom_field') return
        entries.push({ [fieldKey]: fieldCondition })
    })

    changeConditions.forEach((change) => {
        const field = change.field.trim()
        if (!field) return
        const changeRecord: Record<string, any> = {}
        if (change.from.trim()) {
            changeRecord.from = parseChangeConditionValue(change.from)
        }
        if (change.to.trim()) {
            changeRecord.to = parseChangeConditionValue(change.to)
        }
        if (Object.keys(changeRecord).length) {
            entries.push({ changes: { [field]: changeRecord } })
        }
    })

    return entries
}

function buildFieldConditionFromDraft(condition: AutomationConditionDraft): any {
    const value = condition.value.trim()
    if (!value && condition.operator !== 'exists') {
        return undefined
    }

    switch (condition.operator) {
        case 'equals':
            return value
        case 'in':
        case 'any':
        case 'all':
        case 'none':
            return { [condition.operator]: splitList(value) }
        case 'exists':
            return { exists: value !== 'false' }
        default:
            return { [condition.operator]: value }
    }
}

function parseChangeConditionValue(value: string): any {
    const trimmed = value.trim()
    if (!trimmed) return undefined
    if (trimmed.includes(',')) {
        return splitList(trimmed)
    }
    return trimmed
}

function buildActionFromDraft(actionDraft: AutomationEventActionDraft): Record<string, any> {
    const action: Record<string, any> = {}
    const set: Record<string, any> = {}
    const customFields: Record<string, any> = {}

    actionDraft.setFields.forEach((entry) => {
        const value = entry.value.trim()
        if (!value) return
        if (entry.scope === 'custom_field') {
            const key = entry.customFieldKey.trim()
            if (!key) return
            customFields[key] = parseYamlScalar(value)
            return
        }
        set[entry.field === 'type' ? 'type' : entry.field] = parseSetValue(entry.field, value)
    })

    if (Object.keys(customFields).length) {
        set.custom_fields = customFields
    }
    if (Object.keys(set).length) {
        action.set = set
    }

    const add = buildListAction(actionDraft.addFields)
    if (Object.keys(add).length) {
        action.add = add
    }

    const remove = buildListAction(actionDraft.removeFields)
    if (Object.keys(remove).length) {
        action.remove = remove
    }

    if (actionDraft.comment.trim()) {
        action.comment = actionDraft.comment.trim()
    }

    if (actionDraft.runMode === 'shell' && actionDraft.runShell.trim()) {
        action.run = actionDraft.runShell.trim()
    }

    if (actionDraft.runMode === 'command' && actionDraft.runCommand.trim()) {
        const run: Record<string, any> = {
            command: actionDraft.runCommand.trim(),
        }
        const args = splitList(actionDraft.runArgs)
        if (args.length) {
            run.args = args
        }
        if (actionDraft.runCwd.trim()) {
            run.cwd = actionDraft.runCwd.trim()
        }
        if (actionDraft.runIgnoreFailure) {
            run.ignore_failure = true
        }
        if (!actionDraft.runWait) {
            run.wait = false
        }
        const env = actionDraft.runEnv.reduce<Record<string, string>>((acc, entry) => {
            const key = entry.key.trim()
            if (!key) return acc
            acc[key] = entry.value
            return acc
        }, {})
        if (Object.keys(env).length) {
            run.env = env
        }
        action.run = run
    }

    return action
}

function parseSetValue(field: string, value: string): string | string[] {
    if (field === 'tags' || field === 'labels') {
        const values = splitList(value)
        return values.length > 1 ? values : values[0]
    }
    return value
}

function buildListAction(entries: AutomationListFieldDraft[]): Record<string, any> {
    return entries.reduce<Record<string, any>>((acc, entry) => {
        const value = entry.value.trim()
        if (!value) return acc
        const values = entry.field === 'sprint' ? value : splitList(value)
        acc[entry.field] = entry.field === 'sprint'
            ? value
            : values.length === 1
                ? values[0]
                : values
        return acc
    }, {})
}

function parseYamlScalar(value: string): unknown {
    try {
        return parseYaml(value)
    } catch {
        return value
    }
}

function splitList(value: string): string[] {
    return value
        .split(',')
        .map((entry) => entry.trim())
        .filter(Boolean)
}

function createConditionDraft(): AutomationConditionDraft {
    return {
        id: nextRowId('condition'),
        scope: 'field',
        field: 'status',
        customFieldKey: '',
        operator: 'equals',
        value: '',
    }
}

export function createChangeDraft(): AutomationChangeDraft {
    return {
        id: nextRowId('change'),
        field: 'status',
        from: '',
        to: '',
    }
}

export function createSetFieldDraft(): AutomationSetFieldDraft {
    return {
        id: nextRowId('set'),
        scope: 'field',
        field: 'status',
        customFieldKey: '',
        value: '',
    }
}

export function createListFieldDraft(field: AutomationListFieldDraft['field'] = 'tags'): AutomationListFieldDraft {
    return {
        id: nextRowId('list'),
        field,
        value: '',
    }
}

export function createRunEnvDraft(): AutomationRunEnvDraft {
    return {
        id: nextRowId('env'),
        key: '',
        value: '',
    }
}

export function normalizeConditionFieldSelection(row: AutomationConditionDraft, selection: ConditionFieldOptionValue) {
    if (selection === 'custom_field') {
        row.scope = 'custom_field'
        row.field = 'custom_field'
        return
    }
    row.scope = 'field'
    row.field = selection === 'other' ? '' : selection
    row.customFieldKey = ''
}

export function normalizeSetFieldSelection(row: AutomationSetFieldDraft, selection: SetFieldOptionValue) {
    if (selection === 'custom_field') {
        row.scope = 'custom_field'
        row.field = 'custom_field'
        return
    }
    row.scope = 'field'
    row.field = selection
    row.customFieldKey = ''
}

export function getConditionFieldSelection(row: AutomationConditionDraft): ConditionFieldOptionValue {
    if (row.scope === 'custom_field') return 'custom_field'
    if (automationConditionFieldOptions.some((option) => option.value === row.field)) {
        return row.field as ConditionFieldOptionValue
    }
    return 'other'
}

export function getSetFieldSelection(row: AutomationSetFieldDraft): SetFieldOptionValue {
    if (row.scope === 'custom_field') return 'custom_field'
    if (automationSetFieldOptions.some((option) => option.value === row.field)) {
        return row.field as SetFieldOptionValue
    }
    return 'status'
}

export function getAutomationEventLabel(event: AutomationEventKey): string {
    return EVENT_LABELS[event]
}

function resolveActionForEvent(on: Record<string, any>, event: AutomationEventKey): unknown {
    return on[YAML_EVENT_KEYS[event]] ?? on[denormalizeEventKey(event)]
}

function normalizeEventKey(rawKey: string): AutomationEventKey | null {
    return EVENT_ALIAS_TO_KEY[rawKey] ?? null
}

function denormalizeEventKey(event: AutomationEventKey): string {
    switch (event) {
        case 'job_start':
            return 'job_started'
        case 'complete':
            return 'job_completed'
        case 'error':
            return 'job_failed'
        case 'cancel':
            return 'job_cancelled'
        default:
            return event
    }
}

function normalizeListField(rawKey: string): AutomationListFieldDraft['field'] {
    if (rawKey === 'label' || rawKey === 'labels') return 'labels'
    if (rawKey === 'blocked_by') return 'depends_on'
    if (rawKey === 'references') return 'related'
    return (SUPPORTED_LIST_FIELDS.has(rawKey) ? rawKey : 'tags') as AutomationListFieldDraft['field']
}

function cloneRecord(value: Record<string, any>): Record<string, any> {
    return JSON.parse(JSON.stringify(value)) as Record<string, any>
}

function compactAutomationRecord(value: Record<string, any>): Record<string, any> {
    const compacted = compactAutomationValue(value)
    return compacted && typeof compacted === 'object' && !Array.isArray(compacted)
        ? compacted as Record<string, any>
        : {}
}

function compactAutomationValue(value: unknown): unknown {
    if (value == null) {
        return undefined
    }

    if (Array.isArray(value)) {
        const items = value
            .map((item) => compactAutomationValue(item))
            .filter((item) => item !== undefined)
        return items.length ? items : undefined
    }

    if (typeof value !== 'object') {
        return value
    }

    const record: Record<string, unknown> = {}
    for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
        const compacted = compactAutomationValue(entry)
        if (compacted === undefined) {
            continue
        }
        if (Array.isArray(compacted) && compacted.length === 0) {
            continue
        }
        if (compacted && typeof compacted === 'object' && !Array.isArray(compacted) && Object.keys(compacted).length === 0) {
            continue
        }
        record[key] = compacted
    }

    return Object.keys(record).length ? record : undefined
}

function nextRowId(prefix: string): string {
    rowCounter += 1
    return `${prefix}-${rowCounter}`
}