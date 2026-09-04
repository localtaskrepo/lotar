import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import ColumnsMenu from '../components/ColumnsMenu.vue'
import type { FieldOption } from '../composables/useColumns'

const options: FieldOption[] = [
    { key: 'id', label: 'ID' },
    { key: 'title', label: 'Title' },
    { key: 'custom:sprint', label: 'sprint' },
]

function mountMenu(visible = new Set(['id'])) {
    const setVisibleCalls: Array<[string, boolean]> = []
    const wrapper = mount(ColumnsMenu, {
        props: {
            open: true,
            options,
            isVisible: (key: string) => visible.has(key),
            setVisible: (key: string, event: Event) => {
                const checked = (event.target as HTMLInputElement).checked
                setVisibleCalls.push([key, checked])
            },
            label: 'Table columns',
        },
        slots: {
            trigger: '<button class="trigger">Columns</button>',
        },
    })
    return { wrapper, setVisibleCalls }
}

describe('ColumnsMenu', () => {
    it('renders a labeled checkbox per option with checked state', () => {
        const { wrapper } = mountMenu()
        const labels = wrapper.findAll('label.column-option')
        expect(labels.map((l) => l.text().trim())).toEqual(['ID', 'Title', 'sprint'])
        const checks = wrapper.findAll('input[type="checkbox"]')
        expect((checks[0]!.element as HTMLInputElement).checked).toBe(true)
        expect((checks[1]!.element as HTMLInputElement).checked).toBe(false)
    })

    it('forwards checkbox changes through setVisible', async () => {
        const { wrapper, setVisibleCalls } = mountMenu()
        const titleCheck = wrapper
            .findAll('label.column-option')
            .find((l) => l.text().trim() === 'Title')!
            .find('input[type="checkbox"]')!
        await titleCheck.setValue(true)
        expect(setVisibleCalls).toContainEqual(['title', true])
    })

    it('emits reset when Reset is clicked and closes on Close', async () => {
        const { wrapper } = mountMenu()
        const buttons = wrapper.findAll('button')
        await buttons.find((b) => b.text().trim() === 'Reset')!.trigger('click')
        expect(wrapper.emitted('reset')).toHaveLength(1)
        await buttons.find((b) => b.text().trim() === 'Close')!.trigger('click')
        expect(wrapper.emitted('update:open')).toEqual([[false]])
    })

    it('closes on outside click and Escape, but not on clicks inside', async () => {
        const { wrapper } = mountMenu()
        // Click inside the popover: no close.
        await wrapper.find('.columns-popover').trigger('click')
        expect(wrapper.emitted('update:open')).toBeUndefined()
        // Escape closes.
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
        await Promise.resolve()
        expect(wrapper.emitted('update:open')).toEqual([[false]])
        // Reopen, then click outside: closes.
        await wrapper.setProps({ open: true })
        window.dispatchEvent(new MouseEvent('click', { bubbles: true }))
        await Promise.resolve()
        expect(wrapper.emitted('update:open')).toEqual([[false], [false]])
    })

    it('hides the popover when open is false but keeps the trigger', async () => {
        const { wrapper } = mountMenu()
        await wrapper.setProps({ open: false })
        expect(wrapper.find('.columns-popover').exists()).toBe(false)
        expect(wrapper.find('.trigger').exists()).toBe(true)
    })
})
