import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { defineComponent, nextTick } from 'vue'
import { useColumns } from '../composables/useColumns'

function mountHarness() {
    const component = defineComponent({
        setup() {
            return useColumns()
        },
        template: '<div />',
    })
    const wrapper = mount(component)
    return { wrapper }
}

describe('useColumns', () => {
    beforeEach(() => {
        localStorage.clear()
        // The custom-field registry is shared app state; reset it between tests.
        useColumns().setCustomFieldKeys([])
    })

    afterEach(() => {
        localStorage.clear()
    })

    it('exposes the built-in column list on first render', async () => {
        const { wrapper } = mountHarness()
        await nextTick()
        expect(wrapper.vm.columnOrder.length).toBeGreaterThan(0)
        expect(wrapper.vm.columnOrder).toContain('id')
        expect(wrapper.vm.columnOrder).toContain('title')
        expect(wrapper.vm.columnOrder).not.toContain('custom:*')
    })

    it('appends custom field entries and filters out the wildcard', async () => {
        const { wrapper } = mountHarness()
        await nextTick()
        wrapper.vm.setCustomFieldKeys(['*', 'sprint', 'iteration'])
        await nextTick()
        expect(wrapper.vm.columnOrder).toContain('custom:sprint')
        expect(wrapper.vm.columnOrder).toContain('custom:iteration')
        expect(wrapper.vm.columnOrder).not.toContain('custom:*')
    })

    it('strips stored custom field keys that are no longer declared', async () => {
        const { wrapper } = mountHarness()
        await nextTick()
        wrapper.vm.setCustomFieldKeys(['sprint'])
        wrapper.vm.columns = ['id', 'custom:sprint', 'custom:stale']
        wrapper.vm.setCustomFieldKeys([])
        await nextTick()
        expect(wrapper.vm.columns).not.toContain('custom:stale')
        expect(wrapper.vm.columns).not.toContain('custom:sprint')
    })

    it('renders header labels for custom fields using the bare name', async () => {
        const { wrapper } = mountHarness()
        await nextTick()
        wrapper.vm.setCustomFieldKeys(['sprint'])
        await nextTick()
        expect(wrapper.vm.headerLabel('custom:sprint')).toBe('sprint')
        expect(wrapper.vm.headerLabel('id')).toBe('ID')
    })

    it('relabels the project key when changed', async () => {
        const { wrapper } = mountHarness()
        await nextTick()
        wrapper.vm.setProjectKey('TEAM')
        wrapper.vm.setCustomFieldKeys(['sprint'])
        wrapper.vm.columns = ['id', 'custom:sprint']
        await nextTick()
        const stored = JSON.parse(localStorage.getItem('lotar.taskTable.columns::TEAM') || '[]')
        expect(stored).toContain('custom:sprint')
    })

    it('restores saved custom columns once custom fields become known', async () => {
        localStorage.setItem(
            'lotar.taskTable.columns',
            JSON.stringify(['id', 'title', 'custom:sprint']),
        )
        const { wrapper } = mountHarness()
        await nextTick()
        // Config loads async in real usage: customs become known only after the store exists.
        wrapper.vm.setCustomFieldKeys(['sprint'])
        await nextTick()
        expect(wrapper.vm.columns).toContain('custom:sprint')
        const stored = JSON.parse(localStorage.getItem('lotar.taskTable.columns') || '[]')
        expect(stored).toContain('custom:sprint')
    })

    it('migrates legacy boolean-map storage to the array format', async () => {
        localStorage.setItem('lotar.boardFields::ACME', JSON.stringify({ id: true, tags: false, sprints: true }))
        const component = defineComponent({
            setup() {
                return {
                    store: useColumns({ storagePrefix: 'lotar.boardFields', defaultVisible: ['id', 'title'] }),
                }
            },
            template: '<div />',
        })
        const wrapper = mount(component)
        wrapper.vm.store.setProjectKey('ACME')
        await nextTick()
        expect(wrapper.vm.store.isVisible('id')).toBe(true)
        expect(wrapper.vm.store.isVisible('sprints')).toBe(true)
        expect(wrapper.vm.store.isVisible('tags')).toBe(false)
        // The migrated array format is persisted on the first user change.
        wrapper.vm.store.toggleColumn('title', { target: { checked: false } } as unknown as Event)
        await nextTick()
        const stored = JSON.parse(localStorage.getItem('lotar.boardFields.columns::ACME') || 'null')
        expect(Array.isArray(stored)).toBe(true)
        expect(stored).toContain('sprints')
        expect(stored).not.toContain('tags')
        expect(stored).not.toContain('title')
    })

    it('preserves an explicitly empty selection instead of restoring defaults', async () => {
        localStorage.setItem('lotar.taskTable.columns', JSON.stringify([]))
        const { wrapper } = mountHarness()
        await nextTick()
        expect(wrapper.vm.columns).toEqual([])
        const stored = JSON.parse(localStorage.getItem('lotar.taskTable.columns') || 'null')
        expect(stored).toEqual([])
    })

    it('migrates legacy bare custom field names from boolean maps', async () => {
        useColumns().setCustomFieldKeys(['sprint'])
        localStorage.setItem('lotar.boardFields::ACME', JSON.stringify({ id: true, sprint: true, tags: false }))
        const component = defineComponent({
            setup() {
                return {
                    store: useColumns({ storagePrefix: 'lotar.boardFields', defaultVisible: ['id'] }),
                }
            },
            template: '<div />',
        })
        const wrapper = mount(component)
        wrapper.vm.store.setProjectKey('ACME')
        await nextTick()
        expect(wrapper.vm.store.isVisible('custom:sprint')).toBe(true)
        expect(wrapper.vm.store.isVisible('id')).toBe(true)
        expect(wrapper.vm.store.isVisible('tags')).toBe(false)
    })
})
