import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import PageHeader from '../components/PageHeader.vue'

describe('PageHeader', () => {
    it('orders view options, then Clear filters, then Reload in the title row', async () => {
        const wrapper = mount(PageHeader, {
            props: { clearLabel: 'Clear filters', canClear: true, reloadLabel: 'Refresh things', reloading: false },
            slots: {
                title: 'Things',
                subtitle: 'About things',
                view: '<button type="button">Columns</button>',
                toolbar: '<button type="button">Export</button>',
            },
        })
        const controls = wrapper.get('.app-page-header__controls').findAll('button')
        expect(controls.map((b) => b.text() || b.attributes('aria-label'))).toEqual([
            'Columns',
            'Clear filters',
            'Refresh things',
        ])
        expect(wrapper.get('h1').text()).toBe('Things')
        expect(wrapper.get('.app-page-header__subtitle').text()).toBe('About things')
        expect(wrapper.get('.app-page-header__toolbar').text()).toBe('Export')

        await controls[1]!.trigger('click')
        await controls[2]!.trigger('click')
        expect(wrapper.emitted('clear')).toHaveLength(1)
        expect(wrapper.emitted('reload')).toHaveLength(1)
    })

    it('hides controls that a page does not use and disables Clear with nothing to clear', () => {
        const bare = mount(PageHeader, { slots: { title: 'Scan' } })
        expect(bare.findAll('button')).toHaveLength(0)
        expect(bare.find('.app-page-header__toolbar').exists()).toBe(false)
        expect(bare.find('.app-page-header__subtitle').exists()).toBe(false)

        const idle = mount(PageHeader, {
            props: { clearLabel: 'Clear filters', canClear: false },
            slots: { title: 'T' },
        })
        expect(idle.get('[aria-label="Clear filters"]').attributes('disabled')).toBeDefined()
    })
})
