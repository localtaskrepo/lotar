import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import TaskHoverCard from '../components/TaskHoverCard.vue'

describe('TaskHoverCard', () => {
    it('renders summary as markdown', () => {
        const wrapper = mount(TaskHoverCard, {
            props: {
                task: {
                    id: 'PRJ-1',
                    title: 'Demo',
                    subtitle: '',
                    description: 'Hello **world**',
                    tags: [],
                    status: '',
                    priority: '',
                    assignee: '',
                    due_date: '',
                } as any,
            },
            slots: {
                default: '<span>trigger</span>',
            },
        })

        const summary = wrapper.find('.task-hover-card__summary')
        expect(summary.exists()).toBe(true)

        const strong = summary.find('strong')
        expect(strong.exists()).toBe(true)
        expect(strong.text()).toBe('world')
    })
})
