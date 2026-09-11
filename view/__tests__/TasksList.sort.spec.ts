import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, ref } from 'vue'

const routeState: { query: Record<string, any> } = { query: {} }
const routerPushMock = vi.fn(async () => { })
const routerReplaceMock = vi.fn(async () => { })

const projectsStore = {
    refresh: vi.fn(async () => { }),
}

const _tasksItems = ref<any[]>([])
const _orderIndex = ref<Map<string, number>>(new Map())

const tasksStore = {
    items: _tasksItems,
    orderIndex: _orderIndex,
    count: computed(() => _tasksItems.value.length),
    status: ref('idle' as string),
    error: ref(null as string | null),
    hydrateAll: vi.fn(async (_filter?: Record<string, unknown>) => { }),
    upsert: vi.fn(),
    remove: vi.fn(async () => { }),
}

/** Simulate a completed hydration: items + the authoritative response order. */
function seedHydration(tasks: any[], orderedIds?: string[]) {
    _tasksItems.value = tasks
    const order = orderedIds ?? [...tasks].sort((a, b) => a.id.localeCompare(b.id)).map((t) => t.id)
    const map = new Map<string, number>()
    order.forEach((id, index) => map.set(id, index))
    _orderIndex.value = map
}

const sprintsStore = {
    sprints: ref<any[]>([]),
    active: ref<any[]>([]),
    refresh: vi.fn(async () => { }),
    loading: ref(false),
}

const configStore = {
    statuses: ref<string[]>(['open', 'done']),
    priorities: ref<string[]>(['low', 'med', 'high']),
    types: ref<string[]>(['task']),
    customFields: ref<string[]>([]),
    refresh: vi.fn(async () => { }),
}

vi.mock('vue-router', () => ({
    useRoute: () => routeState,
    useRouter: () => ({
        currentRoute: ref({ path: '/' }),
        push: routerPushMock,
        replace: routerReplaceMock,
    }),
}))

vi.mock('../api/client', () => ({
    api: {
        updateTask: vi.fn(async () => ({})),
        setStatus: vi.fn(async () => ({})),
        deleteTask: vi.fn(async () => ({})),
        exportTasks: vi.fn(async () => new Response('id,title\n', { headers: { 'Content-Type': 'text/csv' } })),
    },
}))

vi.mock('../components/IconGlyph.vue', () => ({
    default: { template: '<span class="icon" />' },
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))


vi.mock('../components/UiButton.vue', () => ({
    default: {
        props: ['variant', 'iconOnly', 'disabled', 'type', 'ariaLabel', 'title'],
        emits: ['click'],
        template: '<button type="button" class="btn" @click="$emit(\'click\', $event)"><slot /></button>',
    },
}))

vi.mock('../components/UiCard.vue', () => ({
    default: { template: '<div class="card"><slot /></div>' },
}))

vi.mock('../components/UiLoader.vue', () => ({
    default: { template: '<div class="loader" />' },
}))

vi.mock('../components/UiEmptyState.vue', () => ({
    default: { template: '<div class="empty" />' },
}))

vi.mock('../components/ReloadButton.vue', () => ({
    default: {
        props: ['disabled', 'loading', 'label', 'title'],
        emits: ['click'],
        template: '<button type="button" class="reload" @click="$emit(\'click\')">Reload</button>',
    },
}))

vi.mock('../components/SmartListChips.vue', () => ({
    default: {
        props: ['statuses', 'priorities', 'value', 'customPresets'],
        emits: ['update:value', 'preset'],
        template: '<div class="chips" />',
    },
}))

vi.mock('../components/FilterBar.vue', () => ({
    default: {
        props: ['statuses', 'priorities', 'types', 'value', 'storageKey'],
        emits: ['update:value'],
        template: '<div class="filter-bar" />',
    },
}))

vi.mock('../components/TaskTable.vue', () => ({
    default: {
        props: ['tasks', 'sort'],
        emits: ['update:sort'],
        template: '<div class="task-table" :data-count="(tasks || []).length" />',
    },
}))

vi.mock('../composables/useProjects', () => ({
    useProjects: () => projectsStore,
}))

vi.mock('../composables/useTaskStore', () => ({
    useTaskStore: () => tasksStore,
}))

vi.mock('../composables/useSprints', () => ({
    useSprints: () => ({
        sprints: sprintsStore.sprints,
        active: sprintsStore.active,
        refresh: sprintsStore.refresh,
        loading: sprintsStore.loading,
    }),
    useSprintFilterOptions: (sprints: any) => ({
        value: (sprints?.value || []).map((s: any) => ({ id: s.id, label: s.display_name || `Sprint ${s.id}` })),
    }),
}))

vi.mock('../composables/useConfig', () => ({
    useConfig: () => ({
        statuses: configStore.statuses,
        priorities: configStore.priorities,
        types: configStore.types,
        customFields: configStore.customFields,
        refresh: configStore.refresh,
    }),
}))

vi.mock('../composables/useActivity', () => ({
    useActivity: () => ({
        add: vi.fn(),
        markTaskTouch: vi.fn(),
        removeTaskTouch: vi.fn(),
        touches: ref({}),
    }),
}))

vi.mock('../composables/useSse', () => ({
    useSse: () => ({
        es: {} as any,
        on: vi.fn(),
        off: vi.fn(),
        close: vi.fn(),
    }),
}))

vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({ openTaskPanel: vi.fn() }),
}))

