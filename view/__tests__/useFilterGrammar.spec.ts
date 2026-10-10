import { describe, expect, it } from 'vitest'
import {
    canonicalCustomFilterKey,
    chipsForFilterValue,
    filterFragment,
    findGrammarKey,
    parseFilterQuery,
    serializeFilterToken,
    suggestForFragment,
} from '../composables/useFilterGrammar'

describe('literal object-property names in filters (DEV-98)', () => {
    it('keeps constructor colon tokens as ordinary unknown text', () => {
        const parsed = parseFilterQuery('constructor:literal')
        expect(parsed.text).toBe('constructor:literal')
        expect(Object.keys(parsed.filters)).toEqual([])
    })

    it('keeps a namespaced constructor field as a custom field', () => {
        const parsed = parseFilterQuery('field:constructor=fixture')
        expect(parsed.filters['field:constructor']).toBe('fixture')
        expect(Object.keys(parsed.filters)).toEqual(['field:constructor'])
    })

    it('preserves a bare prototype-named field as an own filter property', () => {
        const parsed = parseFilterQuery('__proto__=fixture')
        expect(Object.prototype.hasOwnProperty.call(parsed.filters, '__proto__')).toBe(true)
        expect(parsed.filters['__proto__']).toBe('fixture')
    })

    it('uses quoted field names to distinguish actual custom fields from builtin aliases', () => {
        const parsed = parseFilterQuery('field:"state"=ready field:"sprint"=inc-2')
        expect(parsed.filters).toEqual({ 'field:state': 'ready', 'field:sprint': 'inc-2' })
        expect(canonicalCustomFilterKey('field:"state"')).toBe('field:state')
    })

    it('quotes reserved custom names when serializing a record key or suggesting a field', () => {
        expect(serializeFilterToken('field:sprint', '')).toBe('field:"sprint"=')
        expect(suggestForFragment('field:', { customFields: ['sprint'] })[0]!.insert).toBe('field:"sprint"=')
    })
})

const source = {
    statuses: ['Todo', 'In Progress', 'Done'],
    priorities: ['Low', 'Medium', 'High'],
    types: ['Bug', 'Feature'],
    sprints: [{ id: 1, label: 'Sprint 1' }, { id: 2, label: 'Sprint 2' }],
    projects: [{ prefix: 'DEV' }, { prefix: 'LOTA' }],
    customFields: ['sprint', 'iteration'],
}

describe('parseFilterQuery', () => {
    it('splits plain text from structured tokens', () => {
        const parsed = parseFilterQuery('login bug status:todo priority:high')
        expect(parsed.text).toBe('login bug')
        expect(parsed.filters.status).toBe('todo')
        expect(parsed.filters.priority).toBe('high')
    })

    it('resolves aliases and quoted values', () => {
        const parsed = parseFilterQuery('state:"in progress" prio:High tag:ci')
        expect(parsed.filters.status).toBe('in progress')
        expect(parsed.filters.priority).toBe('High')
        expect(parsed.filters.tags).toBe('ci')
        expect(parsed.text).toBe('')
    })

    it('maps the mine flag', () => {
        const parsed = parseFilterQuery('mine:true')
        expect(parsed.filters.mine).toBe('true')
        expect(parsed.text).toBe('')
    })

    it('keeps unknown key:value tokens as free text', () => {
        const parsed = parseFilterQuery('randomkey:val hello')
        expect(parsed.text).toContain('randomkey:val')
        expect(parsed.text).toContain('hello')
        expect(parsed.errors).toBeUndefined()
    })

    it('ignores tokens with empty values', () => {
        const parsed = parseFilterQuery('status: hello')
        expect(parsed.filters.status).toBeUndefined()
        expect(parsed.text).toBe('status: hello')
    })

    it('keeps the value intact when a colon token value contains equals', () => {
        const parsed = parseFilterQuery('status:x=1')
        expect(parsed.filters.status).toBe('x=1')
        expect(parsed.text).toBe('')
    })

    it('keeps unknown colon tokens without equals as free text', () => {
        const parsed = parseFilterQuery('field:iteration')
        expect(parsed.filters).toEqual({})
        expect(parsed.text).toBe('field:iteration')
        expect(parsed.errors).toBeUndefined()
    })

    it('keeps deletion colon tokens as free text', () => {
        const parsed = parseFilterQuery('deletion:deleted')
        expect(parsed.filters).toEqual({})
        expect(parsed.text).toBe('deletion:deleted')
        expect(parsed.errors).toBeUndefined()
    })
})

