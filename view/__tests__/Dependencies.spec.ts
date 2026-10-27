import { flushPromises, mount } from '@vue/test-utils'
import { reactive } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Dependencies from '../pages/Dependencies.vue'
import DependencyGraph from '../components/DependencyGraph.vue'
import FilterBar from '../components/FilterBar.vue'
import ColumnsMenu from '../components/ColumnsMenu.vue'

const mocks = vi.hoisted(() => ({
    check: vi.fn(),
    repair: vi.fn(),
    suggest: vi.fn(),
    replace: vi.fn(),
    refreshGraph: vi.fn(),
    refreshConfig: vi.fn(),
    on: vi.fn(),
    close: vi.fn(),
    reconnect: undefined as (() => void) | undefined,
    route: { query: {} as Record<string, string> },
}))
vi.mock('../api/client', () => ({
    api: { relationshipCheck: mocks.check, relationshipRepair: mocks.repair, suggestTasks: mocks.suggest },
}))
vi.mock('../composables/useConfig', async () => {
    const { ref } = await import('vue')
    return {
        useConfig: () => ({
            statuses: ref(['Todo', 'Done']),
            priorities: ref(['Low', 'High']),
            types: ref(['Feature', 'Bug']),
            customFields: ref(['team']),
            tags: ref(['ui']),
            members: ref(['alex']),
            scope: ref('DEV'),
            refresh: mocks.refreshConfig,
        }),
    }
})
vi.mock('../composables/useSprints', async () => {
    const { ref } = await import('vue')
    return {
        useSprints: () => ({ sprints: ref([]), refresh: vi.fn(async () => {}) }),
        useSprintFilterOptions: () => ref([]),
    }
})
vi.mock('../composables/useProjects', async () => {
    const { ref } = await import('vue')
    return {
        useProjects: () => ({
            projects: ref([
                { prefix: 'DEV', name: 'Development' },
                { prefix: 'OTHER', name: 'Other' },
            ]),
            refresh: vi.fn(async () => {}),
        }),
    }
})
vi.mock('vue-router', async (importOriginal) => ({
    ...(await importOriginal<typeof import('vue-router')>()),
    useRoute: () => mocks.route,
    useRouter: () => ({ replace: mocks.replace }),
}))
vi.mock('../composables/useSse', () => ({
    useSse: (_path: string, _params: unknown, options: { onReconnect: () => void }) => {
        mocks.reconnect = options.onReconnect
        return { on: mocks.on, close: mocks.close }
    },
}))

const mounted: Array<{ wrapper: ReturnType<typeof mount>; host: HTMLElement }> = []
function create() {
    const host = document.createElement('div')
    document.body.appendChild(host)
    const wrapper = mount(Dependencies, {
        attachTo: host,
        global: {
            stubs: {
                DependencyGraph: {
                    props: [
                        'root',
                        'project',
                        'depth',
                        'relationshipKinds',
                        'showIsolated',
                        'showLayerControls',
                        'filters',
                        'sortBy',
                        'order',
                        'fields',
                        'fixBusy',
                    ],
                    methods: {
                        refresh() {
                            mocks.refreshGraph()
                        },
                    },
                    template: '<div data-testid="graph" />',
                },
            },
        },
    })
    mounted.push({ wrapper, host })
    return wrapper
}
const button = (wrapper: ReturnType<typeof create>, text: string) =>
    wrapper.findAll('button').find((entry) => entry.text() === text)!
const search = (wrapper: ReturnType<typeof create>) => wrapper.get('[data-testid="filter-search"]')
async function showFilters(wrapper: ReturnType<typeof create>) {
    await wrapper.get('[data-testid="filter-toggle"]').trigger('click')
}

