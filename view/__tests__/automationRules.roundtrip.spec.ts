import { describe, expect, it } from 'vitest'
import {
    type AutomationRecord,
    buildAutomationRuleFromDraft,
    buildAutomationYamlDocument,
    getEditableRuleState,
    parseAutomationYamlDocument,
} from '../utils/automationRules'

function roundTrip(rule: AutomationRecord) {
    const state = getEditableRuleState(rule)
    return { state, rebuilt: buildAutomationRuleFromDraft(state.draft) }
}

describe('automation rule form round trips', () => {
    it('rebuilds a rule with conditions, changes and every action kind', () => {
        const rule: AutomationRecord = {
            name: 'Escalate blocked bugs',
            cooldown: '1h',
            when: {
                all: [
                    { type: 'Bug' },
                    { priority: { in: ['High', 'Critical'] } },
                    { tags: { exists: true } },
                    { custom_fields: { severity: 'sev1' } },
                    // The builder always writes change conditions last.
                    { changes: { status: { from: 'InProgress', to: 'Blocked' } } },
                ],
            },
            on: {
                updated: {
                    set: { assignee: '@reporter', status: 'Review', custom_fields: { team: 'core' } },
                    // Lists with several values stay lists; a single value is
                    // written as a scalar, which the server accepts too.
                    add: { tags: ['escalated', 'needs-owner'] },
                    remove: { labels: 'triage' },
                    comment: 'Escalated automatically',
                    run: {
                        command: 'notify',
                        args: ['--channel', 'bugs'],
                        env: { LEVEL: 'high' },
                        cwd: 'scripts',
                        wait: false,
                        ignore_failure: true,
                    },
                },
            },
        }
        const { state, rebuilt } = roundTrip(rule)
        expect(state.unsupportedReasons).toEqual([])
        expect(rebuilt).toEqual(rule)
    })

    it('rebuilds any-match rules and shell run actions', () => {
        const rule: AutomationRecord = {
            when: { any: [{ status: 'Done' }, { assignee: '@bot' }] },
            on: { created: { run: 'echo created' } },
        }
        const { state, rebuilt } = roundTrip(rule)
        expect(state.editable).toBe(true)
        expect(rebuilt).toEqual(rule)
    })

    it('flags shapes the form cannot represent', () => {
        const state = getEditableRuleState({
            on: {
                created: {
                    run: { command: 'notify', env: { RETRIES: 3 }, shell: true },
                    archive: true,
                },
            },
        })
        expect(state.editable).toBe(false)
        const reasons = state.unsupportedReasons.join(' ')
        expect(reasons).toContain('run.shell')
        expect(reasons).toContain('run.env values must be strings')
        expect(reasons).toContain('archive')
    })
})

describe('automation yaml documents', () => {
    it('round trips rules and max_iterations through yaml', () => {
        const rules: AutomationRecord[] = [
            { name: 'First', on: { created: { set: { status: 'Todo' } } } },
            { name: 'Second', when: { type: 'Bug' }, on: { assigned: { comment: 'Picked up' } } },
        ]
        const yaml = buildAutomationYamlDocument(rules, '7')
        expect(yaml).toContain('max_iterations: 7\n')

        const parsed = parseAutomationYamlDocument(yaml)
        expect(parsed).toEqual({ rules, maxIterations: '7', parseError: null })
    })

    it('keeps a non-numeric max_iterations as text and drops empty documents', () => {
        expect(buildAutomationYamlDocument([], ' ')).toBe('')
        const yaml = buildAutomationYamlDocument([], '07')
        expect(yaml).toContain('max_iterations: "07"')
        expect(parseAutomationYamlDocument(yaml).maxIterations).toBe('07')
    })

    it('accepts bare rule lists and reports invalid yaml', () => {
        expect(parseAutomationYamlDocument('').rules).toEqual([])
        expect(parseAutomationYamlDocument('- name: Bare\n- 42\n').rules).toEqual([{ name: 'Bare' }])
        expect(parseAutomationYamlDocument('automation:\n  - name: Listed\n').rules).toEqual([{ name: 'Listed' }])

        const broken = parseAutomationYamlDocument('automation: [unclosed')
        expect(broken.rules).toEqual([])
        expect(broken.parseError).toBeTruthy()
    })
})
