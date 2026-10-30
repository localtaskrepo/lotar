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
    getBacklogOrder: vi.fn(async (): Promise<import('../api/types').TaskOrderResponse> => ({
        status: 'ok',
        tasks: [],
        changed: false,
    })),
    reorderTasks: vi.fn(async (): Promise<import('../api/types').TaskOrderResponse> => ({
        status: 'ok',
        tasks: [],
        changed: false,
    })),
    listTasks: vi.fn(),
    sprintAdd: vi.fn(),
    sprintRemove: vi.fn(),
    showConfig: vi.fn(async () => ({})),
}))

vi.mock('../api/client', () => ({ api }))

const sseHandlers = vi.hoisted(() => new Map<string, Array<(event: MessageEvent) => void>>())
vi.mock('../composables/useSse', () => ({
    useSse: () => ({
        on: (kind: string, handler: (event: MessageEvent) => void) => {
            sseHandlers.set(kind, [...(sseHandlers.get(kind) ?? []), handler])
        },
        close: vi.fn(),
    }),
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))

const openTaskPanel = vi.fn()
// Mirrors the shared controller's reactive state; tests toggle `open` to
// model the panel lifecycle without mounting TaskPanelHost.
const sharedPanelState = reactive({ open: false })

vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({
        state: sharedPanelState,
        openTaskPanel,
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
        template:
            '<select class="select" @change="$emit(\'update:modelValue\', $event.target && $event.target.value)"><slot /></select>',
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
        template:
            '<div class="view-settings"><input type="checkbox" data-testid="allow-closed-toggle" :checked="!!allowClosed" @change="$emit(\'update:allowClosed\', $event.target && $event.target.checked)" /></div>',
    },
}))

vi.mock('../components/analytics/SprintAnalyticsDialog.vue', () => ({
    default: { template: '<div class="analytics" />' },
}))

const sprintsStore = {
    sprints: ref<any[]>([]),
    loading: ref(false),
    refresh: vi.fn(async () => {}),
    missingSprints: ref<any[]>([]),
    hasMissing: ref(false),
}

vi.mock('../composables/useSprints', () => ({
    useSprintFilterOptions: () => ref([]),
    useSprints: () => sprintsStore,
}))