beforeEach(() => {
    vi.clearAllMocks()
    mocks.replace.mockReset()
    localStorage.clear()
    mocks.route = reactive({ query: { root: 'DEV-8', project: 'DEV', layers: 'hierarchy', depth: '2' } })
    mocks.replace.mockImplementation(async ({ query }) => {
        const normalized = Object.fromEntries(
            Object.entries(query)
                .filter(([, value]) => value !== undefined)
                .map(([key, value]) => [key, String(value)]),
        )
        if (JSON.stringify(normalized) !== JSON.stringify(mocks.route.query)) mocks.route.query = normalized
    })
    mocks.refreshConfig.mockResolvedValue(undefined)
    mocks.suggest.mockResolvedValue([])
    mocks.check.mockResolvedValue({ issues: [], complete: true, revision: 'r1' })
    mocks.repair.mockResolvedValue({
        applied: false,
        changed_tasks: ['DEV-8'],
        revision: 'r1',
        issues: [{ code: 'missing_mirror', message: 'Review reciprocal', repairable: true }],
    })
    vi.stubGlobal('EventSource', class {})
})
afterEach(() => {
    for (const { wrapper, host } of mounted.splice(0).reverse()) {
        wrapper.unmount()
        host.remove()
    }
    vi.useRealTimers()
    vi.unstubAllGlobals()
})

