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
})
