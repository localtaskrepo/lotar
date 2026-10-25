import { describe, expect, it } from 'vitest'
import {
    buildRemoteConfig,
    formatMappingErrors,
    parseMappingInput,
    remoteProviderLabel,
    validateMapping,
    type RemoteFormState,
} from '../utils/syncRemotes'

const form = (overrides: Partial<RemoteFormState> = {}): RemoteFormState => ({
    name: 'jira-main',
    provider: 'jira',
    project: ' ATLS ',
    repo: '',
    filter: '',
    auth_profile: '',
    mapping: '',
    ...overrides,
})

describe('syncRemotes', () => {
    it('parses YAML mappings and rejects non-mappings', () => {
        expect(parseMappingInput('  ')).toEqual({})
        expect(parseMappingInput('status: state')).toEqual({ status: 'state' })
        expect(parseMappingInput('- a\n- b')).toBeNull()
        expect(parseMappingInput('a: [')).toBeNull()
    })

    it('reports each kind of mapping problem', () => {
        expect(validateMapping({ status: 'state' })).toEqual([])
        expect(
            validateMapping({
                status: { field: 'state', colour: 'x' } as never,
                tags: { add: [1] } as never,
                priority: { values: { High: 2 } } as never,
                due: { when_empty: 'drop' } as never,
            }),
        ).toEqual([
            "Mapping for status has unsupported key 'colour'.",
            'Mapping for tags add must be a list of strings.',
            'Mapping for priority values must map strings to strings.',
            "Mapping for due when_empty must be 'skip' or 'clear'.",
        ])
        expect(formatMappingErrors(['a', 'b', 'c', 'd', 'e'])).toBe('a b c (2 more)')
    })

    it('builds a trimmed remote config or explains what is missing', () => {
        expect(buildRemoteConfig(form()).config).toEqual({
            provider: 'jira',
            project: 'ATLS',
            repo: null,
            filter: null,
            auth_profile: null,
            mapping: {},
        })
        expect(buildRemoteConfig(form({ provider: 'github' })).errors).toEqual(['GitHub repository is required.'])
        expect(buildRemoteConfig(form({ mapping: 'a: [' })).errors).toEqual(['Mapping must be valid YAML.'])
        expect(remoteProviderLabel({ provider: 'github', repo: ' ' } as never)).toBe('GitHub')
    })
})
