import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import type { AgentProfileInfo } from '../api/types'
import AssigneePicker from '../components/AssigneePicker.vue'
import { RECENT_ASSIGNEES_KEY } from '../utils/assigneeOptions'

const profiles: AgentProfileInfo[] = [
    { name: 'fix', runner: 'claude', model: 'claude-opus-5-5', effort: 'high', permissions: 'workspace' },
    { name: 'fix-light', runner: 'claude', model: 'claude-sonnet-5-5', effort: 'medium', permissions: 'workspace' },
    { name: 'review', runner: 'codex', model: 'gpt-5.5', permissions: 'read-only' },
]

function mountPicker(props: Record<string, unknown> = {}, initial = '') {
    const value = ref(initial)
    const escapes = ref(0)
    const Host = defineComponent({
        setup() {
            return () =>
                h('div', { onKeydown: (event: KeyboardEvent) => event.key === 'Escape' && escapes.value++ }, [
                    h(AssigneePicker, {
                        id: 'picker',
                        ariaLabel: 'Assignee',
                        emptyLabel: 'Unassigned',
                        people: ['sam', 'priya'],
                        whoami: 'mallox',
                        profiles,
                        ...props,
                        modelValue: value.value,
                        'onUpdate:modelValue': (next: string) => (value.value = next),
                    }),
                ])
        },
    })
    const wrapper = mount(Host, { attachTo: document.body })
    return { wrapper, value, escapes }
}

const trigger = (wrapper: ReturnType<typeof mount>) => wrapper.find('#picker')
const search = (wrapper: ReturnType<typeof mount>) => wrapper.find('[data-testid="assignee-picker-search"]')
const active = (wrapper: ReturnType<typeof mount>) =>
    wrapper.find('.assignee-picker__option--active').attributes('data-value')

describe('AssigneePicker', () => {
    beforeEach(() => localStorage.clear())

    it('shows the agent with its runner, model and effort on the button', () => {
        const { wrapper } = mountPicker({}, '@fix-light')
        expect(trigger(wrapper).attributes('data-value')).toBe('@fix-light')
        expect(trigger(wrapper).text()).toContain('@fix-light')
        expect(trigger(wrapper).text()).toContain('claude · claude-sonnet-5-5 · medium')
        wrapper.unmount()
    })

    it('groups the list and picks with the arrow keys and Enter', async () => {
        const { wrapper, value } = mountPicker()
        await trigger(wrapper).trigger('keydown', { key: 'ArrowDown' })
        const labels = wrapper.findAll('.assignee-picker__group-label').map((label) => label.text())
        expect(labels).toEqual(['You', 'People', 'fix', 'review'])
        expect(wrapper.find('.assignee-picker__section').text()).toBe('Agents')
        expect(active(wrapper)).toBe('')
        await search(wrapper).trigger('keydown', { key: 'ArrowDown' })
        expect(active(wrapper)).toBe('mallox')
        await search(wrapper).trigger('keydown', { key: 'End' })
        expect(active(wrapper)).toBe('@review')
        await search(wrapper).trigger('keydown', { key: 'Enter' })
        await flushPromises()
        expect(value.value).toBe('@review')
        expect(wrapper.find('[data-testid="assignee-picker-popover"]').exists()).toBe(false)
        expect(JSON.parse(localStorage.getItem(RECENT_ASSIGNEES_KEY) ?? '[]')).toEqual(['@review'])
        wrapper.unmount()
    })

    it('narrows the list by model words and takes the first match on Enter', async () => {
        const { wrapper, value } = mountPicker()
        await trigger(wrapper).trigger('click')
        await search(wrapper).setValue('sonnet')
        expect(wrapper.findAll('[role="option"]').map((option) => option.attributes('data-value'))).toEqual([
            '@fix-light',
            'sonnet',
        ])
        await search(wrapper).trigger('keydown', { key: 'Enter' })
        await flushPromises()
        expect(value.value).toBe('@fix-light')
        wrapper.unmount()
    })

    it('offers a typed name nobody has and sets it', async () => {
        const { wrapper, value } = mountPicker()
        await trigger(wrapper).trigger('click')
        await search(wrapper).setValue('newcomer@example.com')
        const custom = wrapper.find('[data-kind="custom"]')
        expect(custom.text()).toContain('Assign to “newcomer@example.com”')
        await custom.trigger('click')
        await flushPromises()
        expect(value.value).toBe('newcomer@example.com')
        wrapper.unmount()
    })

    it('opens with a typed letter as the search', async () => {
        const { wrapper } = mountPicker()
        await trigger(wrapper).trigger('keydown', { key: 'r' })
        expect((search(wrapper).element as HTMLInputElement).value).toBe('r')
        wrapper.unmount()
    })

    it('closes on Escape without letting it reach the dialog around it', async () => {
        const { wrapper, value, escapes } = mountPicker({}, 'sam')
        await trigger(wrapper).trigger('click')
        await search(wrapper).trigger('keydown', { key: 'Escape' })
        await flushPromises()
        expect(wrapper.find('[data-testid="assignee-picker-popover"]').exists()).toBe(false)
        expect(escapes.value).toBe(0)
        expect(value.value).toBe('sam')
        expect(document.activeElement).toBe(trigger(wrapper).element)
        wrapper.unmount()
    })

    it('lists only agents and no typed values for the agent run dialog', async () => {
        const { wrapper } = mountPicker({ includePeople: false, customVerb: null, emptyLabel: undefined })
        await trigger(wrapper).trigger('click')
        expect(wrapper.findAll('[role="option"]').map((option) => option.attributes('data-kind'))).toEqual([
            'agent',
            'agent',
            'agent',
        ])
        await search(wrapper).setValue('nobody')
        expect(wrapper.find('[data-kind="custom"]').exists()).toBe(false)
        expect(wrapper.text()).toContain('No match')
        wrapper.unmount()
    })
})
