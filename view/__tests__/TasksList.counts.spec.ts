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

const tasksStore = {
    items: _tasksItems,
    count: computed(() => _tasksItems.value.length),
    status: ref('idle' as string),
    error: ref(null as string | null),
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
        props: ['tasks'],
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

describe('TasksList counts', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-09T12:00:00'))

        routeState.query = {}
        tasksStore.items.value = []
        tasksStore.status.value = 'idle'

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

    it('shows total count for the current server-filtered result set', async () => {
        routeState.query = { due: 'today' }
        tasksStore.items.value = [baseTask({ id: 'PRJ-1', due_date: '2026-01-09' })]

        const wrapper = mount(TasksList, {
            global: {
                stubs: {
                    Teleport: true,
                },
            },
        })

        await flushPromises()
        vi.runAllTimers()
        await flushPromises()

        expect(wrapper.find('h1').text()).toContain('(1)')
    })
})
