import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, reactive, ref } from 'vue'

const routeState: { query: Record<string, any>; hash: string } = { query: {}, hash: '' }

vi.mock('vue-router', () => ({
    useRoute: () => routeState,
    useRouter: () => ({
        push: vi.fn(),
        replace: vi.fn(),
    }),
}))

const api = vi.hoisted(() => ({
    getBacklogOrder: vi.fn(async (): Promise<import('../api/types').TaskOrderResponse> => ({ status: 'ok', tasks: [], changed: false })),
    reorderTasks: vi.fn(async (): Promise<import('../api/types').TaskOrderResponse> => ({ status: 'ok', tasks: [], changed: false })),
    listTasks: vi.fn(),
    sprintAdd: vi.fn(),
    sprintRemove: vi.fn(),
    sprintUpdate: vi.fn(),
    showConfig: vi.fn(async () => ({})),
}))

vi.mock('../api/client', () => ({ api }))

vi.mock('../composables/useSse', () => ({
    useSse: () => ({ on: vi.fn(), close: vi.fn() }),
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))

const sharedPanelState = reactive({ open: false })

vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({
        state: sharedPanelState,
        openTaskPanel: vi.fn(),
    }),
}))

vi.mock('../components/IconGlyph.vue', () => ({
    default: { template: '<span class="icon" />' },
}))