import TasksList from '../pages/TasksList.vue'
import { normalizeSortOrder } from '../utils/taskSort'
import { api } from '../api/client'

function baseTask(overrides: Partial<any>) {
    return {
        id: 'PRJ-1',
        title: 'Task',
        status: 'open',
        priority: 'med',
        task_type: 'task',
        assignee: 'alice',
        reporter: null,
        effort: null,
        due_date: null,
        created: '2026-01-09T10:00:00Z',
        modified: '2026-01-09T11:00:00Z',
        tags: [],
        relationships: {},
        comments: [],
        custom_fields: {},
        sprints: [],
        references: [],
        history: [],
        ...overrides,
    }
}

function rankTasks(count: number) {
    // Insertion order deliberately reverse-sorted by Rank so only a true
    // global sort can produce the ordered page slices.
    return Array.from({ length: count }, (_, i) =>
        baseTask({
            id: `PRJ-${String(count - i).padStart(2, '0')}`,
            custom_fields: { Rank: String(count - i).padStart(2, '0') },
        }),
    )
}

describe('TasksList server-authoritative sorting', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-09T12:00:00'))
        localStorage.clear()
        localStorage.setItem('lotar.preferences.tasks.pageSize', '25')

        routeState.query = {}
        tasksStore.items.value = []
        _orderIndex.value = new Map()
        tasksStore.status.value = 'idle'
        tasksStore.hydrateAll.mockClear()

        routerPushMock.mockClear()
        routerReplaceMock.mockClear()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    async function settle() {
        await flushPromises()
        await vi.runAllTimersAsync()
        await flushPromises()
    }

    it('forwards sort_by and order to the hydrate filter', async () => {
        routeState.query = { sort_by: 'custom:Rank', order: 'asc' }
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()

        expect(tasksStore.hydrateAll).toHaveBeenCalled()
        const calls = tasksStore.hydrateAll.mock.calls as unknown as Array<[Record<string, unknown>]>
        const lastFilter = calls[calls.length - 1]![0]
        expect(lastFilter.sort_by).toBe('custom:Rank')
        expect(lastFilter.order).toBe('asc')
        wrapper.unmount()
    })

    it('renders the authoritative server order across pages, immune to map-order drift', async () => {
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()
        tasksStore.hydrateAll.mockClear()
        const vm = wrapper.vm as any

        // Server responded with Rank-ascending order; the local map later
        // drifted (e.g. an SSE upsert re-inserted rows). Page slicing must
        // follow the hydration ledger, not the map iteration order.
        const tasks = rankTasks(30)
        const serverOrder = [...tasks].sort(
            (a, b) => Number((a.custom_fields as Record<string, unknown>).Rank) - Number((b.custom_fields as Record<string, unknown>).Rank),
        ).map((t) => t.id)
        seedHydration([...tasks].reverse(), serverOrder)
        vm.filter = { sort_by: 'custom:Rank', order: 'asc' }
        await settle()

        const pageOne = (vm.shownTasks as any[]).map((t) => t.id)
        expect(pageOne).toHaveLength(25)
        expect(pageOne[0]).toBe('PRJ-01')
        expect(pageOne[24]).toBe('PRJ-25')

        vm.pageOffset = 25
        await flushPromises()
        const pageTwo = (vm.shownTasks as any[]).map((t) => t.id)
        expect(pageTwo).toEqual(['PRJ-26', 'PRJ-27', 'PRJ-28', 'PRJ-29', 'PRJ-30'])
        wrapper.unmount()
    })

    it('applies table header sorts to the query and persists them per project', async () => {
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()
        tasksStore.hydrateAll.mockClear()
        const vm = wrapper.vm as any

        vm.onTableSort({ key: 'due_date', dir: 'asc' })
        await settle()

        expect(vm.filter.sort_by).toBe('due')
        expect(vm.filter.order).toBe('asc')
        expect(JSON.parse(localStorage.getItem('lotar.tasks.sort') || 'null')).toEqual({ sort_by: 'due', order: 'asc' })
        const calls = tasksStore.hydrateAll.mock.calls as unknown as Array<[Record<string, unknown>]>
        const lastFilter = calls[calls.length - 1]![0]
        expect(lastFilter.sort_by).toBe('due')
        expect(lastFilter.order).toBe('asc')
        wrapper.unmount()
    })

    it('reloads the saved sort per project without cross-project leakage', async () => {
        localStorage.setItem('lotar.tasks.sort::PA', JSON.stringify({ sort_by: 'status', order: 'asc' }))
        localStorage.setItem('lotar.tasks.sort::PB', JSON.stringify({ sort_by: 'priority', order: 'desc' }))
        routeState.query = { project: 'PA' }
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()
        const vm = wrapper.vm as any

        // Saved sort for the routed project applies on first load.
        expect(vm.filter.sort_by).toBe('status')
        expect(vm.filter.order).toBe('asc')

        // Switching projects reloads that project's saved sort.
        tasksStore.hydrateAll.mockClear()
        vm.filter = { project: 'PB' }
        await settle()
        expect(vm.filter.sort_by).toBe('priority')
        expect(vm.filter.order).toBe('desc')
        const pbCalls = tasksStore.hydrateAll.mock.calls as unknown as Array<[Record<string, unknown>]>
        const pbFilter = pbCalls[pbCalls.length - 1]![0]
        expect(pbFilter.project).toBe('PB')
        expect(pbFilter.sort_by).toBe('priority')

        // A project with no saved sort falls back to the default, not to the
        // previous project's sort.
        vm.filter = { project: 'PC' }
        await settle()
        expect(vm.filter.sort_by).toBeUndefined()
        expect(vm.filter.order).toBe('desc')
        wrapper.unmount()
    })

    it('restores the persisted per-project sort on a plain route reload', async () => {
        localStorage.setItem('lotar.tasks.sort::PA', JSON.stringify({ sort_by: 'priority', order: 'asc' }))
        routeState.query = {}
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()
        const vm = wrapper.vm as any
        // Plain route: the project re-enters the filter the way the real
        // FilterBar does (saved snapshot restore or single-project
        // auto-select). The page's sort storage is the sole sort owner.
        vm.filter = { project: 'PA' }
        await settle()
        expect(vm.filter.sort_by).toBe('priority')
        expect(vm.filter.order).toBe('asc')
        const calls = tasksStore.hydrateAll.mock.calls as unknown as Array<[Record<string, unknown>]>
        expect(calls[calls.length - 1]![0].sort_by).toBe('priority')
        wrapper.unmount()
    })

    it('keeps the persisted sort when a stale FilterBar snapshot carries a different one', async () => {
        localStorage.setItem('lotar.tasks.sort::PA', JSON.stringify({ sort_by: 'priority', order: 'asc' }))
        // Written by an older build, before sort ownership moved to the page.
        localStorage.setItem(
            'lotar.tasks.filter',
            JSON.stringify({ project: 'PA', sort_by: 'status', order: 'desc' }),
        )
        routeState.query = {}
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()
        const vm = wrapper.vm as any
        vm.filter = { project: 'PA' }
        await settle()
        // Single owner: the page sort storage wins over the stale snapshot.
        expect(vm.filter.sort_by).toBe('priority')
        expect(vm.filter.order).toBe('asc')
        wrapper.unmount()
    })

    it('forwards an explicit invalid sort_by to the server and blocks export on error', async () => {
        routeState.query = { sort_by: 'bogus' }
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()
        const vm = wrapper.vm as any
        const calls = tasksStore.hydrateAll.mock.calls as unknown as Array<[Record<string, unknown>]>
        expect(calls[calls.length - 1]![0].sort_by).toBe('bogus')

        // Server rejection surfaces through the store error and disables export.
        tasksStore.error.value = "Invalid sort_by: 'bogus'"
        await flushPromises()
        expect(vm.exportDisabled).toBe(true)
        tasksStore.error.value = null
        await flushPromises()
        expect(vm.exportDisabled).toBe(false)
        wrapper.unmount()
    })

    it('exports the current display query with the active sort', async () => {
        const createObjectURL = vi.fn(() => 'blob:lotar-test')
        const revokeObjectURL = vi.fn()
        const URL_ = URL as unknown as Record<string, unknown>
        const originalCreate = URL_.createObjectURL
        const originalRevoke = URL_.revokeObjectURL
        URL_.createObjectURL = createObjectURL
        URL_.revokeObjectURL = revokeObjectURL
        const exportTasks = api.exportTasks as ReturnType<typeof vi.fn>
        exportTasks.mockClear()

        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        try {
            await settle()
            const vm = wrapper.vm as any
            vm.filter = { project: 'PA', sort_by: 'custom:Rank', order: 'asc' }
            await settle()

            expect(vm.exportDisabled).toBe(false)
            await vm.exportCsv()
            expect(exportTasks).toHaveBeenCalledTimes(1)
            const exported = exportTasks.mock.calls[0]![0] as Record<string, any>
            expect(exported.project).toBe('PA')
            expect(exported.sort_by).toBe('custom:Rank')
            expect(exported.order).toBe('asc')
            expect(createObjectURL).toHaveBeenCalled()
        } finally {
            URL_.createObjectURL = originalCreate
            URL_.revokeObjectURL = originalRevoke
            wrapper.unmount()
        }
    })

    it('clearing filters resets the saved sort to the default', async () => {
        localStorage.setItem('lotar.tasks.sort::PA', JSON.stringify({ sort_by: 'status', order: 'asc' }))
        routeState.query = { project: 'PA' }
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()
        const vm = wrapper.vm as any
        expect(vm.filter.sort_by).toBe('status')

        vm.resetFilters()
        await settle()
        expect(vm.filter.sort_by).toBeUndefined()
        // The mocked FilterBar does not re-emit its default; the effective
        // order still resolves to desc.
        expect(normalizeSortOrder(vm.filter.order)).toBe('desc')
        expect(localStorage.getItem('lotar.tasks.sort::PA')).toBeNull()
        wrapper.unmount()
    })
})