describe('Unified dependency search and view controls', () => {
    it('makes isolated-task visibility explicit, URL-backed and separate from task filters', async () => {
        const wrapper = create()
        await flushPromises()
        expect(wrapper.find('button[aria-label="Show unlinked tasks"]').exists()).toBe(false)
        await showFilters(wrapper)
        const option = wrapper.get('[data-testid="filter-panel"] button[aria-label="Show unlinked tasks"]')
        expect(option.attributes('aria-pressed')).toBe('false')
        expect(wrapper.getComponent(DependencyGraph).props('showIsolated')).toBe(false)
        await option.trigger('click')
        await flushPromises()
        expect(mocks.route.query.isolated).toBe('1')
        expect(wrapper.getComponent(DependencyGraph).props('showIsolated')).toBe(true)
        expect(option.attributes('aria-pressed')).toBe('true')
        expect(wrapper.getComponent(DependencyGraph).props('filters')).not.toHaveProperty('isolated')
        await option.trigger('click')
        await flushPromises()
        expect(mocks.route.query).not.toHaveProperty('isolated')
        expect(wrapper.getComponent(DependencyGraph).props('showIsolated')).toBe(false)
        expect(option.attributes('aria-pressed')).toBe('false')
    })
    it('restores isolated-task visibility when returning to a shared view URL', async () => {
        mocks.route.query.isolated = '1'
        const wrapper = create()
        await flushPromises()
        expect(wrapper.getComponent(DependencyGraph).props('showIsolated')).toBe(true)
    })
    it('uses one shared search and the common filter panel extension', async () => {
        const wrapper = create()
        await flushPromises()
        expect(wrapper.findAll('input[type="text"]')).toHaveLength(1)
        expect(wrapper.find('#dependency-root-task').exists()).toBe(false)
        expect(wrapper.getComponent(FilterBar).props('showOrder')).toBe(true)
        expect((search(wrapper).element as HTMLInputElement).value).toBe('DEV-8')
        await showFilters(wrapper)
        expect(wrapper.get('fieldset').get('legend').text()).toBe('Relationship types')
        expect(wrapper.findAll('fieldset input[type="checkbox"]')).toHaveLength(6)
        expect(wrapper.findAll('button').some((entry) => entry.text() === 'Explore')).toBe(false)
    })
    it('enables Clear filters for a root-only link and clears its task scope', async () => {
        mocks.route.query = { root: 'DEV-8', project: 'DEV' }
        const wrapper = create()
        await flushPromises()
        const clear = wrapper.get('button[aria-label="Clear filters"]')
        expect(clear.attributes('disabled')).toBeUndefined()
        await clear.trigger('click')
        await flushPromises()
        expect(mocks.route.query).not.toHaveProperty('root')
        expect((search(wrapper).element as HTMLInputElement).value).toBe('')
    })

    it('keeps the input, focus, draft and suggestions through live search and route echoes', async () => {
        vi.useFakeTimers()
        mocks.suggest.mockResolvedValue([{ id: 'DEV-54', title: 'Transaction planning' }])
        const wrapper = create()
        await flushPromises()
        const input = search(wrapper)
        ;(input.element as HTMLInputElement).focus()
        await input.setValue('transaction')
        await vi.advanceTimersByTimeAsync(600)
        await flushPromises()
        expect(search(wrapper).element).toBe(input.element)
        expect(document.activeElement).toBe(input.element)
        expect((input.element as HTMLInputElement).value).toBe('transaction')
        expect(wrapper.get('[role="listbox"]').text()).toContain('Transaction planning')
        expect(mocks.route.query.q).toBe('transaction')
        expect(mocks.route.query).not.toHaveProperty('root')
        expect(wrapper.getComponent(DependencyGraph).props('filters')).toMatchObject({ q: 'transaction' })
    })
    it('restores remembered search into the actual graph filters, not only the input', async () => {
        mocks.route.query = {}
        localStorage.setItem(
            'lotar.dependencies.filter',
            JSON.stringify({ q: 'cache', status: 'Todo', project: 'DEV' }),
        )
        const wrapper = create()
        await flushPromises()
        expect((search(wrapper).element as HTMLInputElement).value).toBe('cache')
        expect(wrapper.getComponent(DependencyGraph).props()).toMatchObject({
            project: 'DEV',
            filters: { deletion: 'active', q: 'cache', status: 'Todo' },
        })
        expect(mocks.route.query).toMatchObject({ q: 'cache', status: 'Todo', project: 'DEV' })
    })

    it('selects a suggested task explicitly and commits typed conditions before rooting its graph', async () => {
        vi.useFakeTimers()
        mocks.suggest.mockResolvedValue([{ id: 'DEV-54', title: 'Transaction planning' }])
        const wrapper = create()
        await flushPromises()
        const input = search(wrapper)
        ;(input.element as HTMLInputElement).focus()
        await input.setValue('status:Todo transaction')
        await vi.advanceTimersByTimeAsync(200)
        await flushPromises()
        await wrapper
            .findAll('[role="option"]')
            .find((entry) => entry.text().includes('DEV-54'))!
            .trigger('mousedown')
        await flushPromises()
        expect(mocks.route.query).toMatchObject({ root: 'DEV-54', q: 'DEV-54', status: 'Todo' })
        expect(wrapper.getComponent(DependencyGraph).props()).toMatchObject({
            root: 'DEV-54',
            filters: { deletion: 'active', status: 'Todo' },
        })
        expect(wrapper.getComponent(DependencyGraph).props('filters')).not.toHaveProperty('q')
        expect(search(wrapper).element).toBe(input.element)
    })

    it('does not focus a typed ID on idle but accepts exact-ID Enter without provider results', async () => {
        vi.useFakeTimers()
        const wrapper = create()
        await flushPromises()
        const input = search(wrapper)
        ;(input.element as HTMLInputElement).focus()
        await input.setValue('DEV-54')
        await vi.advanceTimersByTimeAsync(600)
        await flushPromises()
        expect(mocks.route.query).not.toHaveProperty('root')
        await input.trigger('keydown', { key: 'Enter' })
        await flushPromises()
        expect(mocks.route.query).toMatchObject({ root: 'DEV-54', q: 'DEV-54' })
    })

    it('completes task-type filters in the same search without task lookup taking over', async () => {
        vi.useFakeTimers()
        const wrapper = create()
        await flushPromises()
        const input = search(wrapper)
        ;(input.element as HTMLInputElement).focus()
        mocks.suggest.mockClear()
        await input.setValue('type:Bu')
        await vi.advanceTimersByTimeAsync(200)
        await flushPromises()
        const option = wrapper.findAll('[role="option"]').find((entry) => entry.text().includes('Bug'))!
        expect(option).toBeDefined()
        await option.trigger('mousedown')
        await flushPromises()
        expect(mocks.route.query.type).toBe('Bug')
        expect(wrapper.getComponent(DependencyGraph).props('filters')).toMatchObject({ type: 'Bug' })
        expect(mocks.suggest).not.toHaveBeenCalled()
    })

    it('preserves the graph component when task roots and project filters change', async () => {
        const wrapper = create()
        await flushPromises()
        const graph = wrapper.getComponent(DependencyGraph).element
        wrapper.getComponent(FilterBar).vm.$emit('task-selected', 'DEV-54')
        await flushPromises()
        expect(wrapper.getComponent(DependencyGraph).element).toBe(graph)
        wrapper.getComponent(FilterBar).vm.$emit('update:value', { project: 'OTHER', q: 'DEV-54' })
        await flushPromises()
        expect(wrapper.getComponent(DependencyGraph).element).toBe(graph)
        expect(mocks.route.query.project).toBe('OTHER')
        expect(mocks.route.query).not.toHaveProperty('root')
        expect(mocks.route.query).not.toHaveProperty('q')
    })
    it('keeps ordinary search text when switching projects without a selected task', async () => {
        mocks.route.query = { project: 'DEV', q: 'cache' }
        const wrapper = create()
        await flushPromises()
        wrapper.getComponent(FilterBar).vm.$emit('update:value', { project: 'OTHER', q: 'cache' })
        await flushPromises()
        expect(mocks.route.query).toMatchObject({ project: 'OTHER', q: 'cache' })
        expect(mocks.route.query).not.toHaveProperty('root')
    })

    it('allows independent kinds, including hierarchy without dependencies and explicitly no relationships', async () => {
        const wrapper = create()
        await flushPromises()
        await showFilters(wrapper)
        const group = wrapper.get('fieldset')
        const labels = group.findAll('label')
        const blocking = labels.find((entry) => entry.text() === 'Blocking dependencies')!.get('input')
        if ((blocking.element as HTMLInputElement).checked) await blocking.setValue(false)
        await flushPromises()
        expect(wrapper.getComponent(DependencyGraph).props('relationshipKinds')).toEqual(['hierarchy'])
        await labels
            .find((entry) => entry.text() === 'Related tasks')!
            .get('input')
            .setValue(true)
        await flushPromises()
        expect(wrapper.getComponent(DependencyGraph).props('relationshipKinds')).toEqual(['hierarchy', 'related'])
        await labels
            .find((entry) => entry.text() === 'Parent / child')!
            .get('input')
            .setValue(false)
        await labels
            .find((entry) => entry.text() === 'Related tasks')!
            .get('input')
            .setValue(false)
        await flushPromises()
        expect(wrapper.getComponent(DependencyGraph).props('relationshipKinds')).toEqual([])
        expect(mocks.route.query.relationships).toBe('none')
    })
    it('selects and clears All relationship types and reports a partial selection', async () => {
        const wrapper = create()
        await flushPromises()
        await showFilters(wrapper)
        const all = wrapper.get('input[aria-label="All relationship types"]')
        expect((all.element as HTMLInputElement).indeterminate).toBe(true)
        await all.setValue(true)
        await flushPromises()
        expect(wrapper.getComponent(DependencyGraph).props('relationshipKinds')).toHaveLength(5)
        expect((all.element as HTMLInputElement).checked).toBe(true)
        expect((all.element as HTMLInputElement).indeterminate).toBe(false)
        await all.setValue(false)
        await flushPromises()
        expect(wrapper.getComponent(DependencyGraph).props('relationshipKinds')).toEqual([])
        expect(mocks.route.query.relationships).toBe('none')
    })

    it('keeps task-type filters distinct from kinds and overlay navigation metadata', async () => {
        mocks.route.query = {
            root: 'DEV-8',
            project: 'DEV',
            relationships: 'related',
            type: 'Bug',
            taskPanel: 'DEV-2',
            sort_by: 'title',
            order: 'asc',
        }
        const wrapper = create()
        await flushPromises()
        const props = wrapper.getComponent(DependencyGraph).props()
        expect(props).toMatchObject({
            relationshipKinds: ['related'],
            filters: { deletion: 'active', type: 'Bug' },
            sortBy: 'title',
            order: 'asc',
        })
        expect(props.filters).not.toHaveProperty('relationships')
        expect(props.filters).not.toHaveProperty('taskPanel')
        wrapper.getComponent(FilterBar).vm.$emit('update:value', { project: 'DEV', type: 'Feature', q: 'DEV-8' })
        await flushPromises()
        expect(mocks.route.query).toMatchObject({ taskPanel: 'DEV-2', root: 'DEV-8', type: 'Feature' })
    })

    it.each(['related', 'hierarchy', 'fixes', 'duplicate', 'all'])(
        'keeps the previous %s layer links readable',
        async (type) => {
            mocks.route.query = { project: 'DEV', layers: type }
            const wrapper = create()
            await flushPromises()
            const kinds = wrapper.getComponent(DependencyGraph).props('relationshipKinds') as string[]
            expect(kinds).toContain('dependency')
            if (type === 'all') expect(kinds).toHaveLength(5)
            else expect(kinds).toContain(type)
        },
    )

    it('applies per-project Fields choices to visible cards and retains saved values', async () => {
        const wrapper = create()
        await flushPromises()
        expect(wrapper.getComponent(ColumnsMenu).props('label')).toBe('Card fields')
        wrapper.getComponent(ColumnsMenu).props('setVisible')('reporter', {
            target: { checked: false },
        } as unknown as Event)
        await flushPromises()
        expect(wrapper.getComponent(DependencyGraph).props('fields')).toMatchObject({ reporter: false })
        wrapper.unmount()
        const reloaded = create()
        await flushPromises()
        expect(reloaded.getComponent(DependencyGraph).props('fields')).toMatchObject({ reporter: false })
        expect(wrapper.find('[aria-label="Configure hover fields"]').exists()).toBe(false)
    })
})

