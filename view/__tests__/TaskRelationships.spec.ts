import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, h, nextTick, ref } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import TaskPanelRelationshipsTab from '../components/task-panel/TaskPanelRelationshipsTab.vue'
import { useTaskRelationships } from '../composables/useTaskRelationships'
import { relationshipMime, relationshipMutation } from '../utils/relationships'

const apiMock = vi.hoisted(() => ({
    suggestTasks: vi.fn(),
    relationshipMutate: vi.fn(async (_request: unknown) => {}),
}))
const navigateTask = vi.hoisted(() => vi.fn())
vi.mock('../api/client', () => ({ api: apiMock }))
vi.mock('../composables/useTaskPanelController', () => ({ useTaskPanelController: () => ({ navigateTask }) }))

function editor(extra: Record<string, unknown> = {}) {
    return mount(TaskPanelRelationshipsTab, {
        props: {
            mode: 'edit',
            taskId: 'DEV-8',
            relationships: { depends_on: 'DEV-1', related: 'DEV-1', parent: 'DEV-2' },
            mutate: vi.fn(async () => {}),
            ...extra,
        },
        global: { stubs: { DependencyGraph: true, ReloadButton: true } },
    })
}
async function openAdd(wrapper: ReturnType<typeof editor>, category = 'Depends on') {
    await wrapper.get(`section[aria-label="${category}"] .relationship-add-toggle`).trigger('click')
}
async function drag(wrapper: ReturnType<typeof editor>, destination: string) {
    const dataTransfer = { types: [relationshipMime], setData: vi.fn(), effectAllowed: '' }
    await wrapper.get('.relationship-chips li').trigger('dragstart', { dataTransfer })
    await wrapper.get(`section[aria-label="${destination}"]`).trigger('drop', { dataTransfer })
}
afterEach(() => {
    vi.useRealTimers()
    vi.clearAllMocks()
})

