import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it } from 'vitest'
import { defineComponent, nextTick } from 'vue'
import { useTaskTableState, type TaskTableEmit, type TaskTableProps } from '../composables/useTaskTableState'
import type { ColKey } from '../composables/useColumns'

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
                sort: { type: Object, default: undefined },
                bulk: { type: Boolean, default: false },
                statuses: { type: Array, default: () => ['open', 'done'] },
            },
            setup(componentProps) {
                return useTaskTableState(componentProps as TaskTableProps, emit)
            },
            template: '<div />',
        })

        const wrapper = mount(component, { props: props as any })
        return { wrapper, calls }
    }

    it('exposes default column visibility and persists toggles', async () => {
        const { wrapper } = mountHarness()
        expect(wrapper.vm.isVisible('title')).toBe(true)
        wrapper.vm.toggleColumn('tags', { target: { checked: false } } as unknown as Event)
        await nextTick()
        const raw = localStorage.getItem('lotar.taskTable.columns::ACME')
        expect(raw).toBeTruthy()
        const parsed = JSON.parse(raw || '[]')
        expect(parsed).not.toContain('tags')
        expect(wrapper.vm.isVisible('tags')).toBe(false)
    })

    it('allows hiding id and title', async () => {
        const { wrapper } = mountHarness()
        wrapper.vm.toggleColumn('id', { target: { checked: false } } as unknown as Event)
        wrapper.vm.toggleColumn('title', { target: { checked: false } } as unknown as Event)
        await nextTick()
        expect(wrapper.vm.isVisible('id')).toBe(false)
        expect(wrapper.vm.isVisible('title')).toBe(false)
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
        expect(wrapper.vm.sorted[0]!.id).toBe('ACME-1')
        wrapper.vm.onSort('title')
        await nextTick()
        expect(wrapper.vm.sort.key).toBe('title')
        wrapper.vm.onSort('title')
        await nextTick()
        expect(wrapper.vm.sort.dir).toBe('desc')
    })

    it('emits update:sort instead of sorting locally when controlled', async () => {
        const { wrapper, calls } = mountHarness({ sort: { key: null, dir: 'desc' } })
        expect(wrapper.vm.sorted.map((t: any) => t.id)).toEqual(['ACME-1', 'ACME-2'])
        wrapper.vm.onSort('status')
        await nextTick()
        expect(calls).toContainEqual(['update:sort', { key: 'status', dir: 'asc' }])
        // No local mutation: the caller-owned order is rendered as-is.
        expect(wrapper.vm.sort.key).toBe(null)
        // A second click toggles direction through the same event.
        calls.length = 0
        await wrapper.setProps({ sort: { key: 'status', dir: 'asc' } })
        wrapper.vm.onSort('status')
        await nextTick()
        expect(calls).toContainEqual(['update:sort', { key: 'status', dir: 'desc' }])
    })

    it('ignores header clicks for columns without a server sort in controlled mode', async () => {
        const { wrapper, calls } = mountHarness({ sort: { key: null, dir: 'desc' } })
        wrapper.vm.onSort('not-a-column' as ColKey)
        await nextTick()
        expect(calls.filter((c) => c[0] === 'update:sort')).toEqual([])
        expect(wrapper.vm.isSortableCol('not-a-column')).toBe(false)
        // Restored backend sorts make every builtin column sortable again.
        for (const col of ['title', 'tags', 'sprints', 'reporter', 'due_date']) {
            expect(wrapper.vm.isSortableCol(col)).toBe(true)
        }
        expect(wrapper.vm.isSortableCol('custom:Risk')).toBe(true)
    })

    it('sorts title, tags, and sprints headers through the shared comparator', async () => {
        const customTasks = [
            { ...sampleTasks[0]!, id: 'ACME-1', title: 'b', tags: ['x'], sprints: [10] },
            { ...sampleTasks[1]!, id: 'ACME-2', title: 'a', tags: [], sprints: [2] },
        ]
        const calls: any[] = []
        const emit = ((...args: any[]) => calls.push(args)) as TaskTableEmit
        const component = defineComponent({
            props: { tasks: { type: Array, default: () => customTasks }, projectKey: { type: String, default: 'ACME' } },
            setup(componentProps) {
                return useTaskTableState(componentProps as TaskTableProps, emit)
            },
            template: '<div />',
        })
        const wrapper = mount(component)
        wrapper.vm.setSort('title' as any, 'asc')
        await nextTick()
        expect(wrapper.vm.sorted.map((t: any) => t.id)).toEqual(['ACME-2', 'ACME-1'])
        wrapper.vm.setSort('tags' as any, 'asc')
        await nextTick()
        expect(wrapper.vm.sorted.map((t: any) => t.id)).toEqual(['ACME-2', 'ACME-1'])
        wrapper.vm.setSort('sprints' as any, 'asc')
        await nextTick()
        expect(wrapper.vm.sorted.map((t: any) => t.id)).toEqual(['ACME-2', 'ACME-1'])
    })

    it('reloads the stored sort per project without leaking across projects', async () => {
        const { wrapper } = mountHarness()
        wrapper.vm.setSort('status', 'asc')
        await nextTick()
        expect(JSON.parse(localStorage.getItem('lotar.taskTable.sort::ACME') || 'null')).toEqual({ key: 'status', dir: 'asc' })
        localStorage.setItem('lotar.taskTable.sort::OTHER', JSON.stringify({ key: 'priority', dir: 'desc' }))
        await wrapper.setProps({ projectKey: 'OTHER' })
        await nextTick()
        expect(wrapper.vm.sort.key).toBe('priority')
        expect(wrapper.vm.sort.dir).toBe('desc')
        // Switching to a project with no stored sort falls back to the default,
        // not to the previous project's sort.
        await wrapper.setProps({ projectKey: 'THIRD' })
        await nextTick()
        expect(wrapper.vm.sort.key).toBe(null)
        expect(wrapper.vm.sort.dir).toBe('desc')
        // The reload must not have overwritten OTHER's stored value.
        expect(JSON.parse(localStorage.getItem('lotar.taskTable.sort::OTHER') || 'null')).toEqual({ key: 'priority', dir: 'desc' })
    })

    it('sorts custom-field columns through the shared contract in standalone mode', async () => {
        const customTasks = [
            { ...sampleTasks[0]!, id: 'ACME-1', custom_fields: { Risk: 'b' } },
            { ...sampleTasks[1]!, id: 'ACME-2', custom_fields: { Risk: 'a' } },
        ]
        const calls: any[] = []
        const emit = ((...args: any[]) => calls.push(args)) as TaskTableEmit
        const component = defineComponent({
            props: { tasks: { type: Array, default: () => customTasks }, projectKey: { type: String, default: 'ACME' } },
            setup(componentProps) {
                return useTaskTableState(componentProps as TaskTableProps, emit)
            },
            template: '<div />',
        })
        const wrapper = mount(component)
        wrapper.vm.setSort('custom:Risk' as any, 'asc')
        await nextTick()
        expect(wrapper.vm.sorted.map((t: any) => t.id)).toEqual(['ACME-2', 'ACME-1'])
    })

    it('preserves other pages on select-all and never echoes a cleared parent selection', async () => {
        const { wrapper, calls } = mountHarness({ selectedIds: ['ACME-99'] })
        wrapper.vm.toggleAll({ target: { checked: true } } as unknown as Event)
        expect(calls[calls.length - 1]).toEqual(['update:selectedIds', ['ACME-99', 'ACME-1', 'ACME-2']])
        calls.length = 0
        await wrapper.setProps({ selectedIds: [], loading: true })
        wrapper.vm.toggleOne('ACME-1', { target: { checked: true } } as unknown as Event)
        wrapper.vm.toggleAll({ target: { checked: true } } as unknown as Event)
        await nextTick()
        expect(wrapper.vm.selected).toEqual([])
        expect(calls).toEqual([])
        wrapper.unmount()
    })
})