describe('parseFilterQuery assignments (legacy custom input syntax)', () => {
    it('parses explicit field:name=value tokens as custom fields', () => {
        const parsed = parseFilterQuery('field:iteration=beta')
        expect(parsed.filters).toEqual({ 'field:iteration': 'beta' })
        expect(parsed.text).toBe('')
        expect(parsed.errors).toBeUndefined()
    })

    it('canonicalizes reserved field names onto builtin keys', () => {
        const parsed = parseFilterQuery('field:STATE=Todo field:task_type=Bug field:priority=Medium field:sprint=2')
        expect(parsed.filters).toEqual({ status: 'Todo', type: 'Bug', priority: 'Medium', sprints: '2' })
    })

    it('maps field: aliases for q, order, sort_by and deletion', () => {
        const parsed = parseFilterQuery('field:q=hello field:order=asc field:sort_by=priority field:deletion=deleted')
        expect(parsed.filters).toEqual({ q: 'hello', order: 'asc', sort_by: 'priority', deletion: 'deleted' })
    })

    it('lowercases custom field names but keeps values verbatim', () => {
        const parsed = parseFilterQuery('field:Iteration="Beta 2"')
        expect(parsed.filters).toEqual({ 'field:iteration': 'Beta 2' })
        expect(parsed.text).toBe('')
    })

    it('treats an unquoted value as ending at whitespace', () => {
        const parsed = parseFilterQuery('field:Iteration=Beta 2')
        expect(parsed.filters).toEqual({ 'field:iteration': 'Beta' })
        expect(parsed.text).toBe('2')
    })

    it('supports quoted names and values with spaces', () => {
        const parsed = parseFilterQuery('field:"release name"=v1 owner="ops team"')
        expect(parsed.filters).toEqual({ 'field:release name': 'v1', owner: 'ops team' })
    })

    it('preserves escaped quotes and backslashes in quoted values', () => {
        const parsed = parseFilterQuery('field:x="a \\"b\\" \\\\ c"')
        expect(parsed.filters).toEqual({ 'field:x': 'a "b" \\ c' })
    })

    it('parses bare key=value pairs without alias mapping', () => {
        const parsed = parseFilterQuery('owner=ops')
        expect(parsed.filters).toEqual({ owner: 'ops' })
    })

    it('bare owner=ops stays a custom filter while owner:ops means assignee', () => {
        const bare = parseFilterQuery('owner=ops')
        const colon = parseFilterQuery('owner:ops')
        expect(bare.filters).toEqual({ owner: 'ops' })
        expect(colon.filters).toEqual({ assignee: 'ops' })
    })

    it('supports canonical builtin names through bare equality', () => {
        const parsed = parseFilterQuery('status=Todo type=Bug mine=true')
        expect(parsed.filters).toEqual({ status: 'Todo', type: 'Bug', mine: 'true' })
    })

    it('lowercases bare keys and keeps a literal field key as custom', () => {
        const parsed = parseFilterQuery('Owner=Ops')
        expect(parsed.filters).toEqual({ owner: 'Ops' })
    })

    it('mixes assignments with search words and colon tokens', () => {
        const parsed = parseFilterQuery('login field:iteration=beta owner=ops status:todo')
        expect(parsed.text).toBe('login')
        expect(parsed.filters).toEqual({ 'field:iteration': 'beta', owner: 'ops', status: 'todo' })
    })
})

