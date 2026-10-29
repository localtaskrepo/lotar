import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import AutomationRulesEditor from '../components/AutomationRulesEditor.vue'
import { parseAutomationYamlDocument } from '../utils/automationRules'

const api = vi.hoisted(() => ({
    runAutomation: vi.fn(),
    simulateAutomation: vi.fn(),
    suggestTasks: vi.fn(async () => []),
    getTask: vi.fn(),
}))
vi.mock('../api/client', () => ({ api }))

vi.mock('../components/UiModal.vue', () => ({
    default: {
        name: 'UiModal',
        props: ['open', 'ariaLabel', 'ariaLabelledby', 'ariaDescribedby', 'dismissible', 'initialFocus', 'size'],
        emits: ['close'],
        template: '<div v-if="open" class="ui-modal-stub"><slot /></div>',
    },
}))

const RULES = `automation:
  max_iterations: 5
  rules:
    - name: First
      on:
        created:
          set:
            status: Todo
    - name: Agent lifecycle
      when:
        assignee: "@agent"
      on:
        job_started:
          set:
            status: InProgress
        job_completed:
          set:
            status: NeedsReview
          agent:
            model: haiku
            prompt: "Test \${{ticket.id}}"
          archive: true
    - name: Archive finished work
      when:
        all:
          - status: Done
          - status_changed: { older_than: 30d }
      on:
        manual:
          archive: true
`

function lastYaml(wrapper: ReturnType<typeof mount>) {
    const emitted = wrapper.emitted('update:modelValue')
    expect(emitted).toBeTruthy()
    return emitted![emitted!.length - 1]![0] as string
}

function byTestId(wrapper: ReturnType<typeof mount>, id: string) {
    return wrapper.find(`[data-testid="${id}"]`)
}

describe('AutomationRulesEditor rule list (DEV-241)', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('describes each trigger on its own line, with only its own steps', async () => {
        const wrapper = mount(AutomationRulesEditor, { props: { modelValue: RULES } })
        await flushPromises()
        const lines = byTestId(wrapper, 'automation-rule-1')
            .findAll('[data-testid="automation-rule-summary"] li')
            .map((line) => line.text().replace(/\s+/g, ' '))
        expect(lines).toEqual([
            'When an agent job starts, if assignee is @agent: then set status to InProgress.',
            'When an agent job finishes, if assignee is @agent: then set status to NeedsReview, run the agent with model haiku, a custom prompt, archive the task.',
        ])
        const manual = byTestId(wrapper, 'automation-rule-2').text().replace(/\s+/g, ' ')
        expect(manual).toContain('Manual / cron')
        expect(manual).toContain('in its current status for more than 30d')
        wrapper.unmount()
    })

    it.each([
        ['Move down', 0, ['Agent lifecycle', 'First', 'Archive finished work']],
        ['Move up', 1, ['Agent lifecycle', 'First', 'Archive finished work']],
        ['Duplicate', 0, ['First', 'First (copy)', 'Agent lifecycle', 'Archive finished work']],
        ['Delete', 0, ['Agent lifecycle', 'Archive finished work']],
    ] as const)('%s on rule %i keeps max_iterations', async (label, index, expected) => {
        const wrapper = mount(AutomationRulesEditor, { props: { modelValue: RULES } })
        await flushPromises()
        await byTestId(wrapper, `automation-rule-${index}`).find(`button[aria-label="${label}"]`).trigger('click')
        const parsed = parseAutomationYamlDocument(lastYaml(wrapper))
        expect(parsed.maxIterations).toBe('5')
        expect(parsed.rules.map((rule) => rule.name)).toEqual(expected)
        wrapper.unmount()
    })

    it('switches a rule off and on through enabled: false', async () => {
        const wrapper = mount(AutomationRulesEditor, { props: { modelValue: RULES } })
        await flushPromises()
        await byTestId(wrapper, 'automation-rule-toggle-0').setValue(false)
        let yaml = lastYaml(wrapper)
        expect(parseAutomationYamlDocument(yaml).rules[0]).toMatchObject({ name: 'First', enabled: false })
        await wrapper.setProps({ modelValue: yaml })
        expect(byTestId(wrapper, 'automation-rule-0').text()).toContain('Off')
        await byTestId(wrapper, 'automation-rule-toggle-0').setValue(true)
        yaml = lastYaml(wrapper)
        expect(parseAutomationYamlDocument(yaml).rules[0]).not.toHaveProperty('enabled')
        wrapper.unmount()
    })

    it('runs a manual rule after previewing it, but not with unsaved changes', async () => {
        api.runAutomation.mockImplementation(async (request: { dry_run?: boolean }) => ({
            dry_run: Boolean(request.dry_run),
            rules: ['Archive finished work'],
            checked: 3,
            entries: [
                {
                    rule: 'Archive finished work',
                    task: 'AM-1',
                    actions: [{ action: 'archive', description: 'Archive the task' }],
                },
            ],
        }))
        const wrapper = mount(AutomationRulesEditor, {
            props: { modelValue: RULES, project: 'AM', dirty: true },
            attachTo: document.body,
        })
        await flushPromises()
        expect(byTestId(wrapper, 'automation-run-0').exists()).toBe(false)
        expect(byTestId(wrapper, 'automation-run-2').attributes('disabled')).toBeDefined()

        await wrapper.setProps({ dirty: false })
        await byTestId(wrapper, 'automation-run-2').trigger('click')
        await flushPromises()
        expect(api.runAutomation).toHaveBeenLastCalledWith({
            rules: ['Archive finished work'],
            query: 'project=AM',
            dry_run: true,
        })
        expect(byTestId(wrapper, 'automation-run-preview').text()).toContain('AM-1')

        await byTestId(wrapper, 'automation-run-confirm').trigger('click')
        await flushPromises()
        expect(api.runAutomation).toHaveBeenLastCalledWith(expect.objectContaining({ dry_run: false }))
        expect(wrapper.emitted('ran')).toHaveLength(1)
        wrapper.unmount()
    })
})

