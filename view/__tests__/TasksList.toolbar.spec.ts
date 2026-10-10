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
const _status = ref('idle' as string)
const _error = ref(null as string | null)
const _hasSnapshot = ref(false)

const tasksQueryHandle = {
    key: 'tasks-test',
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

vi.mock('../api/client', () => ({
    api: {
        updateTask: vi.fn(async () => ({})),
        setStatus: vi.fn(async () => ({})),
        deleteTask: vi.fn(async () => ({})),
        exportTasks: vi.fn(async () => ({ ok: true })),
    },
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))

// The toolbar under test lives in FilterBar's #actions slot, so the mock must
// render that slot. UiButton, IconGlyph, and ColumnsMenu stay real so class,
// aria, and disabled bindings reach the rendered DOM unchanged.
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
    default: {
        props: ['statuses', 'priorities', 'value', 'customPresets'],
        emits: ['update:value', 'preset'],
        template: '<div class="chips" />',
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

async function mountSettled() {
    const wrapper = mount(TasksList, { global: { stubs: { Teleport: true } } })
    await flushPromises()
    await vi.runAllTimersAsync()
    await flushPromises()
    return wrapper
}

describe('TasksList toolbar actions', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-09T12:00:00'))

        routeState.query = {}
        tasksStore.items.value = []
        tasksStore.status.value = 'idle'
        _hasSnapshot.value = false
        tasksQueryHandle.refresh.mockClear()
        tasksStore.getQuery.mockClear()

        routerPushMock.mockClear()
        routerReplaceMock.mockClear()
        tasksStore.hydrateAll.mockClear()
        projectsStore.refresh.mockClear()
        sprintsStore.refresh.mockClear()
        configStore.refresh.mockClear()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('renders the bulk toggle as an accessible pressed-state button without a local add-task button', async () => {
        tasksStore.items.value = [baseTask({ id: 'PRJ-1' })]
        const wrapper = await mountSettled()

        const bulkButton = wrapper.find('button[aria-label="Bulk select"]')
        expect(bulkButton.exists()).toBe(true)
        expect(bulkButton.attributes('aria-pressed')).toBe('false')
        expect(bulkButton.attributes('title')).toBe('Bulk select')
        expect(bulkButton.text()).toContain('Bulk select')
        expect(bulkButton.find('input[type="checkbox"]').exists()).toBe(false)

        // The local toolbar create action is gone; creation moved to the
        // global header action owned elsewhere.
        expect(wrapper.find('button[aria-label="Add task"]').exists()).toBe(false)

        wrapper.unmount()
    })

    it('toggling bulk on shows the selected-count badge and bulk menu control', async () => {
        tasksStore.items.value = [baseTask({ id: 'PRJ-1' })]
        const wrapper = await mountSettled()

        const bulkButton = wrapper.find('button[aria-label="Bulk select"]')
        await bulkButton.trigger('click')
        await flushPromises()

        expect(bulkButton.attributes('aria-pressed')).toBe('true')
        expect(wrapper.find('.tasks-toolbar-selected').exists()).toBe(true)
        expect(wrapper.find('.tasks-toolbar-selected').text()).toContain('Selected:')
        expect(wrapper.find('.tasks-toolbar-selected').text()).toContain('0 / 1')
        expect(wrapper.find('button[aria-label="Bulk actions"]').exists()).toBe(true)

        const vm = wrapper.vm as any
        vm.setSelectedIds(['PRJ-1'])
        await flushPromises()
        expect(wrapper.find('.tasks-toolbar-selected').text()).toContain('1 / 1')

        wrapper.unmount()
    })

    it('toggling bulk off clears the selection and hides the badge', async () => {
        tasksStore.items.value = [baseTask({ id: 'PRJ-1' })]
        const wrapper = await mountSettled()

        const bulkButton = wrapper.find('button[aria-label="Bulk select"]')
        await bulkButton.trigger('click')
        await flushPromises()

        const vm = wrapper.vm as any
        vm.setSelectedIds(['PRJ-1'])
        await bulkButton.trigger('click')
        await flushPromises()

        expect(bulkButton.attributes('aria-pressed')).toBe('false')
        expect(wrapper.find('.tasks-toolbar-selected').exists()).toBe(false)
        expect(wrapper.find('button[aria-label="Bulk actions"]').exists()).toBe(false)
        expect(vm.selectedIds).toEqual([])

        wrapper.unmount()
    })

    it('keeps Export and Columns controls accessible with expanded state on the columns trigger', async () => {
        tasksStore.items.value = [baseTask({ id: 'PRJ-1' })]
        const wrapper = await mountSettled()

        const exportButton = wrapper.find('button[aria-label="Export CSV"]')
        expect(exportButton.exists()).toBe(true)
        expect(exportButton.attributes('title')).toBe('Export the current filtered view as CSV')
        expect(exportButton.text()).toContain('Export')
        expect(exportButton.attributes('disabled')).toBeUndefined()

        const columnsButton = wrapper.find('button[aria-label="Configure columns"]')
        expect(columnsButton.exists()).toBe(true)
        expect(columnsButton.attributes('title')).toBe('Configure columns')
        expect(columnsButton.attributes('aria-expanded')).toBe('false')
        expect(columnsButton.text()).toContain('Columns')

        await columnsButton.trigger('click')
        await flushPromises()
        expect(columnsButton.attributes('aria-expanded')).toBe('true')
        expect(wrapper.find('[data-columns-menu-popover]').exists()).toBe(true)

        wrapper.unmount()
    })
})