describe('parseFilterQuery comma-separated expressions', () => {
    it('splits comma-delimited assignments with and without spaces', () => {
        const spaced = parseFilterQuery('field:iteration=beta, owner=ops')
        const tight = parseFilterQuery('field:iteration=beta,owner=ops')
        expect(spaced.filters).toEqual({ 'field:iteration': 'beta', owner: 'ops' })
        expect(tight.filters).toEqual({ 'field:iteration': 'beta', owner: 'ops' })
    })

    it('splits comma-delimited colon tokens', () => {
        const parsed = parseFilterQuery('status:todo, priority:high')
        expect(parsed.filters).toEqual({ status: 'todo', priority: 'high' })
    })

    it('does not split comma-valued data when a segment is not an expression', () => {
        const parsed = parseFilterQuery('tags:ci,ops')
        expect(parsed.filters).toEqual({ tags: 'ci,ops' })
    })

    it('does not split quoted comma values', () => {
        const parsed = parseFilterQuery('key="a,b"')
        expect(parsed.filters).toEqual({ key: 'a,b' })
    })

    it('keeps plain comma-separated words as free text', () => {
        const parsed = parseFilterQuery('hello, world')
        expect(parsed.text).toBe('hello, world')
        expect(parsed.filters).toEqual({})
    })
})

describe('parseFilterQuery free-text preservation', () => {
    it('keeps URLs untouched', () => {
        const parsed = parseFilterQuery('https://x.com/a?status:todo=1 bug')
        expect(parsed.filters).toEqual({})
        expect(parsed.text).toBe('https://x.com/a?status:todo=1 bug')
        expect(parsed.errors).toBeUndefined()
    })

    it('keeps fully quoted literals as free text', () => {
        const parsed = parseFilterQuery('"a=b"')
        expect(parsed.filters).toEqual({})
        expect(parsed.text).toBe('a=b')
        expect(parsed.errors).toBeUndefined()
    })

    it('keeps fully quoted colon-looking literals as free text', () => {
        const parsed = parseFilterQuery('"status:todo"')
        expect(parsed.filters).toEqual({})
        expect(parsed.text).toBe('status:todo')
    })

    it('preserves apostrophes in free text', () => {
        const parsed = parseFilterQuery("don't panic")
        expect(parsed.text).toBe("don't panic")
        expect(parsed.errors).toBeUndefined()
    })

    it('keeps quoted empty values as free text', () => {
        const parsed = parseFilterQuery('status:""')
        expect(parsed.filters).toEqual({})
        expect(parsed.text).toBe('status:""')
    })
})

describe('parseFilterQuery error reporting', () => {
    it('reports an add-value error for incomplete field assignments', () => {
        const parsed = parseFilterQuery('field:iteration=')
        expect(parsed.errors).toEqual(['Add a value for "field:iteration"'])
        expect(parsed.filters).toEqual({})
        expect(parsed.text).toBe('')
    })

    it('reports an add-value error for incomplete bare assignments', () => {
        const parsed = parseFilterQuery('key=')
        expect(parsed.errors).toEqual(['Add a value for "key"'])
    })

    it('reports a missing key before =', () => {
        const parsed = parseFilterQuery('=value')
        expect(parsed.errors).toEqual(['Missing key before "="'])
    })

    it('reports invalid field names', () => {
        const parsed = parseFilterQuery('field:=value')
        expect(parsed.errors).toEqual(['Invalid filter name "field:"'])
    })

    it('reports unmatched quotes in explicit expressions', () => {
        const parsed = parseFilterQuery('status:"unclosed')
        expect(parsed.errors?.length).toBe(1)
        expect(parsed.errors![0]).toContain('Unmatched quote')
        expect(parsed.filters).toEqual({})
    })

    it('reports unmatched quotes in assignments', () => {
        const parsed = parseFilterQuery('owner="val')
        expect(parsed.errors?.[0]).toContain('Unmatched quote')
        expect(parsed.filters).toEqual({})
    })

    it('still returns the valid subset alongside errors', () => {
        const parsed = parseFilterQuery('status:todo field:x=')
        expect(parsed.errors?.length).toBe(1)
        expect(parsed.filters.status).toBe('todo')
    })

    it('omits errors entirely for valid input', () => {
        const parsed = parseFilterQuery('login status:todo field:iteration=beta, owner=ops')
        expect(parsed.errors).toBeUndefined()
    })
})

