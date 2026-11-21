import type { AgentProfileInfo } from '../api/types'
import { storageGetJson, storageSetJson } from './storage'

/** What a row stands for; the picker draws a glyph per kind. */
export type AssigneeOptionKind = 'unassigned' | 'self' | 'person' | 'agent' | 'special' | 'custom'

export interface AssigneeOption {
    /** Unique within the list: an option can show in Recent and in its own group. */
    key: string
    value: string
    label: string
    /** Agents: `runner · model · effort · provider`, only the parts set. */
    detail: string
    description: string
    kind: AssigneeOptionKind
}

export interface AssigneeGroup {
    id: string
    label: string
    /** Heading above a run of groups (Agents, above its role groups). */
    section?: string
    options: AssigneeOption[]
}

export interface AssigneeSpecial {
    value: string
    description: string
}

export interface AssigneeSource {
    whoami?: string | null
    people?: string[]
    profiles?: AgentProfileInfo[]
    /** Tokens such as `@assignee`, listed first under Special. */
    specials?: AssigneeSpecial[]
    recent?: string[]
    /** The current value: listed under People when no list knows it. */
    current?: string | null
    /** Offer Unassigned (the empty value) first. */
    emptyLabel?: string
    /** Offer people and You (false: agents only). */
    includePeople?: boolean
    /** Row offered for a typed value no option has, e.g. (typed) => `Assign to “${typed}”`. */
    customLabel?: ((typed: string) => string) | null
}

export const RECENT_ASSIGNEES_KEY = 'lotar.assigneePicker.recent'
const RECENT_LIMIT = 4
const RECENT_STORED = 12

/** Role of a profile: its name up to the first `-` (`fix-light` → `fix`). */
export function agentRole(name: string): string {
    const index = name.indexOf('-')
    return index > 0 ? name.slice(0, index) : name
}

export function agentDetail(profile: AgentProfileInfo): string {
    return [profile.runner, profile.model, profile.effort, profile.provider]
        .map((part) => (part ?? '').trim())
        .filter(Boolean)
        .join(' · ')
}

function agentOption(profile: AgentProfileInfo, group: string): AssigneeOption {
    return {
        key: `${group}:@${profile.name}`,
        value: `@${profile.name}`,
        label: `@${profile.name}`,
        detail: agentDetail(profile),
        description: (profile.description ?? '').trim(),
        kind: 'agent',
    }
}

function plainOption(value: string, kind: AssigneeOptionKind, group: string, description = ''): AssigneeOption {
    return { key: `${group}:${value}`, value, label: value, detail: '', description, kind }
}

/** Every typed word must appear in the row's name, details or description. */
export function matchesAssigneeQuery(option: AssigneeOption, query: string): boolean {
    const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean)
    if (!words.length) return true
    const haystack = [option.label, option.value, option.detail, option.description].join(' ').toLowerCase()
    return words.every((word) => haystack.includes(word))
}

/** Agent groups by role: roles in the order of their first profile, base profile first, variants in config order (the server keeps it). */
function agentGroups(profiles: AgentProfileInfo[]): AssigneeGroup[] {
    const roles = new Map<string, AgentProfileInfo[]>()
    for (const profile of profiles) {
        if (!profile.name) continue
        const role = agentRole(profile.name)
        const list = roles.get(role) ?? []
        list.push(profile)
        roles.set(role, list)
    }
    return [...roles.entries()].map(([role, list]) => {
        const ordered = [...list.filter((p) => p.name === role), ...list.filter((p) => p.name !== role)]
        return {
            id: `agents:${role}`,
            label: role,
            section: 'Agents',
            options: ordered.map((profile) => agentOption(profile, 'agents')),
        }
    })
}

/** The picker's groups for a search; empty groups are left out. */
export function buildAssigneeGroups(source: AssigneeSource, query = ''): AssigneeGroup[] {
    const includePeople = source.includePeople !== false
    const profiles = source.profiles ?? []
    const profileNames = new Set(profiles.map((profile) => `@${profile.name}`))
    const whoami = includePeople ? (source.whoami ?? '').trim() : ''
    const specials = source.specials ?? []
    const specialValues = new Set(specials.map((special) => special.value))

    const people = new Set<string>()
    if (includePeople) {
        const candidates = [...(source.people ?? []), (source.current ?? '').trim()]
        for (const raw of candidates) {
            const value = raw.trim()
            if (!value || value === whoami || profileNames.has(value) || specialValues.has(value)) continue
            people.add(value)
        }
    }

    const groups: AssigneeGroup[] = []
    if (source.emptyLabel !== undefined) {
        groups.push({
            id: 'none',
            label: '',
            options: [{ ...plainOption('', 'unassigned', 'none'), label: source.emptyLabel }],
        })
    }
    if (specials.length) {
        groups.push({
            id: 'special',
            label: 'Special',
            options: specials.map((special) => plainOption(special.value, 'special', 'special', special.description)),
        })
    }
    if (whoami) {
        groups.push({ id: 'you', label: 'You', options: [plainOption(whoami, 'self', 'you')] })
    }
    if (people.size) {
        groups.push({
            id: 'people',
            label: 'People',
            options: [...people]
                .sort((a, b) => a.localeCompare(b))
                .map((value) => plainOption(value, 'person', 'people')),
        })
    }
    groups.push(...agentGroups(profiles))

    const trimmed = query.trim()
    if (!trimmed) {
        const known = new Map(groups.flatMap((group) => group.options).map((option) => [option.value, option]))
        const recent = (source.recent ?? [])
            .map((value) => known.get(value))
            .filter((option): option is AssigneeOption => !!option && option.kind !== 'unassigned')
            .slice(0, RECENT_LIMIT)
            .map((option) => ({ ...option, key: `recent:${option.value}` }))
        if (recent.length) {
            const at = source.emptyLabel !== undefined ? 1 : 0
            groups.splice(at, 0, { id: 'recent', label: 'Recent', options: recent })
        }
        return groups
    }

    const filtered = groups
        .map((group) => ({
            ...group,
            options: group.options.filter((option) => matchesAssigneeQuery(option, trimmed)),
        }))
        .filter((group) => group.options.length)
    const exists = groups.some((group) => group.options.some((option) => option.value === trimmed))
    if (source.customLabel && !exists) {
        filtered.push({
            id: 'custom',
            label: '',
            options: [{ ...plainOption(trimmed, 'custom', 'custom'), label: source.customLabel(trimmed) }],
        })
    }
    return filtered
}

/** The viewer's recent picks, newest first; anything unreadable counts as none. */
export function readRecentAssignees(): string[] {
    const stored = storageGetJson(RECENT_ASSIGNEES_KEY)
    if (!Array.isArray(stored)) return []
    return stored.filter((value): value is string => typeof value === 'string' && value.trim().length > 0)
}

export function rememberRecentAssignee(value: string): string[] {
    const trimmed = value.trim()
    if (!trimmed) return readRecentAssignees()
    const next = [trimmed, ...readRecentAssignees().filter((entry) => entry !== trimmed)].slice(0, RECENT_STORED)
    storageSetJson(RECENT_ASSIGNEES_KEY, next)
    return next
}
