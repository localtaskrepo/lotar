import { listFromCsv } from './useFilterBuilder'
/**
 * Search-grammar engine for the unified filter toolbar.
 *
 * The search box accepts plain text plus `key:value` tokens, e.g.
 *   `login bug status:todo priority:high sprint:2 tag:ci "multi word"`
 * Plain words become the free-text query; recognized tokens become structured
 * filters. Explicit assignments from the legacy custom-filter box are also
 * supported: `field:name=value` (reserved names canonicalize onto builtin
 * controls, e.g. `field:STATE=Todo` -> status, `field:task_type=Bug` -> type;
 * quoted names stay literal custom fields, e.g. `field:"sprint"=inc-2`),
 * anything else stays `field:<lowercased name>`) and bare `key=value` pairs
 * (keys are used as-is, so `owner=ops` stays a custom `owner` filter while
 * `owner:ops` means assignee).
 *
 * Unknown `key:value` tokens, URLs and fully quoted literals stay free text so
 * no typed content is silently turned into (or lost from) a filter. Malformed
 * explicit assignments and unmatched quotes are reported through `errors` and
 * leave the offending token uncommitted; the caller decides whether to reject
 * the whole draft when errors are present.
 */

export interface ParsedFilterQuery {
    /** Free-text words (joined by spaces). */
    text: string
    /** Structured filter tokens keyed by their canonical field name. */
    filters: Record<string, string>
    /** Malformed explicit assignment/field token or unmatched quote messages. */
    errors?: string[]
    /**
     * Canonical filter keys whose final contributing token was an `=`
     * assignment (bare `key=value` or `field:name=value`, including reserved
     * aliases). Parents use it to REPLACE control selections for these keys
     * while `key:value` tokens MERGE into them; the last token for a key
     * decides both the value and the policy. Absent when no assignment
     * contributed (pure colon/free-text queries keep the old shape).
     */
    replaceKeys?: string[]
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
    const index: Record<string, GrammarKeyMeta> = Object.create(null)
    for (const meta of GRAMMAR_KEYS) {
        for (const alias of meta.aliases) index[alias] = meta
    }
    return index
})()

// Reserved names accepted behind an explicit `field:` prefix. Mirrors the
// former FilterBar RESERVED_FIELD_ALIASES (q/query/text/textquery/search,
// order/sort, sort_by/sortby) plus the deletion routing key and every grammar
// alias, so `field:STATE=x` and `field:task_type=x` reach the builtin controls.
const RESERVED_FIELD_ALIASES: Record<string, string> = (() => {
    const map: Record<string, string> = Object.assign(Object.create(null), {
        q: 'q',
        query: 'q',
        text: 'q',
        textquery: 'q',
        search: 'q',
        order: 'order',
        sort: 'order',
        sort_by: 'sort_by',
        sortby: 'sort_by',
        deletion: 'deletion',
    })
    for (const meta of GRAMMAR_KEYS) {
        for (const alias of meta.aliases) map[alias] = meta.key
        map[meta.key] = meta.key
    }
    return map
})()

function normalizeReservedKey(input: string): string {
    return input.toLowerCase().replace(/[-_\s]+/g, '')
}

/** Fixed value lists for non-option keys. */
export const GRAMMAR_FIXED_VALUES: Partial<Record<string, string[]>> = {
    due: ['today', 'soon', 'overdue', 'week', 'month'],
    recent: ['1d', '7d', '30d'],
}

export function findGrammarKey(token: string): GrammarKeyMeta | null {
    return ALIAS_INDEX[token.toLowerCase()] ?? null
}

type QuoteChar = '"' | "'"

interface Segment {
    /** Unescaped segment content. */
    value: string
    /** True when the whole segment was a matched quoted string. */
    quoted: boolean
    /** False when a leading quote never closed at the segment end. */
    ok: boolean
}

/**
 * Interpret one raw segment: a cleanly quoted string unescapes to its literal
 * content, anything else is returned verbatim. Backslash escapes only apply
 * inside quotes (`\"`, `\'`, `\\`); a lone backslash elsewhere is literal.
 */
function unquoteSegment(raw: string): Segment {
    const quote = raw[0]
    if (quote !== '"' && quote !== "'") return { value: raw, quoted: false, ok: true }
    let value = ''
    for (let i = 1; i < raw.length; i++) {
        const ch = raw[i]!
        if (ch === '\\' && i + 1 < raw.length) {
            const next = raw[i + 1]!
            if (next === quote || next === '\\') {
                value += next
                i++
                continue
            }
        }
        if (ch === quote) {
            return i === raw.length - 1 ? { value, quoted: true, ok: true } : { value: raw, quoted: false, ok: false }
        }
        value += ch
    }
    return { value: raw, quoted: false, ok: false }
}