describe('parseFilterQuery replaceKeys metadata', () => {
    it('marks canonical keys from field: alias assignments', () => {
        const parsed = parseFilterQuery('field:STATE=Todo field:priority=Medium')
        expect(parsed.filters).toEqual({ status: 'Todo', priority: 'Medium' })
        expect(parsed.replaceKeys).toEqual(['status', 'priority'])
    })

    it('marks canonical keys from bare assignments', () => {
        const parsed = parseFilterQuery('status=Done sprints=1,2')
        expect(parsed.replaceKeys).toEqual(['status', 'sprints'])
    })

    it('marks custom assignment keys too', () => {
        const parsed = parseFilterQuery('field:iteration=beta owner=ops')
        expect(parsed.replaceKeys).toEqual(['field:iteration', 'owner'])
    })

    it('marks q, order, sort_by and deletion from assignments', () => {
        const parsed = parseFilterQuery('field:q=hello field:order=asc field:sort_by=priority field:deletion=deleted')
        expect(parsed.replaceKeys).toEqual(['q', 'order', 'sort_by', 'deletion'])
    })

    it('omits replaceKeys for pure colon queries', () => {
        const parsed = parseFilterQuery('status:Todo priority:high login')
        expect(parsed.filters.status).toBe('Todo')
        expect(parsed.replaceKeys).toBeUndefined()
    })

    it('omits replaceKeys for pure free text', () => {
        const parsed = parseFilterQuery('hello world')
        expect(parsed.replaceKeys).toBeUndefined()
    })

    it('keeps replaceKeys absent when only errors accompany colon tokens', () => {
        const parsed = parseFilterQuery('status:Todo field:x=')
        expect(parsed.errors?.length).toBe(1)
        expect(parsed.replaceKeys).toBeUndefined()
    })

    it('lets the last token choose the value and the replace policy', () => {
        const colonThenAssign = parseFilterQuery('status:Todo status=Done')
        const assignThenColon = parseFilterQuery('status=Todo status:Done')
        expect(colonThenAssign.filters.status).toBe('Done')
        expect(colonThenAssign.replaceKeys).toEqual(['status'])
        expect(assignThenColon.filters.status).toBe('Done')
        expect(assignThenColon.replaceKeys).toBeUndefined()
    })

    it('keeps replace policy from the last of repeated assignments', () => {
        const parsed = parseFilterQuery('status=A status=B')
        expect(parsed.filters.status).toBe('B')
        expect(parsed.replaceKeys).toEqual(['status'])
    })

    it('reports replaceKeys alongside errors for valid assignments', () => {
        const parsed = parseFilterQuery('owner=ops field:x=')
        expect(parsed.errors?.length).toBe(1)
        expect(parsed.filters.owner).toBe('ops')
        expect(parsed.replaceKeys).toEqual(['owner'])
    })
})

describe('findGrammarKey', () => {
    it('matches canonical names and aliases case-insensitively', () => {
        expect(findGrammarKey('status')?.key).toBe('status')
        expect(findGrammarKey('STATE')?.key).toBe('status')
        expect(findGrammarKey('prio')?.key).toBe('priority')
        expect(findGrammarKey('owner')?.key).toBe('assignee')
        expect(findGrammarKey('nope')).toBeNull()
    })
})