describe('AutomationRuleDialog (DEV-241)', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })
    afterEach(() => {
        document.body.innerHTML = ''
    })

    async function openNew(props: Record<string, unknown> = {}) {
        const wrapper = mount(AutomationRulesEditor, {
            props: {
                modelValue: '',
                availableStatuses: ['Todo', 'InProgress', 'NeedsReview', 'Done'],
                availablePriorities: ['Low', 'High'],
                availableMembers: ['priya', '@claude'],
                ...props,
            },
            attachTo: document.body,
        })
        await byTestId(wrapper, 'automation-new-rule').trigger('click')
        await flushPromises()
        return wrapper
    }

    it('builds a rule from triggers, conditions and actions on one screen', async () => {
        const wrapper = await openNew()
        const save = () => byTestId(wrapper, 'automation-rule-save')
        expect(save().attributes('disabled')).toBeDefined()

        await byTestId(wrapper, 'automation-rule-name').setValue('Ship after tests')
        await byTestId(wrapper, 'automation-trigger-complete').trigger('click')
        await byTestId(wrapper, 'automation-add-condition').trigger('click')
        await byTestId(wrapper, 'automation-condition-value').setValue('InProgress')
        await byTestId(wrapper, 'automation-add-action-complete').setValue('set:status')
        await byTestId(wrapper, 'automation-set-status').setValue('Done')
        await byTestId(wrapper, 'automation-add-action-complete').setValue('comment')
        await byTestId(wrapper, 'automation-comment').setValue('Shipped ${{ticket.id}}')

        expect(byTestId(wrapper, 'automation-rule-sentence').text()).toBe(
            'When an agent job finishes, if status is InProgress: set status to Done, comment “Shipped ${{ticket.id}}”.',
        )
        expect(save().attributes('disabled')).toBeUndefined()
        await save().trigger('click')
        const rule = parseAutomationYamlDocument(lastYaml(wrapper)).rules[0]
        expect(rule).toEqual({
            name: 'Ship after tests',
            when: { status: 'InProgress' },
            on: { job_completed: { set: { status: 'Done' }, comment: 'Shipped ${{ticket.id}}' } },
        })
        wrapper.unmount()
    })

    it('starts from recipes that use the project values', async () => {
        const wrapper = await openNew()
        await byTestId(wrapper, 'automation-recipe-archive-done').trigger('click')
        await byTestId(wrapper, 'automation-rule-save').trigger('click')
        const rule = parseAutomationYamlDocument(lastYaml(wrapper)).rules[0]
        expect(rule).toEqual({
            name: 'Archive finished work',
            when: { all: [{ status: 'Done' }, { status_changed: { older_than: '30d' } }] },
            on: { manual: { archive: true } },
        })
        wrapper.unmount()
    })

    it('keeps agent settings and archive when an existing rule is saved unchanged', async () => {
        const wrapper = mount(AutomationRulesEditor, { props: { modelValue: RULES }, attachTo: document.body })
        await byTestId(wrapper, 'automation-edit-1').trigger('click')
        await flushPromises()
        await byTestId(wrapper, 'automation-rule-save').trigger('click')
        const before = parseAutomationYamlDocument(RULES).rules[1]
        const after = parseAutomationYamlDocument(lastYaml(wrapper)).rules[1]
        expect(after).toEqual(before)
        wrapper.unmount()
    })

    it('offers placeholders for the chosen triggers and previews them on a task', async () => {
        api.getTask.mockResolvedValue({ id: 'DEV-7', title: 'Fix login', tags: [], status: 'Todo' })
        api.simulateAutomation.mockResolvedValue({
            matched: true,
            rule_name: null,
            actions: [{ action: 'comment', description: 'Add comment: Done with Fix login' }],
            task_before: {},
            task_after: { status: 'Done', tags: [] },
            warnings: [],
        })
        const wrapper = await openNew()
        await byTestId(wrapper, 'automation-trigger-updated').trigger('click')
        await byTestId(wrapper, 'automation-add-action-updated').setValue('comment')

        await byTestId(wrapper, 'automation-comment-insert').trigger('click')
        const groups = wrapper.findAll('.template-field__group').map((group) => group.text())
        expect(groups).toEqual(['Task', 'Before the change'])
        const option = wrapper.findAll('.template-field__option').find((item) => item.text().includes('ticket.title'))
        await option!.trigger('mousedown')
        expect((byTestId(wrapper, 'automation-comment').element as HTMLTextAreaElement).value).toBe('${{ticket.title}}')

        await byTestId(wrapper, 'automation-try-task').setValue('DEV-7')
        await byTestId(wrapper, 'automation-try-task').trigger('change')
        await flushPromises()
        expect(byTestId(wrapper, 'automation-comment-preview').text()).toContain('On DEV-7: Fix login')

        await byTestId(wrapper, 'automation-try').trigger('click')
        await flushPromises()
        expect(api.simulateAutomation).toHaveBeenCalledWith(
            expect.objectContaining({ ticket_id: 'DEV-7', event: 'updated', rule: expect.any(Object) }),
        )
        expect(byTestId(wrapper, 'automation-try-result').text()).toContain('Done with Fix login')
        wrapper.unmount()
    })

    it('suggests placeholders while typing ${{', async () => {
        const wrapper = await openNew()
        await byTestId(wrapper, 'automation-trigger-commented').trigger('click')
        await byTestId(wrapper, 'automation-add-action-commented').setValue('comment')
        const field = byTestId(wrapper, 'automation-comment')
        const element = field.element as HTMLTextAreaElement
        element.value = 'Reply: ${{comm'
        element.setSelectionRange(element.value.length, element.value.length)
        await field.trigger('input')
        await flushPromises()
        const options = wrapper.findAll('.template-field__option').map((item) => item.text())
        expect(options).toEqual(['The new commentcomment.text', 'Latest commentticket.comments[-1].text'])
        await field.trigger('keydown', { key: 'Enter' })
        await flushPromises()
        expect(element.value).toBe('Reply: ${{comment.text}}')
        wrapper.unmount()
    })

    it('closes without emitting anything on Cancel', async () => {
        const wrapper = await openNew()
        const cancel = wrapper.findAll('button').find((button) => button.text() === 'Cancel')
        await cancel!.trigger('click')
        expect(wrapper.emitted('update:modelValue')).toBeUndefined()
        expect(byTestId(wrapper, 'automation-rule-dialog').exists()).toBe(false)
        wrapper.unmount()
    })
})