/** First index of `target` outside quotes, or -1. */
function findTopLevel(raw: string, target: string): number {
    let quote: QuoteChar | null = null
    for (let i = 0; i < raw.length; i++) {
        const ch = raw[i]!
        if (quote) {
            if (ch === '\\') {
                const next = raw[i + 1]
                if (next === quote || next === '\\') {
                    i++
                    continue
                }
            }
            if (ch === quote) quote = null
            continue
        }
        if (ch === '"' || ch === "'") {
            quote = ch
            continue
        }
        if (ch === target) return i
    }
    return -1
}

/** Split on separator characters outside quotes; keeps raw segment text. */
function splitTopLevel(raw: string, isSep: (ch: string) => boolean): string[] {
    const parts: string[] = []
    let current = ''
    let quote: QuoteChar | null = null
    for (let i = 0; i < raw.length; i++) {
        const ch = raw[i]!
        if (quote) {
            if (ch === '\\') {
                const next = raw[i + 1]
                if (next === quote || next === '\\') {
                    current += ch + (next ?? '')
                    i++
                    continue
                }
            }
            if (ch === quote) quote = null
            current += ch
            continue
        }
        if (ch === '"' || ch === "'") {
            quote = ch
            current += ch
            continue
        }
        if (isSep(ch)) {
            parts.push(current)
            current = ''
            continue
        }
        current += ch
    }
    parts.push(current)
    return parts
}

/** Split the input into raw tokens (quotes kept) on top-level whitespace. */
function tokenize(input: string): string[] {
    return splitTopLevel(input, (ch) => /\s/.test(ch)).filter((token) => token.length > 0)
}

/** `scheme://` tokens (URLs) never become filters or get split apart. */
function isUrlLike(raw: string): boolean {
    return /^[A-Za-z][A-Za-z0-9+.-]*:\/\//.test(raw)
}

function isFieldKey(key: string): boolean {
    return key.toLowerCase().startsWith('field:')
}

/**
 * A segment counts as an explicit expression when it has the shape of an
 * assignment (`key=value`, `field:name=value`) or a known `key:value` token.
 * Used to decide whether top-level commas separate legacy expressions.
 */
function looksLikeExpression(segment: string): boolean {
    if (isUrlLike(segment)) return false
    const eq = findTopLevel(segment, '=')
    if (eq !== -1) {
        const keyRaw = segment.slice(0, eq)
        if (findTopLevel(keyRaw, ':') === -1) return true
        return isFieldKey(unquoteSegment(keyRaw).value)
    }
    const colon = findTopLevel(segment, ':')
    if (colon > 0) return findGrammarKey(unquoteSegment(segment.slice(0, colon)).value) !== null
    return false
}

/**
 * Split comma-delimited legacy expressions (`a=1, b=2`) into separate tokens,
 * but only when every non-empty comma segment unambiguously has expression
 * shape — quoted or comma-valued data (`tags:ci,ops`, `key="a,b"`) must never
 * be split apart.
 */
function splitCommaExpressions(token: string): string[] {
    const parts = splitTopLevel(token, (ch) => ch === ',')
    if (parts.length < 2) return [token]
    const nonEmpty = parts.filter((part) => part.length > 0)
    if (!nonEmpty.length || !nonEmpty.every(looksLikeExpression)) return [token]
    return nonEmpty
}

function parseAssignment(
    raw: string,
    eq: number,
    filters: Record<string, string>,
    errors: string[],
    assignmentKeys: Record<string, boolean>,
): void {
    const keySeg = unquoteSegment(raw.slice(0, eq))
    if (!keySeg.ok) {
        errors.push(`Unmatched quote in "${raw}"`)
        return
    }
    const valueSeg = unquoteSegment(raw.slice(eq + 1))
    if (!valueSeg.ok) {
        errors.push(`Unmatched quote in "${raw}"`)
        return
    }
    const keyText = keySeg.quoted ? keySeg.value : keySeg.value.trim()
    const value = valueSeg.quoted ? valueSeg.value : valueSeg.value.trim()
    const display = raw.slice(0, eq).trim() || raw

    if (isFieldKey(keyText)) {
        const nameSeg = unquoteSegment(keyText.slice(keyText.indexOf(':') + 1).trim())
        if (!nameSeg.ok) {
            errors.push(`Unmatched quote in "${raw}"`)
            return
        }
        const name = nameSeg.quoted ? nameSeg.value : nameSeg.value.trim()
        if (!name) {
            errors.push(`Invalid filter name "${display}"`)
            return
        }
        if (!value) {
            errors.push(`Add a value for "${display}"`)
            return
        }
        const builtin = nameSeg.quoted ? undefined : RESERVED_FIELD_ALIASES[normalizeReservedKey(name)]
        const key = builtin || `field:${name.toLowerCase()}`
        filters[key] = value
        assignmentKeys[key] = true
        return
    }

    if (!keyText) {
        errors.push('Missing key before "="')
        return
    }
    if (!value) {
        errors.push(`Add a value for "${keyText}"`)
        return
    }
    // Bare equality never maps aliases (legacy parseCustomFilter behavior):
    // `owner=ops` stays a custom `owner` filter, unlike `owner:ops`.
    const key = keyText.toLowerCase()
    filters[key] = value
    assignmentKeys[key] = true
}