describe('canonicalCustomFilterKey', () => {
    it('canonicalizes reserved field names and lowercases custom ones', () => {
        expect(canonicalCustomFilterKey('field:STATE')).toBe('status')
        expect(canonicalCustomFilterKey('field:task_type')).toBe('type')
        expect(canonicalCustomFilterKey('field:deletion')).toBe('deletion')
        expect(canonicalCustomFilterKey('field:iteration')).toBe('field:iteration')
        expect(canonicalCustomFilterKey('field:"my field"')).toBe('field:my field')
    })

    it('lowercases bare keys without alias mapping', () => {
        expect(canonicalCustomFilterKey('Owner')).toBe('owner')
        expect(canonicalCustomFilterKey('state')).toBe('state')
    })

    it('returns empty for blank or nameless keys', () => {
        expect(canonicalCustomFilterKey('')).toBe('')
        expect(canonicalCustomFilterKey('field:')).toBe('')
    })
})

describe('serializeFilterToken', () => {
    it('formats plain custom tokens', () => {
        expect(serializeFilterToken('field:iteration', 'beta')).toBe('field:iteration=beta')
        expect(serializeFilterToken('owner', 'ops')).toBe('owner=ops')
        expect(serializeFilterToken('status', 'Todo')).toBe('status=Todo')
    })

    it('produces editable field prefixes for empty values', () => {
        expect(serializeFilterToken('field:iteration', '')).toBe('field:iteration=')
        expect(serializeFilterToken('owner', '')).toBe('owner=')
    })

    it('quotes names and values that need it', () => {
        expect(serializeFilterToken('field:my field', 'a b')).toBe('field:"my field"="a b"')
        expect(serializeFilterToken('weird key', 'v')).toBe('"weird key"=v')
    })

    it('escapes quotes and backslashes', () => {
        expect(serializeFilterToken('field:x', 'a "b" \\ c')).toBe('field:x="a \\"b\\" \\\\ c"')
    })

    it('round-trips through parseFilterQuery', () => {
        const cases: Array<[string, string]> = [
            ['field:iteration', 'beta'],
            ['field:my field', 'a b'],
            ['owner', 'ops team'],
            ['field:x', 'a,b=c "q" \\ z'],
            ['weird:key', 'v'],
            ['status', 'Todo'],
        ]
        for (const [key, value] of cases) {
            const parsed = parseFilterQuery(serializeFilterToken(key, value))
            expect(parsed.errors).toBeUndefined()
            expect(parsed.text).toBe('')
            expect(parsed.filters[key.toLowerCase()]).toBe(value)
        }
    })
})

describe('filterFragment', () => {
    it('splits the final token after top-level whitespace', () => {
        expect(filterFragment('status:todo prio:hi')).toEqual({ prefix: 'status:todo ', fragment: 'prio:hi' })
    })

    it('does not split inside quotes', () => {
        expect(filterFragment('field:"my field"=va')).toEqual({ prefix: '', fragment: 'field:"my field"=va' })
        expect(filterFragment('abc field:"my field"=va')).toEqual({ prefix: 'abc ', fragment: 'field:"my field"=va' })
        expect(filterFragment('state:"in pro')).toEqual({ prefix: '', fragment: 'state:"in pro' })
    })

    it('returns an empty fragment for trailing whitespace or empty input', () => {
        expect(filterFragment('status:todo ')).toEqual({ prefix: 'status:todo ', fragment: '' })
        expect(filterFragment('')).toEqual({ prefix: '', fragment: '' })
    })
})

