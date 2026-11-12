import { describe, expect, it } from 'vitest'
import {
    buildAutomationRuleFromDraft,
    createEmptyRuleDraft,
    getEditableRuleState,
    parseAutomationYamlDocument,
    summarizeAutomationRule,
} from '../utils/automationRules'
import { describeActionSteps, describeConditions } from '../utils/automationText'

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
            any: [{ status: 'InProgress' }, { priority: 'High' }],
        })
        expect(rule.on).toMatchObject({ job_completed: { set: { status: 'Done' } } })
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

    it('flags current-value conditions as yaml-only', () => {
        const rule = {
            name: 'Change starts',
            when: { all: [{ current: { type: 'Change' } }, { status: 'InProgress' }] },
            on: { updated: { comment: 'started' } },
        }
        const state = getEditableRuleState(rule)

        expect(state.editable).toBe(false)
        expect(state.unsupportedReasons).toEqual(['Current-value conditions are currently YAML-only.'])
        expect(
            summarizeAutomationRule({ ...rule, when: { current: { type: 'Change' }, status: 'InProgress' } })
                .conditions,
        ).toEqual(['NOW (...)', 'status = InProgress'])
    })

    it('flags comment author conditions as yaml-only and describes them', () => {
        const rule = {
            name: 'Owner answered',
            when: { comment: { author: 'mallox' } },
            on: { commented: { comment: 'thanks ${{comment.author}}' } },
        }
        const state = getEditableRuleState(rule)

        expect(state.editable).toBe(false)
        expect(state.unsupportedReasons).toEqual(['Comment author conditions are currently YAML-only.'])
        expect(summarizeAutomationRule(rule).conditions).toEqual(['comment author = mallox'])
        expect(describeConditions(rule.when).items).toEqual(['comment author is mallox'])
    })

    it('describes tell_agent actions and keeps them yaml-only', () => {
        const rule = {
            name: 'Comments reach the agent',
            on: { commented: { tell_agent: { message: '${{comment.author}}: ${{comment.text}}', agent: '@fix' } } },
        }
        expect(getEditableRuleState(rule).editable).toBe(false)
        expect(describeActionSteps(rule.on.commented)).toEqual([
            'tell the ticket’s agent (or start @fix) “${{comment.author}}: ${{comment.text}}”',
        ])
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
