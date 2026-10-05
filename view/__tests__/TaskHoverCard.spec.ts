import { mount, flushPromises } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import TaskHoverCard from '../components/TaskHoverCard.vue'
import { invalidateCompletionPolicies } from '../composables/useCompletionPolicy'

const showConfigMock = vi.hoisted(() => vi.fn(async () => ({})))
vi.mock('../api/client', () => ({
    api: {
        showConfig: showConfigMock,
    },
}))

function dueTask(overrides: Record<string, unknown> = {}) {
    return {
        id: 'PRJ-9',
        title: 'Due task',
        tags: [],
        status: 'Todo',
        priority: '',
        assignee: '',
        due_date: '2025-12-01',
        sprints: [],
        modified: '',
        ...overrides,
    } as any
}

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

    describe('overdue tone (DEV-21 shared policy)', () => {
        beforeEach(() => {
            vi.useFakeTimers()
            vi.setSystemTime(new Date('2026-01-05T12:00:00'))
            showConfigMock.mockReset()
            showConfigMock.mockImplementation(async () => ({
                issue_states: ['Todo', 'Doing', 'Shipped'],
                effective_done_states: ['Shipped'],
                done_states_mode: 'explicit',
                task_calendar_day: '2026-01-05',
            }))
            invalidateCompletionPolicies()
        })

        afterEach(() => {
            vi.useRealTimers()
        })

        it('tones past-due open tasks as overdue', async () => {
            const wrapper = mount(TaskHoverCard, {
                props: { task: dueTask({ status: 'Todo' }) },
                slots: { default: '<span>trigger</span>' },
            })
            await flushPromises()

            const due = wrapper.find('.task-hover-card__due')
            expect(due.classes()).toContain('is-overdue')
            expect(due.text()).toContain('days ago')
        })

        it('never tones terminal tasks as overdue, including custom names like Shipped', async () => {
            const wrapper = mount(TaskHoverCard, {
                props: { task: dueTask({ status: 'Shipped' }) },
                slots: { default: '<span>trigger</span>' },
            })
            await flushPromises()

            const due = wrapper.find('.task-hover-card__due')
            expect(due.exists()).toBe(true)
            expect(due.classes()).not.toContain('is-overdue')
            expect(due.text()).toContain('days ago')
        })

        it('keeps due-today tasks out of the overdue tone', async () => {
            const wrapper = mount(TaskHoverCard, {
                props: { task: dueTask({ status: 'Todo', due_date: '2026-01-05' }) },
                slots: { default: '<span>trigger</span>' },
            })
            await flushPromises()

            const due = wrapper.find('.task-hover-card__due')
            expect(due.classes()).not.toContain('is-overdue')
            expect(due.classes()).toContain('is-due-today')
        })
    })
})
