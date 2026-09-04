import { describe, expect, it } from 'vitest'
import {
    chipsForFilterValue,
    findGrammarKey,
    parseFilterQuery,
    suggestForFragment,
} from '../composables/useFilterGrammar'

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
    })

    it('ignores tokens with empty values', () => {
        const parsed = parseFilterQuery('status: hello')
        expect(parsed.filters.status).toBeUndefined()
        expect(parsed.text).toBe('status: hello')
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
})
