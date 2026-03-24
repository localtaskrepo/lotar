import { describe, expect, it } from 'vitest'
import {
    buildAutomationRuleFromDraft,
    createEmptyRuleDraft,
    getEditableRuleState,
    parseAutomationYamlDocument,
    summarizeAutomationRule,
} from '../utils/automationRules'

describe('automationRules utilities', () => {
    it('builds any-match rules into an any group', () => {
        const draft = createEmptyRuleDraft()
        draft.matchMode = 'any'
        draft.conditions = [
            {
                id: 'cond-1',
                scope: 'field',
                field: 'status',
                customFieldKey: '',
                operator: 'equals',
                value: 'InProgress',
            },
            {
                id: 'cond-2',
                scope: 'field',
                field: 'priority',
                customFieldKey: '',
                operator: 'equals',
                value: 'High',
            },
        ]

        const completeAction = draft.eventActions.find((entry) => entry.event === 'complete')
        expect(completeAction).toBeTruthy()
        completeAction!.enabled = true
        completeAction!.setFields = [
            {
                id: 'set-1',
                scope: 'field',
                field: 'status',
                customFieldKey: '',
                value: 'Done',
            },
        ]

        const rule = buildAutomationRuleFromDraft(draft)
        expect(rule.when).toEqual({
            any: [
                { status: 'InProgress' },
                { priority: 'High' },
            ],
        })
        expect(rule.on.complete.set.status).toBe('Done')
    })

    it('flags NOT conditions as yaml-only', () => {
        const state = getEditableRuleState({
            name: 'Complex',
            when: {
                not: {
                    status: 'Done',
                },
            },
            on: {
                start: {
                    set: {
                        status: 'InProgress',
                    },
                },
            },
        })

        expect(state.editable).toBe(false)
        expect(state.unsupportedReasons.join(' ')).toContain('NOT conditions')
    })

    it('compacts canonical yaml before building summaries', () => {
        const parsed = parseAutomationYamlDocument(`automation:
  rules:
    - name: Auto-tag bugs
      cooldown: null
      when:
        all: null
        any: null
        not: null
        changes: null
        custom_fields: {}
        type: Bug
      on:
        start: null
        created:
          set:
            priority: High
            status: null
          add:
            tags: [bug-detected]
            labels: null
        updated: null
        assigned: null
        commented: null
        sprint_changed: null
        job_started: null
        job_completed: null
        job_failed: null
        job_cancelled: null
`)

        expect(parsed.parseError).toBeNull()
        expect(parsed.rules).toEqual([
            {
                name: 'Auto-tag bugs',
                when: {
                    type: 'Bug',
                },
                on: {
                    created: {
                        set: {
                            priority: 'High',
                        },
                        add: {
                            tags: ['bug-detected'],
                        },
                    },
                },
            },
        ])
    })

    it('summarizes common actions for rule cards', () => {
        const summary = summarizeAutomationRule({
            name: 'Auto merge',
            when: {
                assignee: '@merge',
            },
            on: {
                complete: {
                    set: {
                        status: 'Done',
                        assignee: '@reporter',
                    },
                    remove: {
                        tags: ['ready-for-review'],
                    },
                },
            },
        })

        expect(summary.name).toBe('Auto merge')
        expect(summary.events).toContain('Job completed')
        expect(summary.conditions).toContain('assignee = @merge')
        expect(summary.actions.some((action) => action.includes('set status=Done'))).toBe(true)
    })
})