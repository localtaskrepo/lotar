import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, ref, shallowRef } from 'vue'

const routeState: { query: Record<string, any> } = { query: {} }
const routerPushMock = vi.fn(async () => { })
const routerReplaceMock = vi.fn(async () => { })

const projectsStore = {
    refresh: vi.fn(async () => { }),
}

const _tasksItems = ref<any[]>([])
const _taskMap = shallowRef(new Map<string, any>())
const _orderIndex = ref<Map<string, number>>(new Map())
const _status = ref('idle' as string)
const _error = ref(null as string | null)
const _hasSnapshot = ref(false)

const tasksQueryHandle = {
    key: 'tasks-noop-test',
    hasSnapshot: _hasSnapshot,
    ids: computed(() => _tasksItems.value.map((t: any) => t.id)),
    ranks: _orderIndex,
    total: computed(() => _tasksItems.value.length),
    status: _status,
    error: _error,
    lastSyncAt: ref(1),
    tasks: computed(() => _tasksItems.value),
    refresh: vi.fn(async () => { _hasSnapshot.value = true }),
    retain: vi.fn(),
    release: vi.fn(),
}

const tasksStore = {
    items: _tasksItems,
    _map: _taskMap,
    count: computed(() => _tasksItems.value.length),
    status: _status,
    error: _error,
    getQuery: vi.fn(() => tasksQueryHandle),
    hydrateAll: vi.fn(async () => { }),
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

const api = vi.hoisted(() => ({
    updateTask: vi.fn(async (_id: string, _patch: any) => ({})),
    setStatus: vi.fn(async () => ({})),
    sprintAdd: vi.fn(async () => ({})),
    sprintRemove: vi.fn(async () => ({})),
    deleteTask: vi.fn(async () => ({})),
    exportTasks: vi.fn(async () => ({ ok: true })),
}))

vi.mock('../api/client', () => ({ api }))

const toast = vi.hoisted(() => vi.fn())
vi.mock('../components/toast', () => ({ showToast: toast }))

vi.mock('../components/FilterBar.vue', () => ({
    default: {
        props: ['statuses', 'priorities', 'types', 'sprintOptions', 'customPresets', 'value', 'storageKey'],
        emits: ['update:value'],
        template: '<div class="filter-bar"><slot name="actions" /></div>',
    },
}))

vi.mock('../components/TaskTable.vue', () => ({
    default: {
        props: ['tasks'],
        template: '<div class="task-table" :data-count="(tasks || []).length" />',
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
        template: '<button type="button" class="reload" @click="$emit(\'click\', $event)">Reload</button>',
    },
}))

vi.mock('../components/SmartListChips.vue', () => ({
    default: { template: '<div class="chips" />' },
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

function seedTask(overrides: Partial<any> = {}) {
    const task = baseTask(overrides)
    _tasksItems.value = [task]
    _taskMap.value = new Map([[task.id, task]])
    return task
}

async function mountSettled() {
    const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
    await flushPromises()
    await vi.runAllTimersAsync()
    await flushPromises()
    // onMounted refreshes sprints/config/route; the no-op assertions below
    // measure only post-mount activity.
    sprintsStore.refresh.mockClear()
    configStore.refresh.mockClear()
    tasksQueryHandle.refresh.mockClear()
    routerPushMock.mockClear()
    routerReplaceMock.mockClear()
    return wrapper
}

function sprintResponse(overrides: Partial<any> = {}) {
    return {
        status: 'ok',
        action: 'add',
        sprint_id: 1,
        sprint_label: 'Sprint 1',
        modified: [],
        unchanged: [],
        replaced: [],
        messages: [],
        integrity: undefined,
        ...overrides,
    }
}

describe('TasksList no-op mutation guards (DEV-102)', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-09T12:00:00'))

        routeState.query = {}
        _tasksItems.value = []
        _taskMap.value = new Map()
        _status.value = 'idle'
        _error.value = null
        _hasSnapshot.value = false
        tasksQueryHandle.refresh.mockClear()
        tasksStore.getQuery.mockClear()
        tasksStore.upsert.mockClear()

        routerPushMock.mockClear()
        routerReplaceMock.mockClear()
        projectsStore.refresh.mockClear()
        sprintsStore.refresh.mockClear()
        configStore.refresh.mockClear()
        api.updateTask.mockClear()
        api.sprintAdd.mockReset().mockResolvedValue(sprintResponse())
        api.sprintRemove.mockReset().mockResolvedValue(sprintResponse({ action: 'remove' }))
        toast.mockClear()
        localStorage.clear()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('submitting the assign dialog with the identical assignee sends no request', async () => {
        seedTask({ assignee: 'alice' })
        const wrapper = await mountSettled()
        const vm = wrapper.vm as any

        vm.openSingleAssign('PRJ-1')
        await flushPromises()
        const input = wrapper.find('input[placeholder="username or @me"]')
        expect(input.exists()).toBe(true)
        await input.setValue('alice')
        await wrapper.find('form.tasks-modal__form button[type="submit"]').trigger('submit')
        await flushPromises()

        expect(api.updateTask).not.toHaveBeenCalled()
        expect(tasksStore.upsert).not.toHaveBeenCalled()
        expect(toast).not.toHaveBeenCalledWith('Task assigned to alice')
        wrapper.unmount()
    })

    it('assigning a different user posts exactly one update', async () => {
        seedTask({ assignee: 'alice' })
        const wrapper = await mountSettled()
        const vm = wrapper.vm as any

        vm.openSingleAssign('PRJ-1')
        await flushPromises()
        const input = wrapper.find('input[placeholder="username or @me"]')
        await input.setValue('bob')
        await wrapper.find('form.tasks-modal__form button[type="submit"]').trigger('submit')
        await flushPromises()

        expect(api.updateTask).toHaveBeenCalledExactlyOnceWith('PRJ-1', { assignee: 'bob' })
        expect(tasksStore.upsert).toHaveBeenCalledTimes(1)
        expect(toast).toHaveBeenCalledWith('Task assigned to bob')
        wrapper.unmount()
    })

    it('keeps @me vs a stored user a server-judged request (no strip-@ equality)', async () => {
        seedTask({ assignee: 'alice' })
        const wrapper = await mountSettled()
        const vm = wrapper.vm as any

        vm.openSingleAssign('PRJ-1')
        await flushPromises()
        const input = wrapper.find('input[placeholder="username or @me"]')
        await input.setValue('@me')
        await wrapper.find('form.tasks-modal__form button[type="submit"]').trigger('submit')
        await flushPromises()

        // The identity cannot be resolved client-side: the request must go
        // to the server, which owns alias resolution and durable no-ops.
        expect(api.updateTask).toHaveBeenCalledExactlyOnceWith('PRJ-1', { assignee: '@me' })
        wrapper.unmount()
    })

    it('unassigning a task without an assignee sends no request', async () => {
        seedTask({ assignee: null })
        const wrapper = await mountSettled()
        const vm = wrapper.vm as any

        await vm.unassignTasks(['PRJ-1'])
        await flushPromises()

        expect(api.updateTask).not.toHaveBeenCalled()
        expect(toast).not.toHaveBeenCalled()

        await vm.unassignTasks(['PRJ-1'])
        await flushPromises()
        expect(api.updateTask).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('unassigning an assigned task still posts the clear exactly once', async () => {
        seedTask({ assignee: 'alice' })
        const wrapper = await mountSettled()
        const vm = wrapper.vm as any

        await vm.unassignTasks(['PRJ-1'])
        await flushPromises()

        expect(api.updateTask).toHaveBeenCalledExactlyOnceWith('PRJ-1', { assignee: '' })
        expect(toast).toHaveBeenCalledWith('Unassigned 1 task')
        wrapper.unmount()
    })

    it('skips sprint and list reconciliation when the sprint action modified nothing', async () => {
        seedTask()
        const wrapper = await mountSettled()
        const vm = wrapper.vm as any

        api.sprintAdd.mockResolvedValueOnce(sprintResponse({ modified: [], unchanged: ['PRJ-1'] }))
        await vm.performSprintAction(['PRJ-1'], 'add', { sprint: '1' })
        await flushPromises()

        expect(api.sprintAdd).toHaveBeenCalledTimes(1)
        expect(sprintsStore.refresh).not.toHaveBeenCalled()
        expect(tasksQueryHandle.refresh).not.toHaveBeenCalled()
        expect(routerPushMock).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('reconciles sprints and the task list after a real sprint change', async () => {
        seedTask()
        const wrapper = await mountSettled()
        const vm = wrapper.vm as any

        api.sprintAdd.mockResolvedValueOnce(sprintResponse({ modified: ['PRJ-1'] }))
        await vm.performSprintAction(['PRJ-1'], 'add', { sprint: '1' })
        await flushPromises()

        expect(api.sprintAdd).toHaveBeenCalledTimes(1)
        expect(sprintsStore.refresh).toHaveBeenCalledTimes(1)
        expect(tasksQueryHandle.refresh).toHaveBeenCalledTimes(1)
        expect(toast).toHaveBeenCalledWith('Added 1 task(s) to Sprint 1')
        wrapper.unmount()
    })
})