describe('Relationship category editor', () => {
    it('removes directly and preserves native modified-link navigation', async () => {
        const mutate = vi.fn(async () => {})
        const wrapper = editor({ mutate })
        await wrapper.get('button[aria-label="Remove DEV-1 from Depends on"]').trigger('click')
        await flushPromises()
        expect(mutate).toHaveBeenCalledExactlyOnceWith({ action: 'remove', kind: 'depends_on', target: 'DEV-1' })
        const link = wrapper.get('a[href="/task/DEV-1"]')
        // jsdom cannot perform native document navigation for modified clicks.
        link.element.addEventListener('click', (event) => {
            if ((event as MouseEvent).ctrlKey || (event as MouseEvent).metaKey) event.preventDefault()
        })
        await link.trigger('click', { ctrlKey: true })
        await link.trigger('click', { metaKey: true })
        expect(navigateTask).not.toHaveBeenCalled()
        await link.trigger('click')
        expect(navigateTask).toHaveBeenCalledExactlyOnceWith('DEV-1')
        wrapper.unmount()
    })
    it('never submits the real outer task form when adding by button or Enter without suggestions', async () => {
        const outerSubmit = vi.fn(),
            mutate = vi.fn(async () => {})
        const wrapper = mount(
            defineComponent({
                setup: () => () =>
                    h(
                        'form',
                        {
                            onSubmit: (event: Event) => {
                                event.preventDefault()
                                outerSubmit()
                            },
                        },
                        [h(TaskPanelRelationshipsTab, { mode: 'create', relationships: {}, mutate })],
                    ),
            }),
            { attachTo: document.body, global: { stubs: { DependencyGraph: true, ReloadButton: true } } },
        )
        expect(wrapper.findAll('form')).toHaveLength(1)
        await wrapper.get('section[aria-label="Depends on"] .relationship-add-toggle').trigger('click')
        const input = wrapper.get('section[aria-label="Depends on"] input')
        await input.setValue('DEV-1')
        const addButton = wrapper.get('section[aria-label="Depends on"] .relationship-add > button')
            .element as HTMLButtonElement
        addButton.click()
        await flushPromises()
        expect(mutate).toHaveBeenLastCalledWith({ action: 'add', kind: 'depends_on', target: 'DEV-1' })
        await input.setValue('DEV-2')
        const enter = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
        input.element.dispatchEvent(enter)
        // jsdom has no implicit Enter-submit default action; reproduce it only
        // when the handler has not cancelled the browser's native default.
        if (!enter.defaultPrevented) (wrapper.element as HTMLFormElement).requestSubmit()
        await flushPromises()
        expect(enter.defaultPrevented).toBe(true)
        expect(mutate).toHaveBeenLastCalledWith({ action: 'add', kind: 'depends_on', target: 'DEV-2' })
        expect(outerSubmit).not.toHaveBeenCalled()
        wrapper.unmount()
    })
    it('renders compact categories with add and secondary controls on demand', async () => {
        const wrapper = editor()
        expect(wrapper.findAll('.relationship-category')).toHaveLength(7)
        expect(wrapper.find('a[href="/task/DEV-1"]').exists()).toBe(true)
        expect(wrapper.findAll('.relationship-add')).toHaveLength(0)
        expect(wrapper.findAll('select')).toHaveLength(0)
        expect(wrapper.findComponent({ name: 'DependencyGraph' }).exists()).toBe(false)
        expect(wrapper.find('button[aria-label="Remove DEV-1 from Depends on"]').exists()).toBe(true)
        expect(wrapper.text()).not.toContain('Reload')
        expect(wrapper.text()).not.toContain('Move to')
        await openAdd(wrapper)
        expect(wrapper.findAll('.relationship-add')).toHaveLength(1)
        expect(wrapper.findAll('select')).toHaveLength(0)
        await openAdd(wrapper, 'Blocks')
        expect(wrapper.findAll('.relationship-add')).toHaveLength(1)
        expect(wrapper.get('input').attributes('aria-label')).toBe('Add blocks')
        wrapper.unmount()
    })
    it('loads the graph preview only when expanded and collapses it after a task switch', async () => {
        const wrapper = editor()
        const disclosure = wrapper.get('.relationship-preview')
        expect(wrapper.find('dependency-graph-stub').exists()).toBe(false)
        ;(disclosure.element as HTMLDetailsElement).open = true
        await disclosure.trigger('toggle')
        expect(wrapper.find('dependency-graph-stub').exists()).toBe(true)
        await wrapper.setProps({ taskId: 'DEV-9' })
        expect(wrapper.find('dependency-graph-stub').exists()).toBe(false)
        expect((disclosure.element as HTMLDetailsElement).open).toBe(false)
        wrapper.unmount()
    })
    it('focuses the on-demand editor and restores focus when keyboard actions close', async () => {
        const host = document.createElement('div')
        document.body.appendChild(host)
        const wrapper = mount(TaskPanelRelationshipsTab, {
            attachTo: host,
            props: {
                mode: 'edit',
                taskId: 'DEV-8',
                relationships: { depends_on: 'DEV-1' },
                mutate: vi.fn(async () => {}),
            },
            global: { stubs: { DependencyGraph: true, ReloadButton: true } },
        })
        try {
            await openAdd(wrapper)
            await flushPromises()
            const input = wrapper.get('input')
            expect(document.activeElement).toBe(input.element)
            await input.trigger('keydown', { key: 'Escape' })
            expect(wrapper.find('input').exists()).toBe(false)
            expect(document.activeElement).toBe(
                wrapper.get('section[aria-label="Depends on"] .relationship-add-toggle').element,
            )
        } finally {
            wrapper.unmount()
            host.remove()
        }
    })
    it('does not request a same-category move and explicitly confirms direction reversal', async () => {
        const mutate = vi.fn(async () => {})
        const wrapper = editor({ mutate })
        await drag(wrapper, 'Depends on')
        expect(mutate).not.toHaveBeenCalled()
        await drag(wrapper, 'Blocks')
        expect(mutate).not.toHaveBeenCalled()
        expect(wrapper.get('[role="alertdialog"]').text()).toContain('reverses dependency direction')
        await wrapper.get('[role="alertdialog"] button').trigger('click')
        await flushPromises()
        expect(mutate).toHaveBeenCalledWith({
            action: 'change_kind',
            kind: 'blocks',
            from_kind: 'depends_on',
            target: 'DEV-1',
        })
    })
    it('requires an explicit singleton replacement guard', async () => {
        const mutate = vi.fn(async () => {})
        const wrapper = editor({ mutate })
        await drag(wrapper, 'Parent')
        expect(wrapper.get('[role="alertdialog"]').text()).toContain('Replace DEV-2 with DEV-1')
        expect(mutate).not.toHaveBeenCalled()
        await wrapper.get('[role="alertdialog"] button').trigger('click')
        expect(mutate).toHaveBeenCalledWith(expect.objectContaining({ expected_current: 'DEV-2' }))
    })
    it('sends an atomic direction change with focused source and original category unchanged', async () => {
        const wrapper = editor({
            mutate: async (intent: Parameters<typeof relationshipMutation>[1]) => {
                await apiMock.relationshipMutate(relationshipMutation('DEV-8', intent))
            },
        })
        await drag(wrapper, 'Blocks')
        expect(apiMock.relationshipMutate).not.toHaveBeenCalled()
        await wrapper.get('[role="alertdialog"] button').trigger('click')
        await flushPromises()
        expect(apiMock.relationshipMutate).toHaveBeenCalledExactlyOnceWith({
            source: 'DEV-8',
            target: 'DEV-1',
            from_kind: 'depends_on',
            kind: 'blocks',
            operation: 'change_kind',
        })
        wrapper.unmount()
    })
    it('confirms occupied duplicate replacement and sends the shared expected-current guard', async () => {
        const wrapper = editor({
            relationships: { duplicate_of: 'DEV-2' },
            mutate: async (intent: Parameters<typeof relationshipMutation>[1]) => {
                await apiMock.relationshipMutate(relationshipMutation('DEV-8', intent))
            },
        })
        await openAdd(wrapper, 'Duplicate of')
        await wrapper.get('section[aria-label="Duplicate of"] input').setValue('DEV-3')
        await wrapper.get('section[aria-label="Duplicate of"] .relationship-add > button').trigger('click')
        expect(apiMock.relationshipMutate).not.toHaveBeenCalled()
        expect(wrapper.get('[role="alertdialog"]').text()).toContain('Replace DEV-2 with DEV-3')
        await wrapper.get('[role="alertdialog"] button').trigger('click')
        await flushPromises()
        expect(apiMock.relationshipMutate).toHaveBeenCalledExactlyOnceWith({
            source: 'DEV-8',
            target: 'DEV-3',
            kind: 'duplicate_of',
            operation: 'add',
            expected_current: 'DEV-2',
        })
        wrapper.unmount()
    })
    it('isolates internal drags and uses the same guarded move as keyboard controls', async () => {
        const mutate = vi.fn(async () => {})
        const wrapper = editor({ mutate })
        const dataTransfer = { types: [relationshipMime], setData: vi.fn(), effectAllowed: '' }
        await wrapper.get('.relationship-chips li').trigger('dragstart', { dataTransfer })
        expect(dataTransfer.setData).toHaveBeenCalledWith(relationshipMime, expect.any(String))
        expect(dataTransfer.setData).not.toHaveBeenCalledWith('text/plain', expect.anything())
        await wrapper.get('section[aria-label="Fixes"]').trigger('drop', { dataTransfer })
        expect(mutate).toHaveBeenCalledWith({
            action: 'change_kind',
            kind: 'fixes',
            from_kind: 'depends_on',
            target: 'DEV-1',
        })
    })
    it('reports failures and discards pending confirmations after task change or deletion', async () => {
        const wrapper = editor({
            mutate: vi.fn(async () => {
                throw new Error('Stale relationship; reloaded')
            }),
        })
        await wrapper.get('button[aria-label="Remove DEV-1 from Depends on"]').trigger('click')
        await flushPromises()
        expect(wrapper.get('[role="alert"]').text()).toContain('Stale relationship')
        await drag(wrapper, 'Parent')
        await wrapper.setProps({ taskId: 'DEV-9' })
        expect(wrapper.find('[role="alertdialog"]').exists()).toBe(false)
        await wrapper.setProps({ disabled: true })
        expect(wrapper.get('.relationship-add-toggle').attributes('disabled')).toBeDefined()
    })
    it('discards stale suggestions and preserves exact-case IDs', async () => {
        vi.useFakeTimers()
        let resolve!: (value: Array<{ id: string; title: string }>) => void
        apiMock.suggestTasks.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done
                }),
        )
        const mutate = vi.fn(async () => {})
        const wrapper = editor({ mutate })
        await openAdd(wrapper)
        const input = wrapper.get('section[aria-label="Depends on"] input')
        await input.setValue('dev')
        await vi.advanceTimersByTimeAsync(160)
        await wrapper.setProps({ taskId: 'DEV-9' })
        resolve([{ id: 'STALE-1', title: 'Stale' }])
        await flushPromises()
        expect(wrapper.find('[role="listbox"]').exists()).toBe(false)
        expect(wrapper.find('input').exists()).toBe(false)
        await openAdd(wrapper)
        await wrapper.get('input').setValue('dev-1')
        await wrapper.get('section[aria-label="Depends on"] .relationship-add > button').trigger('click')
        expect(mutate).toHaveBeenCalledWith({ action: 'add', kind: 'depends_on', target: 'dev-1' })
    })
})

