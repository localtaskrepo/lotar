import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import AutomationRulesEditor from '../components/AutomationRulesEditor.vue'

function findButton(wrapper: ReturnType<typeof mount>, text: string) {
    return wrapper.findAll('button').find((button) => button.text().includes(text))
}

describe('AutomationRulesEditor', () => {
    it('emits generated yaml when a rule is created from the dialog', async () => {
        const wrapper = mount(AutomationRulesEditor, {
            props: {
                modelValue: '',
            },
            attachTo: document.body,
        })

        await findButton(wrapper, 'New rule')?.trigger('click')
        await flushPromises()

        await wrapper.findAll('button').find((button) => button.text().includes('Move a task'))?.trigger('click')
        await flushPromises()

        await wrapper.find('input[placeholder="Human review after testing"]').setValue('Ship after tests')
        await findButton(wrapper, 'Next')?.trigger('click')
        await flushPromises()

        await findButton(wrapper, 'Next')?.trigger('click')
        await flushPromises()

        await wrapper.find('input[placeholder="Done"]').setValue('Done')

        await findButton(wrapper, 'Next')?.trigger('click')
        await flushPromises()
        await findButton(wrapper, 'Create rule')?.trigger('click')
        await flushPromises()

        const emitted = wrapper.emitted('update:modelValue')
        expect(emitted).toBeTruthy()
        const lastPayload = emitted![emitted!.length - 1][0] as string
        expect(lastPayload).toContain('name: Ship after tests')
        expect(lastPayload).toContain('complete:')
        expect(lastPayload).toContain('status: Done')

        wrapper.unmount()
    })

    it('creates a comment rule through the dialog', async () => {
        const wrapper = mount(AutomationRulesEditor, {
            props: { modelValue: '' },
            attachTo: document.body,
        })

        await findButton(wrapper, 'New rule')?.trigger('click')
        await flushPromises()

        await wrapper.findAll('button').find((b) => b.text().includes('Leave a comment'))?.trigger('click')
        await flushPromises()

        await wrapper.find('input[placeholder="Human review after testing"]').setValue('Notify reviewer')
        await findButton(wrapper, 'Next')?.trigger('click')
        await flushPromises()

        // Default trigger for comment is 'complete' — just click Next
        await findButton(wrapper, 'Next')?.trigger('click')
        await flushPromises()

        // Fill in the comment
        const textarea = wrapper.find('.automation-builder__textarea')
        expect(textarea.exists()).toBe(true)
        await textarea.setValue('Ready for review')

        await findButton(wrapper, 'Next')?.trigger('click')
        await flushPromises()
        await findButton(wrapper, 'Create rule')?.trigger('click')
        await flushPromises()

        const emitted = wrapper.emitted('update:modelValue')
        expect(emitted).toBeTruthy()
        const lastPayload = emitted![emitted!.length - 1][0] as string
        expect(lastPayload).toContain('name: Notify reviewer')
        expect(lastPayload).toContain('comment:')
        expect(lastPayload).toContain('Ready for review')

        wrapper.unmount()
    })

    it('renders project-aware suggestions when availableStatuses is passed', async () => {
        const wrapper = mount(AutomationRulesEditor, {
            props: {
                modelValue: '',
                availableStatuses: ['Todo', 'InProgress', 'Review', 'Done'],
            },
            attachTo: document.body,
        })

        // Open dialog and select status recipe
        await findButton(wrapper, 'New rule')?.trigger('click')
        await flushPromises()

        await wrapper.findAll('button').find((b) => b.text().includes('Move a task'))?.trigger('click')
        await flushPromises()
        await findButton(wrapper, 'Next')?.trigger('click')
        await flushPromises()
        await findButton(wrapper, 'Next')?.trigger('click')
        await flushPromises()

        // The datalist should exist with status options
        const datalist = wrapper.find('#automation-status-suggestions')
        expect(datalist.exists()).toBe(true)
        const options = datalist.findAll('option')
        expect(options.length).toBe(4)
        expect(options.map((o) => o.attributes('value'))).toEqual(['Todo', 'InProgress', 'Review', 'Done'])

        // The hint within the dialog should reference configured statuses
        const dialogHints = wrapper.find('.automation-builder__dialog').findAll('.field-hint')
        const statusHint = dialogHints.find((h) => h.text().includes('configured statuses'))
        expect(statusHint).toBeTruthy()

        wrapper.unmount()
    })

    it('renders existing rules and allows editing', async () => {
        const existingYaml = `automation:
  rules:
    - name: Close after complete
      on:
        complete:
          set:
            status: Done`

        const wrapper = mount(AutomationRulesEditor, {
            props: { modelValue: existingYaml },
            attachTo: document.body,
        })
        await flushPromises()

        // Verify the rule card is rendered
        const ruleNames = wrapper.findAll('.rule-name')
        expect(ruleNames.length).toBe(1)
        expect(ruleNames[0].text()).toBe('Close after complete')

        // Click Edit
        await findButton(wrapper, 'Edit')?.trigger('click')
        await flushPromises()

        // Dialog should open at step 0 with "Move a task" recipe pre-selected
        const activeRecipe = wrapper.find('.automation-builder__recipe-card--active')
        expect(activeRecipe.exists()).toBe(true)
        expect(activeRecipe.text()).toContain('Move a task')

        // Name should be pre-filled
        const nameInput = wrapper.find('input[placeholder="Human review after testing"]')
        expect((nameInput.element as HTMLInputElement).value).toBe('Close after complete')

        wrapper.unmount()
    })

    it('assigns tags through the guided dialog', async () => {
        const wrapper = mount(AutomationRulesEditor, {
            props: {
                modelValue: '',
                availableTags: ['urgent', 'ready-for-review', 'needs-qa'],
            },
            attachTo: document.body,
        })

        await findButton(wrapper, 'New rule')?.trigger('click')
        await flushPromises()

        await wrapper.findAll('button').find((b) => b.text().includes('Manage tags'))?.trigger('click')
        await flushPromises()
        await findButton(wrapper, 'Next')?.trigger('click')
        await flushPromises()

        // Default trigger for tags is 'updated' — click Next
        await findButton(wrapper, 'Next')?.trigger('click')
        await flushPromises()

        // Verify ChipListField rendered with the suggestions prop
        // The ChipListField shows suggestions when the composer is open
        const chipField = wrapper.findComponent({ name: 'ChipListField' })
        expect(chipField.exists()).toBe(true)
        expect(chipField.props('suggestions')).toEqual(['urgent', 'ready-for-review', 'needs-qa'])

        wrapper.unmount()
    })
})