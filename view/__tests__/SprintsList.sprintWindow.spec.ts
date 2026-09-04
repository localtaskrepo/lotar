import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';

const routeState = vi.hoisted(() => ({ query: {} as Record<string, any>, hash: '' }))
const sprintsState = vi.hoisted(() => ({
    sprintsRef: null as null | { value: any[] },
}))
const tasksState = {
    tasksRef: null as null | { value: any[] },
}

vi.mock('vue-router', () => ({
    useRoute: () => routeState,
    useRouter: () => ({
        push: vi.fn(),
        replace: vi.fn(),
    }),
}))

vi.mock('../api/client', () => ({
    api: {
        listTasks: vi.fn(),
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

vi.mock('../components/UiInput.vue', () => ({
    default: {
        props: ['modelValue'],
        emits: ['update:modelValue'],
        template: '<input class="input" :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
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

vi.mock('../components/ColumnsMenu.vue', () => ({
    default: {
        props: ['open', 'options', 'isVisible', 'setVisible', 'label'],
        emits: ['update:open', 'reset'],
        template: '<div class="columns-menu"><slot name="trigger" :open="open" :toggle="() => $emit(\'update:open\', !open)" /></div>',
    },
}))

vi.mock('../components/analytics/SprintAnalyticsDialog.vue', () => ({
    default: { template: '<div class="analytics" />' },
}))

vi.mock('../composables/useSprints', async () => {
    const vue = await import('vue')
    sprintsState.sprintsRef ??= vue.ref<any[]>([])
    return {
        useSprints: () => ({
            sprints: sprintsState.sprintsRef!,
            loading: vue.ref(false),
            refresh: vi.fn(async () => { }),
            missingSprints: vue.ref<any[]>([]),
            hasMissing: vue.ref(false),
        }),
    }
})

vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({
        openTaskPanel: vi.fn(),
    }),
}))

vi.mock('../composables/useConfig', () => ({
    useConfig: () => ({
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

vi.mock('../composables/useCopyModifier', () => ({
    useCopyModifier: () => ({
        copyModifierActive: ref(false),
        resolveCopyModifier: vi.fn(() => false),
        resetCopyModifier: vi.fn(),
        bindCopyModifierListeners: vi.fn(),
        unbindCopyModifierListeners: vi.fn(),
    }),
}))

import SprintsList from '../pages/SprintsList.vue';

function makeTask(idNum: number, overrides: Record<string, any> = {}) {
    const padded = String(idNum).padStart(3, '0')
    return {
        id: `PRJ-${padded}`,
        title: `Task ${padded}`,
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
        sprints: [] as number[],
        references: [],
        history: [],
        ...overrides,
    }
}

function makeSprint(id: number, overrides: Record<string, any> = {}) {
    return {
        id,
        label: `Sprint ${id}`,
        display_name: `Sprint ${id}`,
        state: 'active',
        created: '2026-01-01T00:00:00Z',
        modified: '2026-01-01T00:00:00Z',
        tasks: [],
        ...overrides,
    }
}

import { api } from '../api/client'

function setSprintTasks(tasks: any[]) {
    ;(api.listTasks as any).mockResolvedValue({
        status: 'ok',
        count: tasks.length,
        total: tasks.length,
        limit: 200,
        offset: 0,
        tasks,
    })
}

describe('SprintsList sprint window', () => {
    beforeEach(async () => {
        routeState.query = {}
        routeState.hash = ''
        localStorage.clear()
        localStorage.setItem('lotar.sprints.sort', JSON.stringify({ key: 'id', dir: 'asc' }))
        sprintsState.sprintsRef!.value = []

        const { api } = await import('../api/client')
        ;(api.listTasks as any).mockReset()
        setSprintTasks([])
    })

    it('renders one section per sprint with its assigned tasks', async () => {
        sprintsState.sprintsRef!.value = [
            makeSprint(1),
            makeSprint(2),
        ]
        setSprintTasks([
            makeTask(1, { sprints: [1] }),
            makeTask(2, { sprints: [1] }),
            makeTask(3, { sprints: [2] }),
        ])

        const wrapper = mount(SprintsList)
        await flushPromises()

        const groups = wrapper.findAll('.sprint-group:not(.backlog-group)')
        expect(groups).toHaveLength(2)
        expect(groups[0]!.text()).toContain('Sprint 1')
        expect(groups[0]!.findAll('tbody tr.task-row')).toHaveLength(2)
        expect(groups[1]!.findAll('tbody tr.task-row')).toHaveLength(1)

        wrapper.unmount()
    })

    it('badges tasks that belong to multiple sprints when the preference is enabled', async () => {
        localStorage.setItem('lotar.sprints.highlightMultiSprint', 'true')
        sprintsState.sprintsRef!.value = [makeSprint(1), makeSprint(2)]
        setSprintTasks([
            makeTask(1, { sprints: [1, 2] }),
            makeTask(2, { sprints: [1] }),
        ])

        const wrapper = mount(SprintsList)
        await flushPromises()

        const badges = wrapper.findAll('.task-title__badge')
        // The multi-sprint task appears in both sprint sections, each row badged.
        expect(badges).toHaveLength(2)
        expect(badges[0]!.text()).toContain('Multi')

        wrapper.unmount()
    })

    it('hides complete sprints older than the selected window and reports the hidden count', async () => {
        localStorage.setItem('lotar.sprints.window.v2', '30')
        const old = new Date(Date.now() - 90 * 24 * 60 * 60 * 1000).toISOString()
        sprintsState.sprintsRef!.value = [
            makeSprint(1, { state: 'complete', actual_end: old }),
            makeSprint(2, { state: 'active' }),
        ]
        setSprintTasks([
            makeTask(1, { sprints: [1] }),
            makeTask(2, { sprints: [2] }),
        ])

        const wrapper = mount(SprintsList)
        await flushPromises()

        const groups = wrapper.findAll('.sprint-group:not(.backlog-group)')
        expect(groups).toHaveLength(1)
        expect(groups[0]!.text()).toContain('Sprint 2')

        const hint = wrapper.find('.hint')
        expect(hint.exists()).toBe(true)
        expect(hint.text()).toContain('1 completed sprint')

        wrapper.unmount()
    })

    it('shows all sprints in the all-time window', async () => {
        localStorage.setItem('lotar.sprints.window.v2', 'all')
        const old = new Date(Date.now() - 400 * 24 * 60 * 60 * 1000).toISOString()
        sprintsState.sprintsRef!.value = [
            makeSprint(1, { state: 'complete', actual_end: old }),
            makeSprint(2, { state: 'active' }),
        ]
        setSprintTasks([
            makeTask(1, { sprints: [1] }),
            makeTask(2, { sprints: [2] }),
        ])

        const wrapper = mount(SprintsList)
        await flushPromises()

        const groups = wrapper.findAll('.sprint-group:not(.backlog-group)')
        expect(groups).toHaveLength(2)

        wrapper.unmount()
    })

    it('renders unassigned tasks in the backlog group', async () => {
        sprintsState.sprintsRef!.value = [makeSprint(1)]
        setSprintTasks([
            makeTask(1, { sprints: [1] }),
            makeTask(2, { sprints: [] }),
        ])

        const wrapper = mount(SprintsList)
        await flushPromises()

        const backlog = wrapper.find('.sprint-group.backlog-group')
        expect(backlog.exists()).toBe(true)
        expect(backlog.findAll('tbody tr.task-row')).toHaveLength(1)

        wrapper.unmount()
    })
})
