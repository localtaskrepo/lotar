import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentProfileInfo } from '../api/types'
import {
    RECENT_ASSIGNEES_KEY,
    agentDetail,
    agentRole,
    buildAssigneeGroups,
    readRecentAssignees,
    rememberRecentAssignee,
} from '../utils/assigneeOptions'

const profiles: AgentProfileInfo[] = [
    { name: 'fix-light', runner: 'claude', model: 'claude-sonnet-5-5', effort: 'medium', permissions: 'workspace' },
    { name: 'review', runner: 'codex', model: 'gpt-5.5', permissions: 'read-only', description: 'Reviews the commit' },
    { name: 'fix', runner: 'claude', model: 'claude-opus-5-5', effort: 'high', permissions: 'workspace' },
    { name: 'fix-codex', runner: 'codex', provider: 'openai', permissions: 'workspace' },
]

const shape = (groups: ReturnType<typeof buildAssigneeGroups>) =>
    groups.map((group) => [group.id, group.options.map((option) => option.value)])

describe('assignee options', () => {
    beforeEach(() => localStorage.clear())

    it('takes the role from the profile name up to the first dash', () => {
        expect(agentRole('fix-light')).toBe('fix')
        expect(agentRole('fix')).toBe('fix')
        expect(agentRole('-odd')).toBe('-odd')
    })

    it('lists runner, model, effort and provider, only the parts set', () => {
        expect(agentDetail(profiles[0]!)).toBe('claude · claude-sonnet-5-5 · medium')
        expect(agentDetail(profiles[3]!)).toBe('codex · openai')
    })

    it('orders Unassigned, You, People, then agents by role with the base profile first', () => {
        const groups = buildAssigneeGroups({
            emptyLabel: 'Unassigned',
            whoami: 'mallox',
            people: ['sam', 'mallox', '@fix', 'allowed@example.com'],
            profiles,
            current: 'priya',
        })
        expect(shape(groups)).toEqual([
            ['none', ['']],
            ['you', ['mallox']],
            ['people', ['allowed@example.com', 'priya', 'sam']],
            ['agents:fix', ['@fix', '@fix-light', '@fix-codex']],
            ['agents:review', ['@review']],
        ])
        expect(groups[groups.length - 1]?.section).toBe('Agents')
        expect(groups[groups.length - 1]?.options[0]?.description).toBe('Reviews the commit')
    })

    it('offers agents only when people are left out', () => {
        const groups = buildAssigneeGroups({ includePeople: false, whoami: 'mallox', people: ['sam'], profiles })
        expect(groups.map((group) => group.id)).toEqual(['agents:fix', 'agents:review'])
    })

    it('lists specials first and keeps them out of People', () => {
        const groups = buildAssigneeGroups({
            specials: [{ value: '@agent', description: 'any agent profile' }],
            people: ['@agent', 'sam'],
        })
        expect(shape(groups)).toEqual([
            ['special', ['@agent']],
            ['people', ['sam']],
        ])
    })

    it('matches every typed word against name, runner, model and description', () => {
        const source = { people: ['sam'], profiles, customLabel: (typed: string) => `Assign to “${typed}”` }
        expect(shape(buildAssigneeGroups(source, 'light'))).toEqual([
            ['agents:fix', ['@fix-light']],
            ['custom', ['light']],
        ])
        expect(shape(buildAssigneeGroups(source, 'claude high'))).toEqual([
            ['agents:fix', ['@fix']],
            ['custom', ['claude high']],
        ])
        expect(shape(buildAssigneeGroups(source, 'reviews'))[0]).toEqual(['agents:review', ['@review']])
    })

    it('offers a typed value only when no option has it', () => {
        const source = { people: ['sam'], customLabel: (typed: string) => `Assign to “${typed}”` }
        expect(shape(buildAssigneeGroups(source, 'sam'))).toEqual([['people', ['sam']]])
        const typed = buildAssigneeGroups(source, ' priya ')
        const custom = typed[typed.length - 1]
        expect(custom?.options[0]).toMatchObject({ value: 'priya', label: 'Assign to “priya”', kind: 'custom' })
        expect(buildAssigneeGroups({ people: ['sam'], customLabel: null }, 'priya')).toEqual([])
    })

    it('shows up to four known recent picks after Unassigned, only while the search is empty', () => {
        const source = {
            emptyLabel: 'Unassigned',
            people: ['a', 'b', 'c', 'd', 'e'],
            recent: ['e', 'gone', 'd', 'c', 'b', 'a'],
        }
        const groups = buildAssigneeGroups(source)
        expect(shape(groups).slice(0, 2)).toEqual([
            ['none', ['']],
            ['recent', ['e', 'd', 'c', 'b']],
        ])
        expect(new Set(groups.flatMap((group) => group.options.map((option) => option.key))).size).toBe(
            groups.flatMap((group) => group.options).length,
        )
        expect(buildAssigneeGroups(source, 'e').some((group) => group.id === 'recent')).toBe(false)
    })

    it('remembers picks newest first without repeats', () => {
        rememberRecentAssignee('sam')
        rememberRecentAssignee('@fix')
        rememberRecentAssignee('sam')
        rememberRecentAssignee('  ')
        expect(readRecentAssignees()).toEqual(['sam', '@fix'])
    })

    it('treats unreadable or foreign stored picks as none', () => {
        localStorage.setItem(RECENT_ASSIGNEES_KEY, '{not json')
        expect(readRecentAssignees()).toEqual([])
        localStorage.setItem(RECENT_ASSIGNEES_KEY, JSON.stringify({ sam: 1 }))
        expect(readRecentAssignees()).toEqual([])
        localStorage.setItem(RECENT_ASSIGNEES_KEY, JSON.stringify(['sam', 4, '', null]))
        expect(readRecentAssignees()).toEqual(['sam'])
    })

    it('still works when storage throws', () => {
        const spy = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
            throw new Error('denied')
        })
        const setSpy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
            throw new Error('denied')
        })
        expect(readRecentAssignees()).toEqual([])
        expect(rememberRecentAssignee('sam')).toEqual(['sam'])
        spy.mockRestore()
        setSpy.mockRestore()
    })
})