describe('Full-scope dependency validation and safe repair', () => {
    it('previews and confirms only the problem selected on a graph card', async () => {
        const wrapper = create()
        await flushPromises()
        const issue = {
            id: 'mirror-key',
            code: 'missing_mirror',
            severity: 'warning',
            message: 'Missing reciprocal link between DEV-8 and DEV-9',
            tasks: ['DEV-8', 'DEV-9'],
            repairable: true,
        }
        wrapper.getComponent(DependencyGraph).vm.$emit('fix', issue)
        await flushPromises()
        expect(mocks.repair).toHaveBeenCalledExactlyOnceWith({
            project: 'DEV',
            apply: false,
            issue_ids: ['mirror-key'],
        })
        expect(wrapper.get('[aria-label="Safe repair preview"]').text()).toContain('Only this problem is included')
        expect(wrapper.get('[aria-label="Safe repair preview"]').text()).toContain(issue.message)
        mocks.repair.mockResolvedValueOnce({ applied: true, changed_tasks: ['DEV-9'], revision: 'new', issues: [] })
        await button(wrapper, 'Confirm and apply safe repair').trigger('click')
        await flushPromises()
        expect(mocks.repair).toHaveBeenLastCalledWith({
            project: 'DEV',
            apply: true,
            revision: 'r1',
            issue_ids: ['mirror-key'],
        })
    })
    it('does not turn an unsafe or identifier-less problem into a batch repair', async () => {
        const wrapper = create()
        await flushPromises()
        wrapper
            .getComponent(DependencyGraph)
            .vm.$emit('fix', { code: 'cycle', message: 'Choose an edge', tasks: ['DEV-8'], repairable: false })
        await flushPromises()
        expect(mocks.repair).not.toHaveBeenCalled()
        expect(wrapper.get('[role="alert"]').text()).toContain('manual review')
        await button(wrapper, 'Fix relationships').trigger('click')
        await flushPromises()
        expect(mocks.repair).toHaveBeenCalledExactlyOnceWith({ project: 'DEV', apply: false })
        expect(wrapper.get('[aria-label="Safe repair preview"]').text()).not.toContain('Only this problem is included')
    })
    it('previews without mutation and applies only the explicit current revision without graph remount', async () => {
        const wrapper = create()
        await flushPromises()
        const graph = wrapper.getComponent(DependencyGraph).element
        // Page actions sit in the filter row; the title row holds view options (DEV-186).
        expect(wrapper.get('.filter-card').text()).toContain('Fix relationships')
        expect(wrapper.get('header').text()).not.toContain('Fix relationships')
        expect(mocks.repair).not.toHaveBeenCalled()
        await button(wrapper, 'Fix relationships').trigger('click')
        await flushPromises()
        expect(mocks.repair).toHaveBeenCalledExactlyOnceWith({ project: 'DEV', apply: false })
        expect(wrapper.text()).toContain('Review reciprocal')
        mocks.repair.mockResolvedValueOnce({ applied: true, changed_tasks: ['DEV-8'], revision: 'r2', issues: [] })
        await button(wrapper, 'Confirm and apply safe repair').trigger('click')
        await flushPromises()
        expect(mocks.repair).toHaveBeenLastCalledWith({ project: 'DEV', apply: true, revision: 'r1' })
        expect(wrapper.text()).toContain('Safe repair applied to 1 task.')
        expect(wrapper.getComponent(DependencyGraph).element).toBe(graph)
        expect(mocks.refreshGraph).toHaveBeenCalledOnce()
    })

    it('drops a pending preview and unsafe confirmation on project change', async () => {
        let resolve!: (value: unknown) => void
        mocks.repair.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done
                }),
        )
        const wrapper = create()
        await flushPromises()
        await button(wrapper, 'Fix relationships').trigger('click')
        mocks.route.query = { project: 'OTHER', root: 'OTHER-1' }
        await flushPromises()
        resolve({ applied: false, changed_tasks: ['DEV-8'], revision: 'old', issues: [] })
        await flushPromises()
        expect(wrapper.find('[aria-label="Safe repair preview"]').exists()).toBe(false)
        expect(wrapper.findAll('button').some((entry) => entry.text() === 'Confirm and apply safe repair')).toBe(false)
    })

    it('discards previews after task updates and reconnect, with owned subscription cleanup', async () => {
        const wrapper = create()
        await flushPromises()
        await button(wrapper, 'Fix relationships').trigger('click')
        await flushPromises()
        mocks.on.mock.calls.find(([event]) => event === 'task_updated')![1]({ data: '{}' })
        await flushPromises()
        expect(wrapper.find('[aria-label="Safe repair preview"]').exists()).toBe(false)
        expect(wrapper.text()).toContain('Task data changed')
        await button(wrapper, 'Fix relationships').trigger('click')
        await flushPromises()
        mocks.reconnect?.()
        await flushPromises()
        expect(wrapper.find('[aria-label="Safe repair preview"]').exists()).toBe(false)
        wrapper.unmount()
        expect(mocks.close).toHaveBeenCalledOnce()
    })

    it('reports partial full-project validation rather than claiming a clean graph', async () => {
        mocks.check.mockResolvedValueOnce({ issues: [], complete: false, revision: 'r1' })
        const wrapper = create()
        await flushPromises()
        await button(wrapper, 'Validate project').trigger('click')
        await flushPromises()
        expect(mocks.check).toHaveBeenCalledExactlyOnceWith('DEV')
        expect(wrapper.text()).toContain('Validation is partial')
        expect(wrapper.text()).toContain('all tasks in project Development')
        expect(wrapper.text()).toContain('validation is incomplete')
    })

    it('requires a new preview after apply rejection and disables empty repair plans', async () => {
        const wrapper = create()
        await flushPromises()
        await button(wrapper, 'Fix relationships').trigger('click')
        await flushPromises()
        mocks.repair.mockRejectedValueOnce(new Error('Stale repair revision'))
        await button(wrapper, 'Confirm and apply safe repair').trigger('click')
        await flushPromises()
        expect(wrapper.get('[role="alert"]').text()).toContain('Stale repair revision')
        expect(wrapper.find('[aria-label="Safe repair preview"]').exists()).toBe(false)
        mocks.repair.mockResolvedValueOnce({ applied: false, changed_tasks: [], revision: 'r3', issues: [] })
        await button(wrapper, 'Fix relationships').trigger('click')
        await flushPromises()
        expect(button(wrapper, 'Confirm and apply safe repair').attributes('disabled')).toBeDefined()
    })

    it('retains an apply receipt through its own task events without permitting duplicate operations', async () => {
        const wrapper = create()
        await flushPromises()
        await button(wrapper, 'Fix relationships').trigger('click')
        await flushPromises()
        let resolve!: (value: unknown) => void
        mocks.repair.mockImplementationOnce(
            () =>
                new Promise((done) => {
                    resolve = done
                }),
        )
        await button(wrapper, 'Confirm and apply safe repair').trigger('click')
        mocks.on.mock.calls.find(([event]) => event === 'task_updated')![1]({ data: '{}' })
        await flushPromises()
        expect(button(wrapper, 'Fix relationships').attributes('disabled')).toBeDefined()
        resolve({ applied: true, changed_tasks: ['DEV-8'], revision: 'r2', issues: [] })
        await flushPromises()
        expect(wrapper.get('[role="status"]').text()).toBe('Safe repair applied to 1 task.')
    })
})
