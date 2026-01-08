import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it } from 'vitest'
import { defineComponent, nextTick } from 'vue'
import { useTaskTableState, type TaskTableEmit, type TaskTableProps } from '../composables/useTaskTableState'

const sampleTasks = [
    { id: 'ACME-1', title: 'Alpha', status: 'open', priority: 'med', task_type: 'task', reporter: 'lee', assignee: 'sam', created: '2024-01-01T10:00:00Z', modified: '2024-01-01T11:00:00Z', tags: ['a'], relationships: {}, comments: [], custom_fields: {} },
    { id: 'ACME-2', title: 'Beta', status: 'done', priority: 'low', task_type: 'task', reporter: 'jam', assignee: 'kai', created: '2024-01-02T10:00:00Z', modified: '2024-01-02T11:00:00Z', tags: ['b'], relationships: {}, comments: [], custom_fields: {} },
]

describe('useTaskTableState', () => {
    beforeEach(() => {
        localStorage.clear()
    })

    function mountHarness(props: Partial<TaskTableProps> = {}) {
        const calls: any[] = []
        const emit = ((...args: any[]) => {
            calls.push(args)
        }) as TaskTableEmit
        const component = defineComponent({
            props: {
                tasks: { type: Array, default: () => sampleTasks },
                loading: { type: Boolean, default: false },
                selectable: { type: Boolean, default: true },
                selectedIds: { type: Array, default: () => [] },
                projectKey: { type: String, default: 'ACME' },
                bulk: { type: Boolean, default: false },
                statuses: { type: Array, default: () => ['open', 'done'] },
            },
            setup(componentProps) {
                return useTaskTableState(componentProps as TaskTableProps, emit)
            },
            template: '<div />',
        })

        const wrapper = mount(component, { props })
        return { wrapper, calls }
    }

    it('initializes columns with defaults and persists toggles', async () => {
        const { wrapper } = mountHarness()
        expect(wrapper.vm.columns).toContain('title')
        wrapper.vm.toggleColumn('tags', { target: { checked: false } } as unknown as Event)
        await nextTick()
        const raw = localStorage.getItem('lotar.taskTable.columns::ACME')
        expect(raw).toBeTruthy()
        const parsed = JSON.parse(raw || '[]')
        expect(parsed).not.toContain('tags')
    })

    it('allows hiding id and title', async () => {
        const { wrapper } = mountHarness()
        wrapper.vm.toggleColumn('id', { target: { checked: false } } as unknown as Event)
        wrapper.vm.toggleColumn('title', { target: { checked: false } } as unknown as Event)
        await nextTick()
        expect(wrapper.vm.columns).not.toContain('id')
        expect(wrapper.vm.columns).not.toContain('title')
        const raw = localStorage.getItem('lotar.taskTable.columns::ACME')
        expect(raw).toBeTruthy()
        const parsed = JSON.parse(raw || '[]')
        expect(parsed).not.toContain('id')
        expect(parsed).not.toContain('title')
    })

    it('tracks selection and emits updates', async () => {
        const { wrapper, calls } = mountHarness()
        wrapper.vm.toggleOne('ACME-1', { target: { checked: true } } as unknown as Event)
        await nextTick()
        expect(wrapper.vm.selected).toContain('ACME-1')
        expect(calls).toContainEqual(['update:selectedIds', ['ACME-1']])
    })

    it('sorts rows when invoking onSort', async () => {
        const { wrapper } = mountHarness()
        expect(wrapper.vm.sorted[0].id).toBe('ACME-1')
        wrapper.vm.onSort('title')
        await nextTick()
        expect(wrapper.vm.sort.key).toBe('title')
        wrapper.vm.onSort('title')
        await nextTick()
        expect(wrapper.vm.sort.dir).toBe('desc')
    })
})