vi.mock('../composables/useConfig', () => ({
    useConfig: () => ({
        scope: ref(''),
        tags: ref<string[]>([]),
        members: ref<string[]>([]),
        sprintDefaults: ref<any>({}),
        statuses: ref<string[]>(['open', 'done']),
        priorities: ref<string[]>(['low', 'med', 'high']),
        types: ref<string[]>(['task']),
        customFields: ref<string[]>([]),
        refresh: vi.fn(async () => {}),
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
        loadSummary: vi.fn(async () => {}),
        loadBurndown: vi.fn(async () => {}),
        loadVelocity: vi.fn(async () => {}),
        fetchSprintAnalytics: vi.fn(async () => {}),
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
        limit: 1000,
        offset: 0,
        tasks,
    }
}

describe('SprintsList add-task controls (DEV-104)', () => {
    beforeEach(() => {
        routeState.query = {}
        routeState.hash = ''
        localStorage.clear()
        localStorage.setItem('lotar.sprints.window.v2', 'all')

        sprintsStore.sprints.value = [
            { id: 1, display_name: 'Alpha', state: 'active', created: '2026-01-01T00:00:00Z' },
            { id: 2, display_name: 'Beta', state: 'complete', created: '2026-01-02T00:00:00Z' },
        ]
        sprintsStore.refresh.mockClear()

        api.listTasks.mockReset().mockResolvedValue(tasksResponse([makeTask('PRJ-001', [1])]))
        api.sprintAdd.mockReset()
        api.sprintRemove.mockReset()
        openTaskPanel.mockReset()
        sharedPanelState.open = false
    })

    afterEach(() => {
        sprintsStore.sprints.value = []
    })

    async function mountSettled() {
        const wrapper = mount(SprintsList)
        await flushPromises()
        api.listTasks.mockClear()
        sprintsStore.refresh.mockClear()
        openTaskPanel.mockClear()
        return wrapper
    }

    it('renders labelled Add task controls for every sprint group and the backlog', async () => {
        const wrapper = await mountSettled()

        const sprintButtons = wrapper.findAll('[data-testid="sprint-add-task"]')
        expect(sprintButtons.length).toBe(2)
        expect(sprintButtons[0]!.attributes('aria-label')).toBe('Add task to Alpha')
        expect(sprintButtons[1]!.attributes('aria-label')).toBe('Add task to Beta')
        expect(sprintButtons[0]!.text()).toContain('Add task')

        const backlogButton = wrapper.find('[data-testid="backlog-add-task"]')
        expect(backlogButton.exists()).toBe(true)
        expect(backlogButton.attributes('aria-label')).toBe('Add backlog task')
        expect(backlogButton.text()).toContain('Add task')

        wrapper.unmount()
    })

    it('opens the shared create panel with the sprint destination and current project scope', async () => {
        const wrapper = await mountSettled()

        await wrapper.find('[data-testid="sprint-add-task"]').trigger('click')
        expect(openTaskPanel).toHaveBeenCalledTimes(1)
        const call = openTaskPanel.mock.calls[0]![0] as Record<string, any>
        expect(call.taskId).toBe('new')
        expect(call.initialProject).toBeNull()
        expect(call.initialSprint).toBe(1)
        expect(typeof call.onCreated).toBe('function')

        openTaskPanel.mockClear()
        routeState.query = { project: 'PRJ' }
        const scoped = await mountSettled()
        await scoped.find('[data-testid="backlog-add-task"]').trigger('click')
        const scopedCall = openTaskPanel.mock.calls[0]![0] as Record<string, any>
        expect(scopedCall.initialProject).toBe('PRJ')
        expect(scopedCall.initialSprint).toBeNull()

        wrapper.unmount()
        scoped.unmount()
    })

    it('disables the sprint add button for closed sprints unless allow-closed is enabled', async () => {
        const wrapper = await mountSettled()

        const openButton = wrapper.findAll('[data-testid="sprint-add-task"]')[0]!
        const closedButton = wrapper.findAll('[data-testid="sprint-add-task"]')[1]!
        expect(openButton.attributes('disabled')).toBeUndefined()
        expect(closedButton.attributes('disabled')).toBeDefined()

        await closedButton.trigger('click')
        expect(openTaskPanel).not.toHaveBeenCalled()

        const toggle = wrapper.find('[data-testid="allow-closed-toggle"]')
        expect(toggle.exists()).toBe(true)
        await toggle.setValue(true)
        await nextTick()

        expect(closedButton.attributes('disabled')).toBeUndefined()
        await closedButton.trigger('click')
        expect(openTaskPanel).toHaveBeenCalledTimes(1)
        expect((openTaskPanel.mock.calls[0]![0] as Record<string, any>).initialSprint).toBe(2)

        wrapper.unmount()
    })

    it('disables contextual add controls while the shared panel is open and re-enables after close', async () => {
        const wrapper = await mountSettled()

        const sprintButton = wrapper.findAll('[data-testid="sprint-add-task"]')[0]!
        const backlogButton = wrapper.find('[data-testid="backlog-add-task"]')
        expect(sprintButton.attributes('disabled')).toBeUndefined()
        expect(backlogButton.attributes('disabled')).toBeUndefined()

        sharedPanelState.open = true
        await nextTick()

        expect(sprintButton.attributes('disabled')).toBeDefined()
        expect(backlogButton.attributes('disabled')).toBeDefined()
        // Accessible names survive the disabled state.
        expect(sprintButton.attributes('aria-label')).toBe('Add task to Alpha')
        expect(backlogButton.attributes('aria-label')).toBe('Add backlog task')

        await sprintButton.trigger('click')
        await backlogButton.trigger('click')
        expect(openTaskPanel).not.toHaveBeenCalled()

        sharedPanelState.open = false
        await nextTick()

        expect(sprintButton.attributes('disabled')).toBeUndefined()
        expect(backlogButton.attributes('disabled')).toBeUndefined()
        await sprintButton.trigger('click')
        expect(openTaskPanel).toHaveBeenCalledTimes(1)
        expect((openTaskPanel.mock.calls[0]![0] as Record<string, any>).initialSprint).toBe(1)

        wrapper.unmount()
    })

    it('reconciles the page when the created callback fires', async () => {
        const wrapper = await mountSettled()

        await wrapper.find('[data-testid="sprint-add-task"]').trigger('click')
        const onCreated = (openTaskPanel.mock.calls[0]![0] as Record<string, any>).onCreated as (task: any) => void
        api.listTasks.mockResolvedValue(tasksResponse([makeTask('PRJ-001', [1]), makeTask('PRJ-009', [1])]))
        onCreated(makeTask('PRJ-009', [1]))
        await flushPromises()

        expect(api.listTasks).toHaveBeenCalled()
        expect(sprintsStore.refresh).toHaveBeenCalled()
        const rows = wrapper.findAll('tr.task-row[data-task-id="PRJ-009"]')
        expect(rows.length).toBe(1)

        wrapper.unmount()
    })

    it('does not infer query membership from a newly created task while refresh is pending', async () => {
        const wrapper = await mountSettled()
        await wrapper.find('[data-testid="sprint-add-task"]').trigger('click')
        const onCreated = (openTaskPanel.mock.calls[0]![0] as Record<string, any>).onCreated as (task: any) => void
        let finishRefresh!: (response: ReturnType<typeof tasksResponse>) => void
        api.listTasks.mockReturnValue(
            new Promise((resolve) => {
                finishRefresh = resolve
            }),
        )
        onCreated(makeTask('PRJ-009', [1]))
        await nextTick()
        expect(wrapper.findAll('tr.task-row[data-task-id="PRJ-009"]')).toHaveLength(0)
        finishRefresh(tasksResponse([makeTask('PRJ-001', [1])]))
        await flushPromises()
        expect(wrapper.findAll('tr.task-row[data-task-id="PRJ-009"]')).toHaveLength(0)
        wrapper.unmount()
    })

    it('shows tasks created elsewhere without a reload (DEV-248)', async () => {
        sseHandlers.clear()
        const wrapper = await mountSettled()
        api.listTasks.mockResolvedValue(tasksResponse([makeTask('PRJ-001', [1]), makeTask('PRJ-010', [])]))
        // A burst of events (created, then its follow-up update) is one refresh.
        for (const kind of ['task_created', 'task_updated']) {
            for (const handler of sseHandlers.get(kind) ?? [])
                handler(new MessageEvent(kind, { data: '{"id":"PRJ-010"}' }))
        }
        await new Promise((resolve) => setTimeout(resolve, 300))
        await flushPromises()
        expect(api.listTasks).toHaveBeenCalledTimes(1)
        expect(wrapper.findAll('tr.task-row[data-task-id="PRJ-010"]')).toHaveLength(1)
        wrapper.unmount()
    })
})
