import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'

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
    showConfig: vi.fn(async () => ({})),
}))

vi.mock('../api/client', () => ({ api }))

vi.mock('../composables/useSse', () => ({
    useSse: () => ({ on: vi.fn(), close: vi.fn() }),
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
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
        template: '<select class="select" @change="$emit(\'update:modelValue\', $event.target && $event.target.value)"><slot /></select>',
    },
}))

vi.mock('../components/ReloadButton.vue', () => ({
    default: { template: '<button type="button" class="reload"><slot /></button>' },
}))

vi.mock('../components/SmartListChips.vue', () => ({
    default: { template: '<div class="chips" />' },
}))

vi.mock('../components/FilterBar.vue', () => ({
    default: {
        emits: ['update:value'],
        template: '<div class="filter-bar" />',
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

vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({
        state: { open: false },
        openTaskPanel: vi.fn(),
    }),
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

function sprintResponse(overrides: Partial<any> = {}) {
    return {
        status: 'ok',
        action: 'add',
        sprint_id: 1,
        sprint_label: 'Sprint',
        modified: [],
        unchanged: [],
        replaced: [],
        messages: [],
        integrity: undefined,
        ...overrides,
    }
}

/** Native event dispatch: the copy modifier must survive to the handlers. */
async function dispatchWithMeta(element: Element, type: string) {
    element.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, metaKey: true }))
    await nextTick()
}

describe('SprintsList no-op drop guards (DEV-102)', () => {
    beforeEach(() => {
        routeState.query = {}
        routeState.hash = ''
        localStorage.clear()
        localStorage.setItem('lotar.sprints.window.v2', 'all')

        sprintsStore.sprints.value = [
            { id: 1, display_name: 'Alpha', state: 'active', created: '2026-01-01T00:00:00Z' },
            { id: 2, display_name: 'Beta', state: 'active', created: '2026-01-02T00:00:00Z' },
        ]
        sprintsStore.refresh.mockClear()

        api.listTasks.mockReset().mockResolvedValue({
            status: 'ok',
            count: 1,
            total: 1,
            limit: 200,
            offset: 0,
            tasks: [makeTask('PRJ-001', [1])],
        })
        api.sprintAdd.mockReset().mockResolvedValue(sprintResponse())
        api.sprintRemove.mockReset().mockResolvedValue(sprintResponse({ action: 'remove' }))
    })

    afterEach(() => {
        sprintsStore.sprints.value = []
    })

    async function mountSettled() {
        const wrapper = mount(SprintsList)
        await flushPromises()
        api.sprintAdd.mockClear()
        api.sprintRemove.mockClear()
        sprintsStore.refresh.mockClear()
        api.listTasks.mockClear()
        return wrapper
    }

    it('dropping a task on its sole sprint sends no membership request and no refresh', async () => {
        const wrapper = await mountSettled()

        const row = wrapper.find('tr.task-row[data-task-id="PRJ-001"]')
        expect(row.exists()).toBe(true)
        expect(wrapper.find('[data-sprint-id="1"]').exists()).toBe(true)

        await row.trigger('dragstart')
        await wrapper.find('[data-sprint-id="1"]').trigger('drop')
        await flushPromises()

        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.sprintRemove).not.toHaveBeenCalled()
        expect(sprintsStore.refresh).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('reassigning to a genuinely different sprint keeps add, silent source remove, and refresh', async () => {
        const wrapper = await mountSettled()

        api.sprintAdd.mockResolvedValueOnce(sprintResponse({
            action: 'add', sprint_id: 2, sprint_label: 'Beta', modified: ['PRJ-001'],
        }))
        api.sprintRemove.mockResolvedValueOnce(sprintResponse({
            action: 'remove', sprint_id: 1, sprint_label: 'Alpha', modified: ['PRJ-001'],
        }))

        const row = wrapper.find('tr.task-row[data-task-id="PRJ-001"]')
        await row.trigger('dragstart')
        await wrapper.find('[data-sprint-id="2"]').trigger('drop')
        await flushPromises()

        expect(api.sprintAdd).toHaveBeenCalledTimes(1)
        expect(api.sprintAdd.mock.calls[0]![0]).toMatchObject({
            tasks: ['PRJ-001'],
            sprint: 2,
            force_single: true,
            cleanup_missing: true,
        })
        expect(api.sprintRemove).toHaveBeenCalledTimes(1)
        expect(api.sprintRemove.mock.calls[0]![0]).toMatchObject({
            tasks: ['PRJ-001'],
            sprint: 1,
            cleanup_missing: true,
        })
        expect(sprintsStore.refresh).toHaveBeenCalled()
        wrapper.unmount()
    })

    it('copy-dropping onto an already-held sprint sends no membership request', async () => {
        api.listTasks.mockResolvedValue({
            status: 'ok',
            count: 1,
            total: 1,
            limit: 200,
            offset: 0,
            tasks: [makeTask('PRJ-001', [1, 2])],
        })
        const wrapper = await mountSettled()

        const row = wrapper.find('tr.task-row[data-task-id="PRJ-001"]')
        expect(row.exists()).toBe(true)

        await dispatchWithMeta(row.element, 'dragstart')
        await dispatchWithMeta(wrapper.find('[data-sprint-id="1"]').element, 'drop')
        await flushPromises()

        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.sprintRemove).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('copy-dropping onto a new sprint still adds without force_single', async () => {
        const wrapper = await mountSettled()

        api.sprintAdd.mockResolvedValueOnce(sprintResponse({
            action: 'add', sprint_id: 2, sprint_label: 'Beta', modified: ['PRJ-001'],
        }))

        const row = wrapper.find('tr.task-row[data-task-id="PRJ-001"]')
        await dispatchWithMeta(row.element, 'dragstart')
        await dispatchWithMeta(wrapper.find('[data-sprint-id="2"]').element, 'drop')
        await flushPromises()

        expect(api.sprintAdd).toHaveBeenCalledTimes(1)
        expect(api.sprintAdd.mock.calls[0]![0]).toMatchObject({ tasks: ['PRJ-001'], sprint: 2 })
        expect(api.sprintAdd.mock.calls[0]![0].force_single).toBeUndefined()
        // Copy mode never removes the source membership.
        expect(api.sprintRemove).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('in Manual mode, move-dropping a multi-membership task onto one of its sprints still trims via force_single', async () => {
        api.listTasks.mockResolvedValue({
            status: 'ok',
            count: 1,
            total: 1,
            limit: 200,
            offset: 0,
            tasks: [makeTask('PRJ-001', [1, 2])],
        })
        const wrapper = await mountSettled()

        await wrapper.find('[data-sprint-id="1"] [data-testid="sprint-manual-order"]').trigger('click')

        api.sprintAdd.mockResolvedValueOnce(sprintResponse({
            action: 'add', sprint_id: 1, sprint_label: 'Alpha', modified: ['PRJ-001'],
        }))

        const row = wrapper.find('tr.task-row[data-task-id="PRJ-001"]')
        await row.trigger('dragstart')
        await wrapper.find('[data-sprint-id="1"]').trigger('drop')
        await flushPromises()

        // Not a no-op: force_single would drop sprint 2, so the request goes out.
        expect(api.sprintAdd).toHaveBeenCalledTimes(1)
        expect(api.sprintAdd.mock.calls[0]![0]).toMatchObject({ sprint: 1, force_single: true })
        wrapper.unmount()
    })
})
