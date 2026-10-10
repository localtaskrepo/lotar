import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, nextTick, ref, type ComputedRef, type Ref } from 'vue'

const routeState: { query: Record<string, any> } = { query: {} }
const routerPushMock = vi.fn(async () => { })
const routerReplaceMock = vi.fn(async () => { })

const projectsStore = {
    refresh: vi.fn(async () => { }),
}

interface MockHandle {
    key: string
    items: Ref<any[]>
    status: Ref<string>
    error: Ref<string | null>
    hasSnapshot: Ref<boolean>
    ranks: Ref<Map<string, number>>
    total: ComputedRef<number>
    ids: ComputedRef<string[]>
    tasks: ComputedRef<any[]>
    lastSyncAt: Ref<number>
    refresh: ReturnType<typeof vi.fn>
    retain: ReturnType<typeof vi.fn>
    release: ReturnType<typeof vi.fn>
}

const handles = new Map<string, MockHandle>()

function createHandle(key: string): MockHandle {
    const items = ref<any[]>([])
    const status = ref('idle')
    const error = ref<string | null>(null)
    const hasSnapshot = ref(false)
    const ranks = ref<Map<string, number>>(new Map())
    const handle: MockHandle = {
        key,
        items,
        status,
        error,
        hasSnapshot,
        ranks,
        total: computed(() => items.value.length),
        ids: computed(() => items.value.map((t: any) => t.id)),
        tasks: computed(() => items.value),
        lastSyncAt: ref(1),
        refresh: vi.fn(async () => {
            status.value = 'loading'
            error.value = null
            status.value = 'ready'
            hasSnapshot.value = true
        }),
        retain: vi.fn(),
        release: vi.fn(),
    }
    return handle
}

function handleKeyOf(filter: Record<string, unknown> = {}): string {
    // buildServerFilter always materializes `order` (default 'desc'); mirror
    // that and canonicalize key order so test-side handleFor({}) matches the
    // page's serverFilter regardless of property insertion order.
    const source: Record<string, unknown> = { order: 'desc', ...(filter ?? {}) }
    const entries = Object.entries(source)
        .filter(([k, v]) => !['limit', 'offset', 'page'].includes(k) && v !== undefined && v !== null && v !== '')
        .sort(([a], [b]) => a.localeCompare(b))
    return JSON.stringify(entries)
}

function handleFor(filter: Record<string, unknown> = {}): MockHandle {
    const key = handleKeyOf(filter)
    let handle = handles.get(key)
    if (!handle) {
        handle = createHandle(key)
        handles.set(key, handle)
    }
    return handle
}

const tasksStore = {
    getQuery: vi.fn((filter?: Record<string, unknown>) => handleFor(filter)),
    upsert: vi.fn(),
    remove: vi.fn(async () => { }),
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

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))

vi.mock('../components/IconGlyph.vue', () => ({
    default: { template: '<span class="icon" />' },
}))

vi.mock('../components/UiButton.vue', () => ({
    default: {
        props: ['variant', 'iconOnly', 'disabled', 'type', 'ariaLabel', 'title'],
        emits: ['click'],
        template: '<button type="button" class="btn" @click="$emit(\'click\', $event)"><slot /></button>',
    },
}))

vi.mock('../components/UiLoader.vue', () => ({
    default: { template: '<div class="loader" />' },
}))

vi.mock('../components/UiEmptyState.vue', () => ({
    default: { template: '<div class="empty" />' },
}))

vi.mock('../components/UiModal.vue', () => ({
    default: { template: '<div class="modal"><slot /></div>' },
}))

vi.mock('../components/ReloadButton.vue', () => ({
    default: {
        props: ['disabled', 'loading', 'label', 'title'],
        emits: ['click'],
        template: '<button type="button" class="reload" @click="$emit(\'click\')">Reload</button>',
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
        props: ['tasks', 'loading'],
        template: '<div class="task-table" :data-count="(tasks || []).length" :data-loading="loading ? \'1\' : \'0\'" />',
    },
}))

