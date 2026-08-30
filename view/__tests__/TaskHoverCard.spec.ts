import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import TaskHoverCard from '../components/TaskHoverCard.vue'

describe('TaskHoverCard', () => {
    it('renders updated and sprints when present', () => {
        const wrapper = mount(TaskHoverCard, {
            props: {
                task: {
                    id: 'PRJ-1',
                    title: 'Demo',
                    tags: [],
                    status: '',
                    priority: '',
                    assignee: '',
                    due_date: '',
                    sprints: [1, 2],
                    modified: '2026-01-02T10:00:00Z',
                } as any,
            },
            slots: {
                default: '<span>trigger</span>',
            },
        })

        expect(wrapper.text()).toContain('Updated')
        expect(wrapper.text()).toContain('Sprints')
        expect(wrapper.text()).toContain('#1')
    })

    it('renders custom fields only when enabled in the fields map', () => {
        const task = {
            id: 'PRJ-2',
            title: 'Custom fields demo',
            tags: [],
            status: '',
            priority: '',
            assignee: '',
            due_date: '',
            modified: '',
            custom_fields: {
                sprint: 'Sprint-42',
                iteration: 'iter-7',
                empty: '',
            },
        } as any

        const hidden = mount(TaskHoverCard, {
            props: { task, fields: { 'custom:sprint': false, 'custom:iteration': true } as any },
            slots: { default: '<span>trigger</span>' },
        })
        expect(hidden.text()).not.toContain('Sprint-42')
        expect(hidden.text()).toContain('iter-7')
        expect(hidden.text()).not.toContain('empty')

        const shown = mount(TaskHoverCard, {
            props: { task, fields: { 'custom:sprint': true } as any },
            slots: { default: '<span>trigger</span>' },
        })
        expect(shown.text()).toContain('sprint')
        expect(shown.text()).toContain('Sprint-42')
    })
})