describe('Seven-field mutation contract', () => {
    it.each(['depends_on', 'blocks', 'related', 'parent', 'children', 'fixes', 'duplicate_of'] as const)(
        'keeps the focused source for %s',
        (kind) => {
            expect(relationshipMutation('DEV-8', { action: 'add', kind, target: 'dev-1' })).toEqual({
                source: 'DEV-8',
                target: 'dev-1',
                kind,
                operation: 'add',
            })
        },
    )
    it('keeps hierarchy direction categories and parent replacement guard verbatim', () => {
        expect(
            relationshipMutation('DEV-8', {
                action: 'change_kind',
                from_kind: 'children',
                kind: 'parent',
                target: 'DEV-1',
                expected_current: 'DEV-2',
            }),
        ).toEqual({
            source: 'DEV-8',
            target: 'DEV-1',
            kind: 'parent',
            from_kind: 'children',
            operation: 'change_kind',
            expected_current: 'DEV-2',
        })
    })
})

describe('Relationship operation draft boundary', () => {
    function setup() {
        let result!: ReturnType<typeof useTaskRelationships>
        const mode = ref<'create' | 'edit'>('create'),
            ready = ref(true),
            scope = ref('DEV-8')
        const execute = vi.fn(async () => {}),
            reconcile = vi.fn(async () => undefined)
        const wrapper = mount(
            defineComponent({
                setup() {
                    result = useTaskRelationships({
                        mode,
                        ready,
                        suppressWatch: ref(false),
                        scope: () => scope.value,
                        taskId: () => scope.value,
                        editable: () => true,
                        execute,
                        reconcile,
                    })
                    return () => null
                },
            }),
        )
        return { result, mode, ready, scope, execute, reconcile, wrapper }
    }
    it('moves only the requested kind, deduplicates exact IDs and builds a create map', async () => {
        const { result, execute, wrapper } = setup()
        await result.mutateRelationship({ action: 'add', kind: 'depends_on', target: 'DEV-1' })
        await result.mutateRelationship({ action: 'add', kind: 'related', target: 'DEV-1' })
        await result.mutateRelationship({ action: 'add', kind: 'depends_on', target: 'dev-1' })
        await result.mutateRelationship({
            action: 'change_kind',
            kind: 'related',
            from_kind: 'depends_on',
            target: 'DEV-1',
        })
        expect(result.buildRelationships()).toMatchObject({ depends_on: ['dev-1'], related: ['DEV-1'] })
        expect(execute).not.toHaveBeenCalled()
        wrapper.unmount()
    })
    it('refuses unconfirmed singleton replacement and preserves the original relationship', async () => {
        const { result, wrapper } = setup()
        await result.mutateRelationship({ action: 'add', kind: 'duplicate_of', target: 'DEV-1' })
        await expect(
            result.mutateRelationship({ action: 'add', kind: 'duplicate_of', target: 'DEV-2' }),
        ).rejects.toThrow('Confirm')
        expect(result.buildRelationships().duplicate_of).toBe('DEV-1')
        await result.mutateRelationship({
            action: 'add',
            kind: 'duplicate_of',
            target: 'DEV-2',
            expected_current: 'DEV-1',
        })
        expect(result.buildRelationships().duplicate_of).toBe('DEV-2')
        wrapper.unmount()
    })
    it('reconciles even a failed operation, without stale-scope effects', async () => {
        const { result, mode, scope, execute, reconcile, wrapper } = setup()
        mode.value = 'edit'
        await nextTick()
        execute.mockRejectedValueOnce(new Error('Conflict'))
        await expect(result.mutateRelationship({ action: 'remove', kind: 'related', target: 'DEV-1' })).rejects.toThrow(
            'Conflict',
        )
        expect(reconcile).toHaveBeenCalledOnce()
        let resolve!: () => void
        execute.mockImplementationOnce(
            () =>
                new Promise<void>((done) => {
                    resolve = done
                }),
        )
        const operation = result.mutateRelationship({ action: 'add', kind: 'related', target: 'DEV-2' })
        scope.value = 'DEV-9'
        await nextTick()
        resolve()
        await operation
        expect(reconcile).toHaveBeenCalledOnce()
        wrapper.unmount()
    })
})