function parseColonToken(
    raw: string,
    colon: number,
    filters: Record<string, string>,
    words: string[],
    errors: string[],
    assignmentKeys: Record<string, boolean>,
): void {
    const keySeg = unquoteSegment(raw.slice(0, colon))
    if (!keySeg.ok) {
        errors.push(`Unmatched quote in "${raw}"`)
        return
    }
    const meta = findGrammarKey(keySeg.value.trim())
    // Unknown `key:value` (including URLs and `field:name` without `=`)
    // stays free text so typed content is preserved verbatim.
    if (!meta) {
        words.push(raw)
        return
    }
    const valueSeg = unquoteSegment(raw.slice(colon + 1))
    if (!valueSeg.ok) {
        errors.push(`Unmatched quote in "${raw}"`)
        return
    }
    const value = valueSeg.quoted ? valueSeg.value : valueSeg.value.trim()
    if (!value) {
        words.push(raw)
        return
    }
    filters[meta.key] = meta.values === 'flag' ? 'true' : value
    // A later colon token switches the key's policy back to merge semantics.
    assignmentKeys[meta.key] = false
}

function parseToken(
    raw: string,
    filters: Record<string, string>,
    words: string[],
    errors: string[],
    assignmentKeys: Record<string, boolean>,
): void {
    // A fully quoted literal is explicit free text ("a=b", "status:todo").
    const whole = unquoteSegment(raw)
    if (whole.quoted) {
        words.push(whole.value)
        return
    }
    if (isUrlLike(raw)) {
        words.push(raw)
        return
    }
    const eq = findTopLevel(raw, '=')
    const colon = findTopLevel(raw, ':')
    if (eq !== -1 && (colon === -1 || colon > eq || isFieldKey(unquoteSegment(raw.slice(0, eq)).value))) {
        parseAssignment(raw, eq, filters, errors, assignmentKeys)
        return
    }
    if (colon !== -1) {
        parseColonToken(raw, colon, filters, words, errors, assignmentKeys)
        return
    }
    words.push(raw)
}

export function parseFilterQuery(input: string): ParsedFilterQuery {
    const filters: Record<string, string> = Object.create(null)
    const words: string[] = []
    const errors: string[] = []
    const assignmentKeys: Record<string, boolean> = Object.create(null)
    for (const rawToken of tokenize(input)) {
        for (const token of splitCommaExpressions(rawToken)) {
            parseToken(token, filters, words, errors, assignmentKeys)
        }
    }
    const result: ParsedFilterQuery = { text: words.join(' '), filters }
    if (errors.length) result.errors = errors
    const replaceKeys = Object.keys(assignmentKeys).filter((key) => assignmentKeys[key])
    if (replaceKeys.length) result.replaceKeys = replaceKeys
    return result
}

/**
 * Canonical form of a custom-filter key as stored in filter values:
 * `field:NAME` canonicalizes reserved names onto builtin keys and lowercases
 * custom names; bare keys are only lowercased (no alias mapping).
 */
export function canonicalCustomFilterKey(raw: string): string {
    const trimmed = raw.trim()
    if (!trimmed) return ''
    if (isFieldKey(trimmed)) {
        const nameSeg = unquoteSegment(trimmed.slice(trimmed.indexOf(':') + 1).trim())
        if (!nameSeg.ok) return ''
        const name = nameSeg.quoted ? nameSeg.value : nameSeg.value.trim()
        if (!name) return ''
        const builtin = nameSeg.quoted ? undefined : RESERVED_FIELD_ALIASES[normalizeReservedKey(name)]
        return builtin || `field:${name.toLowerCase()}`
    }
    const seg = unquoteSegment(trimmed)
    const key = seg.quoted ? seg.value : trimmed
    return key.toLowerCase()
}