describe('suggestForFragment', () => {
    it('suggests keys before a colon is typed', () => {
        const suggestions = suggestForFragment('sta', source)
        expect(suggestions.length).toBeGreaterThan(0)
        expect(suggestions[0]!.insert.startsWith('status:')).toBe(true)
    })

    it('suggests matching option values after a colon', () => {
        const suggestions = suggestForFragment('status:in', source)
        expect(suggestions.map((s) => s.label)).toContain('In Progress')
    })

    it('suggests sprint values by id', () => {
        const suggestions = suggestForFragment('sprint:', source)
        expect(suggestions.map((s) => s.insert)).toContain('sprints:2')
    })

    it('suggests assignee modes', () => {
        const suggestions = suggestForFragment('assignee:', source)
        expect(suggestions.map((s) => s.label)).toContain('Me')
        expect(suggestions.map((s) => s.label)).toContain('No assignee')
    })

    it('returns nothing for unknown keys', () => {
        expect(suggestForFragment('bogus:x', source)).toEqual([])
    })

    it('suggests value completions for bare partial words', () => {
        const suggestions = suggestForFragment('todo', source)
        expect(suggestions.map((s) => s.insert)).toContain('status:Todo')
    })

    it('suggests sprint label completions for bare partial words', () => {
        const suggestions = suggestForFragment('2', source)
        expect(suggestions.map((s) => s.insert)).toContain('sprints:2')
    })

    it('returns no bare-word suggestions when nothing matches', () => {
        expect(suggestForFragment('zzz', source)).toEqual([])
    })

    it('suggests custom field names after field:', () => {
        const suggestions = suggestForFragment('field:', source)
        expect(suggestions.map((s) => s.insert)).toContain('field:iteration=')
        expect(suggestions.map((s) => s.insert)).toContain('field:"sprint"=')
        expect(suggestions.map((s) => s.label)).toContain('iteration')
    })

    it('filters custom field names by partial input', () => {
        expect(suggestForFragment('field:it', source).map((s) => s.insert)).toEqual(['field:iteration='])
    })

    it('matches custom field names behind a quote', () => {
        const suggestions = suggestForFragment('field:"my', { customFields: ['my field'] })
        expect(suggestions.map((s) => s.insert)).toEqual(['field:"my field"='])
    })

    it('offers the field: prefix while typing', () => {
        expect(suggestForFragment('fi', source).map((s) => s.insert)).toContain('field:')
        expect(suggestForFragment('', source).map((s) => s.insert)).toContain('field:')
        expect(suggestForFragment('', source).map((s) => s.insert)).toContain('status:')
    })

    it('keeps key suggestions unchanged when field does not match', () => {
        const suggestions = suggestForFragment('sta', source)
        expect(suggestions[0]!.insert).toBe('status:')
        expect(suggestions.some((s) => s.insert === 'field:')).toBe(false)
    })

    it('offers no suggestions for completed custom assignments', () => {
        expect(suggestForFragment('field:iteration=', source)).toEqual([])
    })
})

describe('chipsForFilterValue', () => {
    it('creates one chip per CSV value with sprint labels', () => {
        const chips = chipsForFilterValue({ sprints: '1,2', status: 'Todo' }, source)
        expect(chips).toHaveLength(3)
        const sprintChips = chips.filter((c) => c.key === 'sprints')
        expect(sprintChips.map((c) => c.display)).toEqual(['Sprint 1', 'Sprint 2'])
    })

    it('labels special assignee values', () => {
        expect(chipsForFilterValue({ assignee: '@me' }, source)[0]!.label).toBe('Mine')
        expect(chipsForFilterValue({ assignee: '__none__' }, source)[0]!.label).toBe('No assignee')
    })

    it('skips order, text query and tags; project becomes a chip', () => {
        const chips = chipsForFilterValue({ order: 'asc', project: 'DEV', q: 'hello', tags: 'ci' }, source)
        expect(chips).toEqual([{ key: 'project', label: 'Project', value: 'DEV', display: 'DEV' }])
    })

    it('splits needs filters into one chip per entry', () => {
        const chips = chipsForFilterValue({ needs: 'effort,review' }, source)
        expect(chips.map((c) => c.label)).toEqual(['Needs effort', 'Needs review'])
    })

    it('labels custom field chips without the field: prefix', () => {
        const chips = chipsForFilterValue({ 'field:iteration': 'beta', owner: 'ops' }, source)
        expect(chips).toEqual([
            { key: 'field:iteration', label: 'iteration', value: 'beta', display: 'beta' },
            { key: 'owner', label: 'owner', value: 'ops', display: 'ops' },
        ])
    })

    it('keeps comma-containing custom values as one chip', () => {
        const chips = chipsForFilterValue({ 'field:greeting': 'hello, world' }, source)
        expect(chips).toHaveLength(1)
        expect(chips[0]!.value).toBe('hello, world')
        expect(chips[0]!.key).toBe('field:greeting')
    })
})
