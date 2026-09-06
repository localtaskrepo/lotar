import { listFromCsv } from './useFilterBuilder'
/**
 * Search-grammar engine for the unified filter toolbar.
 *
 * The search box accepts plain text plus `key:value` tokens, e.g.
 *   `login bug status:todo priority:high sprint:2 tag:ci "multi word"`
 * Plain words become the free-text query; recognized tokens become structured
 * filters; unknown `key:value` tokens pass through as custom field filters.
 */

export interface ParsedFilterQuery {
    /** Free-text words (joined by spaces). */
    text: string
    /** Structured filter tokens keyed by their canonical field name. */
    filters: Record<string, string>
}

export interface GrammarKeyMeta {
    /** Canonical field name emitted into the filter value. */
    key: string
    /** Aliases accepted in queries (lowercase, no separators). */
    aliases: string[]
    /** Human label shown in suggestions and chips. */
    label: string
    /** Which value style the key expects. */
    values: 'options' | 'assignee' | 'due' | 'recent' | 'flag' | 'text'
    /** Option list id, resolved by the caller (e.g. 'statuses', 'sprints'). */
    optionsFrom?: 'statuses' | 'priorities' | 'types' | 'sprints' | 'projects' | 'customFields'
}

export const GRAMMAR_KEYS: GrammarKeyMeta[] = [
    { key: 'status', aliases: ['status', 'state'], label: 'Status', values: 'options', optionsFrom: 'statuses' },
    { key: 'priority', aliases: ['priority', 'prio'], label: 'Priority', values: 'options', optionsFrom: 'priorities' },
    { key: 'type', aliases: ['type', 'tasktype', 'kind'], label: 'Type', values: 'options', optionsFrom: 'types' },
    { key: 'sprints', aliases: ['sprint', 'sprints'], label: 'Sprint', values: 'options', optionsFrom: 'sprints' },
    { key: 'project', aliases: ['project', 'projectkey'], label: 'Project', values: 'options', optionsFrom: 'projects' },
    { key: 'assignee', aliases: ['assignee', 'owner'], label: 'Assignee', values: 'assignee' },
    { key: 'tags', aliases: ['tag', 'tags'], label: 'Tag', values: 'text' },
    { key: 'due', aliases: ['due', 'duedate', 'dueon'], label: 'Due', values: 'due' },
    { key: 'recent', aliases: ['recent'], label: 'Recent', values: 'recent' },
    { key: 'needs', aliases: ['needs', 'need'], label: 'Needs', values: 'text' },
    { key: 'mine', aliases: ['mine', 'me'], label: 'Mine', values: 'flag' },
]

const ALIAS_INDEX: Record<string, GrammarKeyMeta> = (() => {
    const index: Record<string, GrammarKeyMeta> = {}
    for (const meta of GRAMMAR_KEYS) {
        for (const alias of meta.aliases) index[alias] = meta
    }
    return index
})()

/** Fixed value lists for non-option keys. */
export const GRAMMAR_FIXED_VALUES: Partial<Record<string, string[]>> = {
    due: ['today', 'soon', 'overdue', 'week', 'month'],
    recent: ['1d', '7d', '30d'],
}

export function findGrammarKey(token: string): GrammarKeyMeta | null {
    return ALIAS_INDEX[token.toLowerCase()] ?? null
}

/** Split a raw query into word and `key:value` tokens, honoring double quotes. */
function tokenize(input: string): string[] {
    const tokens: string[] = []
    let current = ''
    let quote: '"' | "'" | null = null
    const push = () => {
        const trimmed = current.trim()
        if (trimmed) tokens.push(trimmed)
        current = ''
    }
    for (const ch of input) {
        if (quote) {
            if (ch === quote) quote = null
            else current += ch
            continue
        }
        if (ch === '"' || ch === "'") {
            quote = ch
            continue
        }
        if (/\s/.test(ch)) {
            push()
            continue
        }
        current += ch
    }
    push()
    return tokens
}

function unquote(value: string): string {
    if (value.length >= 2) {
        const first = value[0]
        const last = value[value.length - 1]
        if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
            return value.slice(1, -1)
        }
    }
    return value
}

const TOKEN_RE = /^([a-zA-Z][a-zA-Z0-9_:-]*):(.*)$/

export function parseFilterQuery(input: string): ParsedFilterQuery {
    const filters: Record<string, string> = {}
    const words: string[] = []
    for (const token of tokenize(input)) {
        const match = TOKEN_RE.exec(token)
        if (match) {
            const [, rawKey, rawValue] = match
            const key = findGrammarKey(rawKey ?? '')
            const value = unquote(rawValue ?? '').trim()
            if (key && value) {
                if (key.values === 'flag') {
                    filters[key.key] = 'true'
                } else {
                    filters[key.key] = value
                }
                continue
            }
            // Unknown key:value → treat as free text so the query is not lost.
            words.push(token)
            continue
        }
        words.push(token)
    }
    return { text: words.join(' '), filters }
}

export interface FilterValueSource {
    statuses?: string[]
    priorities?: string[]
    types?: string[]
    sprints?: Array<{ id: number; label: string }>
    projects?: Array<{ prefix: string; name?: string }>
    customFields?: string[]
}

export interface SuggestionItem {
    /** What to insert into the input when picked (including the `key:` prefix). */
    insert: string
    /** Display label. */
    label: string
    /** Secondary hint (e.g. the value kind). */
    hint?: string
}

