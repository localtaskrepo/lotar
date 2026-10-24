import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import UiPagination from '../components/UiPagination.vue'

describe('UiPagination', () => {
    it('renders nothing when the range is empty or fits a single page', () => {
        const empty = mount(UiPagination, {
            props: { offset: 0, total: 0, pageSize: 50, ariaLabel: 'Task list pagination' },
        })
        expect(empty.find('nav').exists()).toBe(false)

        const single = mount(UiPagination, {
            props: { offset: 0, total: 50, pageSize: 50, ariaLabel: 'Task list pagination' },
        })
        expect(single.find('nav').exists()).toBe(false)
    })

    it('exposes the scoped nav label, standard prev/next names, and the current page marker', () => {
        const wrapper = mount(UiPagination, {
            props: { offset: 50, total: 120, pageSize: 50, ariaLabel: 'Backlog pagination' },
        })
        const nav = wrapper.find('nav[aria-label="Backlog pagination"]')
        expect(nav.exists()).toBe(true)
        expect(nav.find('button[aria-label="Previous page"]').exists()).toBe(true)
        expect(nav.find('button[aria-label="Next page"]').exists()).toBe(true)
        const current = nav.find('button[aria-current="page"]')
        expect(current.attributes('aria-label')).toBe('Page 2')
        expect(current.text()).toBe('2')
        expect(nav.find('.ui-pagination__status').attributes('aria-live')).toBe('polite')
        expect(nav.find('.ui-pagination__status').text()).toBe('Page 2 of 3')
    })

    it('disables prev on the first page and next on the last page', async () => {
        const wrapper = mount(UiPagination, {
            props: { offset: 0, total: 120, pageSize: 50, ariaLabel: 'Task list pagination' },
        })
        expect(wrapper.find('button[aria-label="Previous page"]').attributes('disabled')).toBeDefined()
        expect(wrapper.find('button[aria-label="Next page"]').attributes('disabled')).toBeUndefined()

        await wrapper.setProps({ offset: 100 })
        expect(wrapper.find('button[aria-label="Previous page"]').attributes('disabled')).toBeUndefined()
        expect(wrapper.find('button[aria-label="Next page"]').attributes('disabled')).toBeDefined()
    })

    it('disables every control while disabled', () => {
        const wrapper = mount(UiPagination, {
            props: { offset: 50, total: 120, pageSize: 50, disabled: true, ariaLabel: 'Task list pagination' },
        })
        const buttons = wrapper.findAll('button')
        expect(buttons.length).toBeGreaterThan(0)
        buttons.forEach((button) => {
            expect(button.attributes('disabled')).toBeDefined()
        })
    })

    it('emits the target page on page jump, next, and prev clicks', async () => {
        const wrapper = mount(UiPagination, {
            props: { offset: 0, total: 200, pageSize: 50, ariaLabel: 'Task list pagination' },
        })
        await wrapper.find('button[aria-label="Page 3"]').trigger('click')
        expect(wrapper.emitted('page')).toEqual([[3]])

        await wrapper.find('button[aria-label="Next page"]').trigger('click')
        expect(wrapper.emitted('page')).toEqual([[3], [2]])

        await wrapper.setProps({ offset: 50 })
        await wrapper.find('button[aria-label="Previous page"]').trigger('click')
        expect(wrapper.emitted('page')).toEqual([[3], [2], [1]])
    })

    it('never emits while disabled or for the current page', async () => {
        const wrapper = mount(UiPagination, {
            props: { offset: 0, total: 100, pageSize: 50, disabled: true, ariaLabel: 'Task list pagination' },
        })
        await wrapper.find('button[aria-label="Next page"]').trigger('click')
        await wrapper.find('button[aria-label="Page 2"]').trigger('click')
        expect(wrapper.emitted('page')).toBeUndefined()

        await wrapper.setProps({ disabled: false })
        await wrapper.find('button[aria-label="Page 1"]').trigger('click')
        expect(wrapper.emitted('page')).toBeUndefined()
    })

    it('allows the current page to align a non-page-aligned URL offset', async () => {
        const wrapper = mount(UiPagination, {
            props: { offset: 10, total: 120, pageSize: 50, ariaLabel: 'Task list pagination' },
        })
        await wrapper.find('button[aria-label="Page 1"]').trigger('click')
        expect(wrapper.emitted('page')).toEqual([[1]])
        wrapper.unmount()
    })

    it('normalizes invalid sizes and offsets without non-finite page controls', async () => {
        for (const pageSize of [0.5, 0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
            const wrapper = mount(UiPagination, {
                props: { offset: Number.NaN, total: 3, pageSize, ariaLabel: 'Task list pagination' },
            })
            expect(wrapper.find('.ui-pagination__status').text()).toBe('Page 1 of 3')
            await wrapper.find('button[aria-label="Next page"]').trigger('click')
            expect(wrapper.emitted('page')).toEqual([[2]])
            wrapper.unmount()
        }
    })

    it('bounds the numbered window with ellipsis so long lists stay compact', () => {
        const wrapper = mount(UiPagination, {
            props: { offset: 250, total: 600, pageSize: 50, ariaLabel: 'Sprint #1 pagination' },
        })
        const nav = wrapper.find('nav[aria-label="Sprint #1 pagination"]')
        const numbers = nav.findAll('button').map((b) => b.text())
        // Prev + first/current+/-1/last + Next.
        expect(numbers).toEqual(['Prev', '1', '5', '6', '7', '12', 'Next'])
        const gaps = nav.findAll('.ui-pagination__ellipsis')
        expect(gaps).toHaveLength(2)

        // Window stays bounded at the edges too: current=1 -> 1, 2, ..., 12.
        const edge = mount(UiPagination, {
            props: { offset: 0, total: 600, pageSize: 50, ariaLabel: 'Sprint #1 pagination' },
        })
        expect(edge.findAll('button').map((b) => b.text())).toEqual(['Prev', '1', '2', '12', 'Next'])
        expect(edge.findAll('.ui-pagination__ellipsis')).toHaveLength(1)
    })

    it('shows every page without ellipsis when the list has at most seven pages', () => {
        const wrapper = mount(UiPagination, {
            props: { offset: 200, total: 350, pageSize: 50, ariaLabel: 'Task list pagination' },
        })
        expect(wrapper.findAll('button').map((b) => b.text())).toEqual([
            'Prev',
            '1',
            '2',
            '3',
            '4',
            '5',
            '6',
            '7',
            'Next',
        ])
        expect(wrapper.findAll('.ui-pagination__ellipsis')).toHaveLength(0)
    })
})