const SAFE_KEY = /^[A-Za-z0-9_-]+$/
const SAFE_VALUE = /^[A-Za-z0-9_:@/+.?&%#-]+$/

function escapeQuoted(value: string): string {
    return value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')
}

function quoteIfNeeded(value: string, safe: RegExp): string {
    return safe.test(value) ? value : `"${escapeQuoted(value)}"`
}

/**
 * Render a filter entry as a single safe draft token. Custom keys keep the
 * `field:` prefix; names and values are quoted (and escaped) whenever they
 * contain characters that would otherwise change the parse. Reserved custom
 * names are also quoted to avoid routing them onto builtin controls. An empty value
 * produces an editable field prefix (`field:iteration=` / `owner=`).
 */
export function serializeFilterToken(key: string, value: string): string {
    const trimmedKey = key.trim()
    let keyPart: string
    if (isFieldKey(trimmedKey)) {
        const name = trimmedKey.slice(trimmedKey.indexOf(':') + 1).trim()
        const literalName = RESERVED_FIELD_ALIASES[normalizeReservedKey(name)]
            ? `"${escapeQuoted(name)}"` : quoteIfNeeded(name, SAFE_KEY)
        keyPart = `field:${name ? literalName : ''}`
    } else {
        keyPart = quoteIfNeeded(trimmedKey, SAFE_KEY)
    }
    const trimmedValue = value.trim()
    return `${keyPart}=${trimmedValue ? quoteIfNeeded(trimmedValue, SAFE_VALUE) : ''}`
}

/**
 * Quote-aware split of a draft into the text before the final token and the
 * final token itself (the portion a suggestion should replace). Whitespace
 * inside quoted segments never starts a new fragment.
 */
export function filterFragment(input: string): { prefix: string; fragment: string } {
    let start = 0
    let quote: QuoteChar | null = null
    for (let i = 0; i < input.length; i++) {
        const ch = input[i]!
        if (quote) {
            if (ch === '\\') {
                const next = input[i + 1]
                if (next === quote || next === '\\') {
                    i++
                    continue
                }
            }
            if (ch === quote) quote = null
            continue
        }
        if (ch === '"' || ch === "'") {
            quote = ch
            continue
        }
        if (/\s/.test(ch)) start = i + 1
    }
    return { prefix: input.slice(0, start), fragment: input.slice(start) }
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

function customFieldPrefixSuggestion(): SuggestionItem {
    return { insert: 'field:', label: 'field:', hint: 'Custom field' }
}

function customFieldSuggestions(partial: string, source: FilterValueSource): SuggestionItem[] {
    const needle = partial.replace(/^["']/, '').toLowerCase()
    return (source.customFields ?? [])
        .filter((name) => name.toLowerCase().startsWith(needle))
        .slice(0, 10)
        .map((name) => ({
            insert: serializeFilterToken(`field:${name}`, ''),
            label: name,
            hint: 'Custom field',
        }))
}

/**
 * Suggest completions for the token currently being typed.
 * Returns key suggestions before a `:`, value suggestions after it, and
 * value matches for bare words that look like partial values ("todo").
 * `field:` offers custom field names from the source, inserting an editable
 * `field:<quoted name>=` prefix.
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
            const items: SuggestionItem[] = keyMatches.slice(0, 8).map((meta) => ({
                insert: `${meta.key}:`,
                label: `${meta.key}:`,
                hint: meta.label,
            }))
            if ('field'.startsWith(lower)) items.push(customFieldPrefixSuggestion())
            return items
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
        if (!matches.length && 'field'.startsWith(lower)) matches.push(customFieldPrefixSuggestion())
        return matches
    }
    const rawKey = trimmed.slice(0, colon).toLowerCase()
    const rawValue = unquote(trimmed.slice(colon + 1))
    if (rawKey === 'field') return customFieldSuggestions(rawValue, source)
    const meta = findGrammarKey(rawKey)
    if (!meta) return []
    const lower = rawValue.toLowerCase()
    return valueSuggestions(meta, source)
        .filter((sug) => !lower || sug.label.toLowerCase().includes(lower))
        .slice(0, 10)
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
        if (!meta) {
            // Unknown keys (custom extras like `field:iteration` or `owner`)
            // render as one chip so comma-containing values stay intact and
            // removal can address the whole entry by key.
            chips.push({
                key,
                label: key.startsWith('field:') ? key.slice('field:'.length) : key,
                value: raw,
                display: raw,
            })
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