function optionsFor(meta: GrammarKeyMeta, source: FilterValueSource): string[] {
    switch (meta.optionsFrom) {
        case 'statuses':
            return source.statuses ?? []
        case 'priorities':
            return source.priorities ?? []
        case 'types':
            return source.types ?? []
        case 'sprints':
            return (source.sprints ?? []).map((s) => String(s.id))
        case 'projects':
            return (source.projects ?? []).map((p) => p.prefix)
        default:
            return []
    }
}

function valueSuggestions(meta: GrammarKeyMeta, source: FilterValueSource): SuggestionItem[] {
    if (meta.values === 'options') {
        return optionsFor(meta, source).map((value) => ({
            insert: `${meta.key}:${value}`,
            label: value,
            hint: meta.label,
        }))
    }
    if (meta.values === 'assignee') {
        return [
            { insert: 'assignee:@me', label: 'Me', hint: 'Assignee' },
            { insert: 'assignee:__none__', label: 'No assignee', hint: 'Assignee' },
        ]
    }
    if (meta.values === 'due' || meta.values === 'recent') {
        return (GRAMMAR_FIXED_VALUES[meta.key] ?? []).map((value) => ({
            insert: `${meta.key}:${value}`,
            label: value,
            hint: meta.label,
        }))
    }
    if (meta.values === 'flag') {
        return [{ insert: `${meta.key}:true`, label: meta.label, hint: 'Toggle' }]
    }
    return []
}

function optionCandidates(meta: GrammarKeyMeta, source: FilterValueSource): Array<{ value: string; label: string }> {
    switch (meta.optionsFrom) {
        case 'sprints':
            return (source.sprints ?? []).map((s) => ({ value: String(s.id), label: s.label }))
        case 'projects':
            return (source.projects ?? []).map((p) => ({ value: p.prefix, label: p.prefix }))
        default:
            return optionsFor(meta, source).map((value) => ({ value, label: value }))
    }
}

/**
 * Suggest completions for the token currently being typed.
 * Returns key suggestions before a `:`, value suggestions after it, and
 * value matches for bare words that look like partial values ("todo").
 */
export function suggestForFragment(fragment: string, source: FilterValueSource): SuggestionItem[] {
    const trimmed = fragment.trimStart()
    const colon = trimmed.indexOf(':')
    if (colon === -1) {
        const lower = trimmed.toLowerCase()
        const keyMatches = lower
            ? GRAMMAR_KEYS.filter((meta) => meta.aliases.some((a) => a.startsWith(lower)))
            : GRAMMAR_KEYS.filter((meta) => meta.values !== 'flag')
        if (!lower || keyMatches.length) {
            return keyMatches.slice(0, 8).map((meta) => ({
                insert: `${meta.key}:`,
                label: `${meta.key}:`,
                hint: meta.label,
            }))
        }
        // Bare word matching no key alias: offer value completions across
        // option-backed keys so partial matches stay keyboard-selectable.
        const matches: SuggestionItem[] = []
        for (const meta of GRAMMAR_KEYS) {
            if (meta.values !== 'options') continue
            for (const candidate of optionCandidates(meta, source)) {
                if (candidate.label.toLowerCase().includes(lower)) {
                    matches.push({ insert: `${meta.key}:${candidate.value}`, label: candidate.label, hint: meta.label })
                    if (matches.length >= 10) return matches
                }
            }
        }
        return matches
    }
    const rawKey = trimmed.slice(0, colon).toLowerCase()
    const rawValue = unquote(trimmed.slice(colon + 1))
    const meta = findGrammarKey(rawKey)
    if (!meta) return []
    const lower = rawValue.toLowerCase()
    return valueSuggestions(meta, source)
        .filter((sug) => !lower || sug.label.toLowerCase().includes(lower))
        .slice(0, 10)
}

/** A structured filter rendered as a dismissible chip. */
export interface FilterChip {
    /** Canonical filter key. */
    key: string
    /** Display label. */
    label: string
    /** Filter value (single CSV entry or the whole value for simple keys). */
    value: string
    /** Optional display override for the value (e.g. sprint names). */
    display?: string
}

export function chipsForFilterValue(value: Record<string, string>, source: FilterValueSource): FilterChip[] {
    const chips: FilterChip[] = []
    const sprintLabels = new Map((source.sprints ?? []).map((s) => [String(s.id), s.label]))
    for (const [key, raw] of Object.entries(value)) {
        if (!raw) continue
        if (key === 'order') continue
        const meta = GRAMMAR_KEYS.find((m) => m.key === key)
        const label = meta?.label ?? key
        if (key === 'project') {
            chips.push({ key, label: 'Project', value: raw, display: raw })
            continue
        }
        if (key === 'mine' && raw === 'true') {
            chips.push({ key, label: 'Mine', value: 'true' })
            continue
        }
        if (key === 'assignee') {
            if (raw === '@me') chips.push({ key, label: 'Mine', value: raw })
            else if (raw === '__none__') chips.push({ key, label: 'No assignee', value: raw })
            else chips.push({ key, label, value: raw })
            continue
        }
        if (key === 'due') {
            chips.push({ key, label: 'Due', value: raw, display: raw })
            continue
        }
        if (key === 'recent') {
            chips.push({ key, label: 'Recent', value: raw, display: raw })
            continue
        }
        if (key === 'needs') {
            for (const part of listFromCsv(raw)) {
                chips.push({ key, label: `Needs ${part}`, value: part })
            }
            continue
        }
        if (key === 'q' || key === 'tags') {
            continue
        }
        // CSV-backed multi-selects: one chip per value.
        for (const v of listFromCsv(raw)) {
            const display = key === 'sprints' ? (sprintLabels.get(v) ?? `#${v}`) : v
            chips.push({ key, label, value: v, display })
        }
    }
    return chips
}
