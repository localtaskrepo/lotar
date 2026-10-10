import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { invalidateCompletionPolicies } from '../composables/useCompletionPolicy'

const routeState: { query: Record<string, any>; hash: string } = { query: {}, hash: '' }

vi.mock('vue-router', () => ({
    useRoute: () => routeState,
    useRouter: () => ({
        push: vi.fn(),
        replace: vi.fn(),
    }),
}))

const showConfigMock = vi.hoisted(() => vi.fn(async () => ({})))

vi.mock('../api/client', () => ({
    api: {
        listTasks: vi.fn(),
        showConfig: showConfigMock,
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
        props: ['value', 'statuses', 'priorities', 'types', 'customPresets', 'emitProjectKey', 'storageKey'],
        emits: ['update:value'],
        template: `
          <div class="filter-bar">
            <button type="button" class="set-due-overdue" @click="$emit('update:value', { due: 'overdue' })">od</button>
            <button type="button" class="set-due-today" @click="$emit('update:value', { due: 'today' })">td</button>
          </div>
        `,
    },
}))

vi.mock('../components/analytics/SprintAnalyticsDialog.vue', () => ({
    default: { template: '<div class="analytics" />' },
}))

vi.mock('../components/ColumnsMenu.vue', () => ({
    default: { template: '<div class="columns-menu"><slot name="trigger" /></div>' },
}))

vi.mock('../components/SprintViewSettings.vue', () => ({
    default: { props: ['timeRange', 'choices', 'showAllowClosed', 'allowClosed', 'highlightMultiSprint'], template: '<div class="view-settings" />' },
}))

vi.mock('../composables/useSprints', () => ({
    useSprintFilterOptions: () => ref([]),
    useSprints: () => ({
        sprints: ref<any[]>([]),
        loading: ref(false),
        refresh: vi.fn(async () => { }),
        missingSprints: ref<any[]>([]),
        hasMissing: ref(false),
    }),
}))

vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({
        openTaskPanel: vi.fn(),
    }),
}))

vi.mock('../composables/useConfig', () => ({
    useConfig: () => ({
        scope: ref(''), tags: ref<string[]>([]), members: ref<string[]>([]),
        sprintDefaults: ref<any>({}),
        statuses: ref<string[]>(['Todo', 'Doing', 'Done', 'Closed']),
        priorities: ref<string[]>(['Low', 'Med', 'High']),
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
    }),
}))

vi.mock('../composables/useCopyModifier', () => ({
    useCopyModifier: () => ({
        copyModifierActive: ref(false),
        resolveCopyModifier: vi.fn(() => false),
        resetCopyModifier: vi.fn(),
        bindCopyModifierListeners: vi.fn(),
        unbindCopyModifierListeners: vi.fn(),
    }),
}))

import SprintsList from '../pages/SprintsList.vue'
import { api } from '../api/client'

function makeTask(id: string, overrides: Record<string, unknown> = {}) {
    return {
        id,
        title: `Task ${id}`,
        status: 'Todo',
        priority: 'Med',
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
        sprints: [],
        references: [],
        history: [],
        ...overrides,
    }
}

async function mountSprints() {
    const wrapper = mount(SprintsList)
    await flushPromises()
    return wrapper
}

describe('SprintsList due smart filters and row styling (DEV-21)', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = {}
        routeState.hash = ''
        localStorage.clear()
        showConfigMock.mockReset()
        showConfigMock.mockImplementation(async () => ({
            issue_states: ['Todo', 'Doing', 'Done', 'Closed'],
            effective_done_states: ['Done', 'Closed'],
            done_states_mode: 'explicit',
            task_calendar_day: '2026-01-05',
        }))
        invalidateCompletionPolicies()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('the overdue filter excludes terminal tasks and styles only real overdue rows', async () => {
        const tasks = [
            makeTask('PRJ-001', { status: 'Todo', due_date: '2025-12-01' }),
            makeTask('PRJ-002', { status: 'Done', due_date: '2025-12-01' }),   // terminal past due: excluded
            makeTask('PRJ-003', { status: 'Closed', due_date: '2025-12-01' }), // custom terminal: excluded
            makeTask('PRJ-004', { status: 'Todo', due_date: '2026-01-05' }),   // due today: not overdue
        ]
        vi.mocked(api.listTasks).mockResolvedValue({
            status: 'ok',
            count: tasks.length,
            total: tasks.length,
            limit: 200,
            offset: 0,
            tasks,
        } as any)

        const wrapper = await mountSprints()

        // No due filter: all rows render; only the open past-due row is red.
        let rows = wrapper.findAll('#backlog tbody tr.task-row')
        expect(rows).toHaveLength(4)
        const overdueCell = rows.find(r => r.text().includes('PRJ-001'))!.find('.text-overdue')
        expect(overdueCell.exists()).toBe(true)
        const doneCell = rows.find(r => r.text().includes('PRJ-002'))!.find('.text-overdue')
        expect(doneCell.exists()).toBe(false)

        // Switch the due filter to overdue through the FilterBar contract.
        await wrapper.find('.set-due-overdue').trigger('click')
        await vi.advanceTimersByTimeAsync(400)
        await flushPromises()

        rows = wrapper.findAll('#backlog tbody tr.task-row')
        expect(rows).toHaveLength(1)
        expect(rows[0]!.text()).toContain('PRJ-001')
        wrapper.unmount()
    })

    it('the today filter keeps terminal tasks that are due today', async () => {
        const tasks = [
            makeTask('PRJ-001', { status: 'Todo', due_date: '2026-01-05' }),
            makeTask('PRJ-002', { status: 'Done', due_date: '2026-01-05' }), // done due today still counts
        ]
        vi.mocked(api.listTasks).mockResolvedValue({
            status: 'ok',
            count: tasks.length,
            total: tasks.length,
            limit: 200,
            offset: 0,
            tasks,
        } as any)

        const wrapper = await mountSprints()

        await wrapper.find('.set-due-today').trigger('click')
        await vi.advanceTimersByTimeAsync(400)
        await flushPromises()

        const rows = wrapper.findAll('#backlog tbody tr.task-row')
        expect(rows).toHaveLength(2)
        wrapper.unmount()
    })

    it('renders without a resolved policy (fail closed) using task_state metadata', async () => {
        showConfigMock.mockResolvedValue({})
        const tasks = [
            makeTask('PRJ-001', { status: 'Done', due_date: '2025-12-01', task_state: { is_done: true, due_bucket: null, calendar_day: '2026-01-05' } }),
            makeTask('PRJ-002', { status: 'Todo', due_date: '2025-12-01', task_state: { is_done: false, due_bucket: 'overdue', calendar_day: '2026-01-05' } }),
        ]
        vi.mocked(api.listTasks).mockResolvedValue({
            status: 'ok',
            count: tasks.length,
            total: tasks.length,
            limit: 200,
            offset: 0,
            tasks,
        } as any)

        const wrapper = await mountSprints()

        const rows = wrapper.findAll('#backlog tbody tr.task-row')
        expect(rows.find(r => r.text().includes('PRJ-001'))!.find('.text-overdue').exists()).toBe(false)
        expect(rows.find(r => r.text().includes('PRJ-002'))!.find('.text-overdue').exists()).toBe(true)
        wrapper.unmount()
    })
})