vi.mock('../components/UiButton.vue', () => ({
    default: {
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

vi.mock('../components/UiSelect.vue', () => ({
    default: {
        props: ['modelValue'],
        emits: ['update:modelValue'],
        template: '<select class="select"><slot /></select>',
    },
}))

vi.mock('../components/ReloadButton.vue', () => ({
    default: { template: '<button type="button" class="reload"><slot /></button>' },
}))

vi.mock('../components/FilterBar.vue', () => ({
    default: {
        emits: ['update:value'],
        template: '<div class="filter-bar"><slot name="panel" /><slot name="actions" /></div>',
    },
}))

vi.mock('../components/ColumnsMenu.vue', () => ({
    default: {
        template: '<div class="columns-menu"><slot name="trigger" /></div>',
    },
}))

vi.mock('../components/SprintViewSettings.vue', () => ({
    default: {
        props: ['allowClosed', 'showAllowClosed', 'choices', 'timeRange', 'highlightMultiSprint'],
        emits: ['update:allowClosed'],
        template: '<div class="view-settings" />',
    },
}))

vi.mock('../components/analytics/SprintAnalyticsDialog.vue', () => ({
    default: { template: '<div class="analytics" />' },
}))

const sprintsStore = {
    sprints: ref<any[]>([]),
    loading: ref(false),
    refresh: vi.fn(async () => { }),
    missingSprints: ref<any[]>([]),
    hasMissing: ref(false),
}

vi.mock('../composables/useSprints', () => ({
    useSprintFilterOptions: () => ref([]),
    useSprints: () => sprintsStore,
}))

vi.mock('../composables/useConfig', () => ({
    useConfig: () => ({
        scope: ref(''), tags: ref<string[]>([]), members: ref<string[]>([]),
        sprintDefaults: ref<any>({}),
        statuses: ref<string[]>(['open', 'done']),
        priorities: ref<string[]>(['low', 'med', 'high']),
        types: ref<string[]>(['task']),
        customFields: ref<string[]>([]),
        refresh: vi.fn(async () => { }),
    }),
}))

vi.mock('../composables/useSprintAnalytics', () => ({
    DEFAULT_VELOCITY_PARAMS: { limit: 4, metric: 'tasks' },
    useSprintAnalytics: () => ({
        getSummary: vi.fn(() => undefined),
        getBurndown: vi.fn(() => undefined),
        getVelocity: vi.fn(() => undefined),
        getSummaryError: vi.fn(() => null),
        getBurndownError: vi.fn(() => null),
        getVelocityError: vi.fn(() => null),
        isSummaryLoading: vi.fn(() => false),
        isBurndownLoading: vi.fn(() => false),
        isVelocityLoading: vi.fn(() => false),
        loadSummary: vi.fn(async () => { }),
        loadBurndown: vi.fn(async () => { }),
        loadVelocity: vi.fn(async () => { }),
        fetchSprintAnalytics: vi.fn(async () => { }),
    }),
}))

import SprintsList from '../pages/SprintsList.vue'

function makeTask(id: string, sprints: number[]) {
    return {
        id,
        title: `Task ${id}`,
        status: 'open',
        priority: 'med',
        task_type: 'task',
        reporter: null,
        assignee: null,
        effort: null,
        due_date: null,
        created: '2026-01-01T00:00:00Z',
        modified: '2026-01-01T00:00:00Z',
        tags: [],
        relationships: {},
        comments: [],
        custom_fields: {},
        sprints,
        references: [],
        history: [],
    }
}

function tasksResponse(tasks: any[]) {
    return {
        status: 'ok',
        count: tasks.length,
        total: tasks.length,
        limit: 200,
        offset: 0,
        tasks,
    }
}

function makeDataTransfer() {
    const store: Record<string, string> = {}
    return {
        effectAllowed: 'none',
        setData: (type: string, value: string) => { store[type] = value },
        getData: (type: string) => store[type] || '',
        __store: store,
    } as any
}

describe('SprintsList column drag reorder (DEV-112)', () => {
    beforeEach(() => {
        routeState.query = {}
        routeState.hash = ''
        localStorage.clear()
        localStorage.setItem('lotar.sprints.window.v2', 'all')

        sprintsStore.sprints.value = [
            { id: 1, display_name: 'Alpha', state: 'active', created: '2026-01-01T00:00:00Z' },
        ]
        sprintsStore.refresh.mockClear()

        api.listTasks.mockReset().mockResolvedValue(tasksResponse([
            makeTask('PRJ-001', [1]),
            makeTask('PRJ-002', []),
        ]))
        sharedPanelState.open = false
    })

    afterEach(() => {
        sprintsStore.sprints.value = []
        vi.useRealTimers()
    })

    async function mountSettled() {
        const wrapper = mount(SprintsList)
        await flushPromises()
        wrapper.vm.$nextTick()
        return wrapper
    }

    function sprintHeaders(wrapper: ReturnType<typeof mount>) {
        return wrapper
            .findAll('[data-sprint-id="1"] th[data-column]')
            .map((th) => th.attributes('data-column'))
    }

    it('reorders sprint table columns via drag and drop and persists the order', async () => {
        const wrapper = await mountSettled()

        const before = sprintHeaders(wrapper)
        expect(before.includes('status')).toBe(true)
        expect(before.includes('priority')).toBe(true)
        expect(before.indexOf('priority')).toBeGreaterThan(before.indexOf('status'))

        const dt = makeDataTransfer()
        const priorityButton = wrapper.find('[data-sprint-id="1"] th[data-column="priority"] button.header-button')
        const statusTh = wrapper.find('[data-sprint-id="1"] th[data-column="status"]')
        await priorityButton.trigger('dragstart', { dataTransfer: dt })
        await statusTh.trigger('dragover', { dataTransfer: dt, clientX: 0 })
        await statusTh.trigger('drop', { dataTransfer: dt, clientX: 0 })

        const after = sprintHeaders(wrapper)
        expect(after.indexOf('priority')).toBeLessThan(after.indexOf('status'))

        // The backlog table shares the same column store and follows the new order.
        const backlogHeaders = wrapper
            .findAll('[data-sprint-id="backlog"] th[data-column]')
            .map((th) => th.attributes('data-column'))
        expect(backlogHeaders.indexOf('priority')).toBeLessThan(backlogHeaders.indexOf('status'))

        const saved = JSON.parse(localStorage.getItem('lotar.sprints.columnOrder') || '[]')
        expect(saved.indexOf('priority')).toBeLessThan(saved.indexOf('status'))

        wrapper.unmount()
    })

    it('tags sprint column drags with the sprints MIME plus text/plain', async () => {
        const wrapper = await mountSettled()

        const dt = makeDataTransfer()
        const titleButton = wrapper.find('[data-sprint-id="1"] th[data-column="title"] button.header-button')
        await titleButton.trigger('dragstart', { dataTransfer: dt })

        expect(dt.effectAllowed).toBe('move')
        expect(dt.__store['application/x-lotar-sprints-col']).toBe('title')
        expect(dt.__store['text/plain']).toBe('title')

        wrapper.unmount()
    })

    it('shows and clears drag indicators, and aborting a drag does not reorder', async () => {
        const wrapper = await mountSettled()

        const dt = makeDataTransfer()
        const priorityButton = wrapper.find('[data-sprint-id="1"] th[data-column="priority"] button.header-button')
        const statusTh = wrapper.find('[data-sprint-id="1"] th[data-column="status"]')

        await priorityButton.trigger('dragstart', { dataTransfer: dt })
        expect(priorityButton.classes()).toContain('is-dragging')

        await statusTh.trigger('dragover', { dataTransfer: dt, clientX: 0 })
        await nextTick()
        const statusButton = wrapper.find('[data-sprint-id="1"] th[data-column="status"] button.header-button')
        expect(statusButton.classes()).toContain('is-drag-over')

        await priorityButton.trigger('dragend', { dataTransfer: dt })
        await nextTick()
        expect(priorityButton.classes()).not.toContain('is-dragging')
        expect(statusButton.classes()).not.toContain('is-drag-over')

        const after = sprintHeaders(wrapper)
        expect(after.indexOf('priority')).toBeGreaterThan(after.indexOf('status'))
        // Aborting never mutated the stored order (nothing was persisted).
        expect(localStorage.getItem('lotar.sprints.columnOrder')).toBeNull()

        wrapper.unmount()
    })

    it('swallows header clicks right after a drag and sorts once the guard passes', async () => {
        vi.useFakeTimers({ toFake: ['Date'] })
        vi.setSystemTime(new Date('2026-01-01T00:00:00Z'))
        const wrapper = await mountSettled()

        const dt = makeDataTransfer()
        const priorityButton = wrapper.find('[data-sprint-id="1"] th[data-column="priority"] button.header-button')
        const statusTh = wrapper.find('[data-sprint-id="1"] th[data-column="status"]')
        await priorityButton.trigger('dragstart', { dataTransfer: dt })
        await statusTh.trigger('dragover', { dataTransfer: dt, clientX: 0 })
        await statusTh.trigger('drop', { dataTransfer: dt, clientX: 0 })

        await priorityButton.trigger('click')
        expect(localStorage.getItem('lotar.sprints.sort')).toBeNull()

        vi.setSystemTime(new Date(Date.now() + 250))
        await priorityButton.trigger('click')
        expect(JSON.parse(localStorage.getItem('lotar.sprints.sort') || 'null')).toEqual({ key: 'priority', dir: 'asc' })

        wrapper.unmount()
    })
})