vi.mock('../components/ColumnsMenu.vue', () => ({
    default: {
        props: ['open', 'options', 'isVisible', 'setVisible', 'label'],
        emits: ['update:open', 'reset'],
        template: '<div class="columns-menu"><slot name="trigger" /></div>',
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
    useSprintFilterOptions: () => ({ value: [] }),
}))

vi.mock('../composables/useConfig', () => ({
    useConfig: () => ({
        scope: ref(''), tags: ref<string[]>([]), members: ref<string[]>([]),
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

async function settle() {
    await flushPromises()
    await vi.runAllTimersAsync()
    await flushPromises()
    await nextTick()
}

describe('TasksList keyed query presentation (DEV-65)', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-09T12:00:00'))
        localStorage.clear()

        routeState.query = {}
        handles.clear()
        tasksStore.getQuery.mockClear()
        tasksStore.upsert.mockClear()
        tasksStore.remove.mockClear()
        routerPushMock.mockClear()
        routerReplaceMock.mockClear()
        projectsStore.refresh.mockClear()
        configStore.refresh.mockClear()
        sprintsStore.refresh.mockClear()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('a pending new query shows a loader, never the previous key rows, and disables actions', async () => {
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()

        // The initial query completed with rows for the previous filter.
        const initial = handleFor({})
        initial.items.value = [baseTask({ id: 'A-1' }), baseTask({ id: 'A-2' })]
        await settle()
        expect((wrapper.vm as any).shownTasks.map((t: any) => t.id)).toEqual(['A-1', 'A-2'])

        // Switch to a NEW query (distinct handle) whose refresh is QUEUED
        // behind a prior hydration: it has not started yet (status stays
        // idle, no snapshot).
        let finishB!: () => void
        const projectB = handleFor({ project: 'B' })
        expect(projectB).not.toBe(initial)
        projectB.refresh.mockImplementationOnce(
            () => new Promise<void>((resolve) => {
                finishB = () => {
                    projectB.items.value = []
                    projectB.hasSnapshot.value = true
                    projectB.status.value = 'ready'
                    resolve()
                }
            }),
        )
        const vm = wrapper.vm as any
        vm.filter = { project: 'B' }
        await vi.advanceTimersByTimeAsync(200)
        await flushPromises()
        await nextTick()

        // Queued phase: explicit pending loader; the previous key's rows are
        // NOT rendered under the new filter; no false empty state.
        expect(vm.loading).toBe(true)
        expect(vm.shownTasks).toEqual([])
        expect(wrapper.find('.loader').exists()).toBe(true)
        expect(wrapper.find('.task-table').exists()).toBe(false)
        expect(wrapper.find('.empty').exists()).toBe(false)

        // Actions are inaccessible while the new query is pending.
        expect(vm.selectionReady).toBe(false)
        expect(vm.exportDisabled).toBe(true)
        expect(vm.disableBulkActions).toBe(true)

        // Started phase (refresh in flight): same pending presentation.
        projectB.status.value = 'loading'
        projectB.error.value = null
        await flushPromises()
        await nextTick()
        expect(vm.loading).toBe(true)
        expect(vm.shownTasks).toEqual([])
        expect(wrapper.find('.loader').exists()).toBe(true)
        expect(wrapper.find('.task-table').exists()).toBe(false)

        finishB()
        await settle()
        expect(vm.loading).toBe(false)
        expect(vm.shownTasks).toEqual([])
        // "No tasks match" appears only after an actual completed zero result.
        expect(wrapper.find('.empty').exists()).toBe(true)
        expect(wrapper.find('.loader').exists()).toBe(false)
        wrapper.unmount()
    })

    it('renders the new query rows once its refresh publishes', async () => {
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()

        let finishB!: () => void
        const projectB = handleFor({ project: 'B' })
        projectB.refresh.mockImplementationOnce(() => {
            projectB.status.value = 'loading'
            return new Promise<void>((resolve) => {
                finishB = () => {
                    projectB.items.value = [baseTask({ id: 'B-1' })]
                    projectB.hasSnapshot.value = true
                    projectB.status.value = 'ready'
                    resolve()
                }
            })
        })
        const vm = wrapper.vm as any
        vm.filter = { project: 'B' }
        await vi.advanceTimersByTimeAsync(200)
        await flushPromises()
        await nextTick()
        expect(vm.loading).toBe(true)

        finishB()
        await settle()
        expect(vm.loading).toBe(false)
        expect(vm.shownTasks.map((t: any) => t.id)).toEqual(['B-1'])
        expect(wrapper.find('.task-table').exists()).toBe(true)
        wrapper.unmount()
    })

    it('a failed NEW key shows the error state (no loader, no previous key rows)', async () => {
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()

        const initial = handleFor({})
        initial.items.value = [baseTask({ id: 'A-1' })]
        await settle()

        const projectC = handleFor({ project: 'C' })
        projectC.refresh.mockImplementationOnce(async () => {
            projectC.status.value = 'loading'
            await Promise.resolve()
            projectC.status.value = 'error'
            projectC.error.value = 'C is down'
        })
        const vm = wrapper.vm as any
        vm.filter = { project: 'C' }
        await settle()

        expect(vm.error).toBe('C is down')
        expect(vm.shownTasks).toEqual([])
        expect(wrapper.find('.empty').exists()).toBe(true)
        expect(wrapper.find('.task-table').exists()).toBe(false)
        expect(wrapper.find('.loader').exists()).toBe(false)
        // No retry banner either: there are no valid rows to retain.
        expect(wrapper.find('.refresh-error').exists()).toBe(false)
        wrapper.unmount()
    })

    it('a same-key refresh failure retains rows, shows a retry banner, and recovers on retry', async () => {
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()

        const initial = handleFor({})
        initial.items.value = [baseTask({ id: 'A-1' })]
        await settle()

        initial.refresh.mockImplementationOnce(async () => {
            initial.status.value = 'loading'
            await Promise.resolve()
            initial.status.value = 'error'
            initial.error.value = 'transient boom'
        })
        const vm = wrapper.vm as any
        await vm.retry()
        await flushPromises()
        await nextTick()

        expect(vm.error).toBe('transient boom')
        expect(vm.shownTasks.map((t: any) => t.id)).toEqual(['A-1'])
        expect(wrapper.find('.refresh-error').exists()).toBe(true)
        expect(wrapper.find('.task-table').exists()).toBe(true)
        expect(wrapper.find('.empty').exists()).toBe(false)
        // Selection stays fail-closed while the query is in error.
        expect(vm.selectionReady).toBe(false)

        initial.refresh.mockImplementationOnce(async () => {
            initial.status.value = 'loading'
            initial.error.value = null
            await Promise.resolve()
            initial.status.value = 'ready'
        })
        await vm.retry()
        await flushPromises()
        await nextTick()

        expect(vm.error).toBeNull()
        expect(wrapper.find('.refresh-error').exists()).toBe(false)
        expect(vm.shownTasks.map((t: any) => t.id)).toEqual(['A-1'])
        wrapper.unmount()
    })

    it('a background refresh keeps the table mounted and selection fail-closed while loading', async () => {
        const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
        await settle()

        const initial = handleFor({})
        initial.items.value = [baseTask({ id: 'A-1' })]
        await settle()
        const vm = wrapper.vm as any
        vm.setSelectedIds(['A-1'])
        expect(vm.selectionReady).toBe(true)

        let finish!: () => void
        initial.refresh.mockImplementationOnce(() => {
            initial.status.value = 'loading'
            return new Promise<void>((resolve) => {
                finish = () => { initial.status.value = 'ready'; resolve() }
            })
        })
        const retryPromise = vm.retry()
        await flushPromises()
        await nextTick()

        // Background refresh: table stays, loading flag set, selection gated.
        expect(vm.loading).toBe(true)
        expect(wrapper.find('.task-table').exists()).toBe(true)
        expect(wrapper.find('.empty').exists()).toBe(false)
        expect(vm.selectionReady).toBe(false)
        expect((wrapper.find('.task-table').attributes('data-loading'))).toBe('1')

        finish()
        await retryPromise
        await settle()
        expect(vm.loading).toBe(false)
        expect(vm.selectionReady).toBe(true)
        wrapper.unmount()
    })
})
