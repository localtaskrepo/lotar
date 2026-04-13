import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, h, ref, shallowRef } from 'vue'
import type { SprintListItem, TaskDTO } from '../api/types'

const routeState: { query: Record<string, any> } = { query: { project: 'ACME' } }
const routerPushMock = vi.fn()

const projectsStore = {
    projects: ref([{ prefix: 'ACME', name: 'Acme Co' }, { prefix: 'BETA', name: 'Beta Co' }]),
    refresh: vi.fn(async () => { }),
}

const taskMap = shallowRef(new Map<string, TaskDTO>())
const taskVersion = shallowRef(0)
const tasksStore = {
    _map: taskMap,
    version: taskVersion,
    items: computed(() => { void taskVersion.value; return Array.from(taskMap.value.values()) }),
    count: computed(() => taskMap.value.size),
    serverTotal: shallowRef(0),
    status: shallowRef('ready' as const),
    error: shallowRef(null as string | null),
    lastSyncAt: shallowRef(0),
    hasData: computed(() => taskMap.value.size > 0),
    hydrateAll: vi.fn(async () => {}),
    hydratePage: vi.fn(async () => ({ total: 0 })),
    fetchOne: vi.fn(async () => null),
    forceRefresh: vi.fn(async () => {}),
    add: vi.fn(async (p: any) => p),
    update: vi.fn(async (_id: string, p: any) => p),
    remove: vi.fn(async () => {}),
    upsert: vi.fn((task: TaskDTO) => { taskMap.value.set(task.id, task); taskVersion.value++; }),
    evict: vi.fn((id: string) => { taskMap.value.delete(id); taskVersion.value++; }),
    connectSse: vi.fn(),
    disconnectSse: vi.fn(),
    sseConnected: shallowRef(false),
}

const sprintsStore = {
    sprints: ref<SprintListItem[]>([]),
    refresh: vi.fn(async () => { }),
    loading: ref(false),
}

const configStore = {
    statuses: ref<string[]>(['Todo', 'Doing', 'Done']),
    priorities: ref<string[]>(['low', 'med', 'high']),
    types: ref<string[]>(['task']),
    customFields: ref<string[]>([]),
    refresh: vi.fn(async () => { }),
    loading: ref(false),
}

const openTaskPanelMock = vi.fn()

vi.mock('vue-router', () => ({
    useRoute: () => routeState,
    useRouter: () => ({ push: routerPushMock }),
}))

vi.mock('../api/client', () => ({
    api: {
        setStatus: vi.fn(async () => { }),
    },
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

vi.mock('../components/UiLoader.vue', () => ({
    default: { template: '<div class="loader"><slot /></div>' },
}))

vi.mock('../components/UiEmptyState.vue', () => ({
    default: { template: '<div class="empty" />' },
}))

vi.mock('../components/IconGlyph.vue', () => ({
    default: { template: '<span class="icon" />' },
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
        props: ['statuses', 'priorities', 'types', 'value', 'showStatus', 'emitProjectKey', 'storageKey'],
        emits: ['update:value'],
        setup(_props: any, { expose }: { expose: (api: any) => void }) {
            expose({ appendCustomFilter: () => { }, clear: () => { } })
            return () => h('div', { class: 'filter-bar' })
        },
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
        loading: configStore.loading,
    }),
}))

vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({ openTaskPanel: openTaskPanelMock }),
}))

import Board from '../pages/Board.vue'

function baseTask(overrides: Partial<TaskDTO>): TaskDTO {
    return {
        id: 'ACME-1',
        title: 'Alpha',
        status: 'Todo',
        priority: 'high',
        task_type: 'task',
        assignee: 'alice',
        created: '2026-01-01T10:00:00Z',
        modified: '2026-01-02T10:00:00Z',
        tags: ['one', 'two'],
        relationships: {},
        comments: [],
        custom_fields: {},
        sprints: [1],
        references: [],
        history: [],
        reporter: null,
        effort: null,
        due_date: '2026-01-10',
        ...overrides,
    } as any
}

describe('Board field visibility', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))

        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        tasksStore.hydrateAll.mockClear()
        projectsStore.refresh.mockClear()
        sprintsStore.refresh.mockClear()
        configStore.refresh.mockClear()
        routerPushMock.mockClear()
        openTaskPanelMock.mockClear()
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('hides card fields per-project and persists to localStorage', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', title: 'Alpha' }),
            baseTask({ id: 'BETA-2', title: 'Beta', assignee: 'bob', status: 'Doing' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const firstCard = wrapper.findAll('article.card.task')[0]
        expect(firstCard.text()).toContain('ACME-1')
        expect(firstCard.text()).toContain('Alpha')
        expect(firstCard.text()).toContain('high')
        expect(firstCard.text()).toContain('alice')
        expect(firstCard.text()).toContain('Due')
        expect(firstCard.text()).toContain('one')

        // Open is not required for DOM presence in tests; just flip the checkboxes.
        const checks = wrapper.findAll('input[type="checkbox"]')

        const byLabel = (label: string) =>
            checks.find((c) => (c.element.nextSibling as any)?.textContent?.trim() === label)!

        await byLabel('ID').setValue(false)
        await byLabel('Title').setValue(false)
        await byLabel('Priority').setValue(false)
        await flushPromises()

        expect(firstCard.text()).not.toContain('ACME-1')
        expect(firstCard.text()).not.toContain('Alpha')
        expect(firstCard.text()).not.toContain('high')

        await byLabel('Assignee').setValue(false)
        await byLabel('Due').setValue(false)
        await byLabel('Tags').setValue(false)
        await byLabel('Sprints').setValue(false)
        await flushPromises()

        expect(firstCard.text()).not.toContain('alice')
        expect(firstCard.text()).not.toContain('Due')
        expect(firstCard.text()).not.toContain('one')

        const saved = JSON.parse(localStorage.getItem('lotar.boardFields::ACME') || '{}')
        expect(saved).toMatchObject({
            id: false,
            title: false,
            priority: false,
            assignee: false,
            due_date: false,
            tags: false,
            sprints: false,
        })

        // Different project should not inherit ACME settings.
        routeState.query = { project: 'BETA' }
        const wrapper2 = mount(Board)
        await flushPromises()

        const betaCard = wrapper2.findAll('article.card.task')[0]
        expect(betaCard.text()).toContain('BETA-2')
        expect(betaCard.text()).toContain('Beta')
    })
})
