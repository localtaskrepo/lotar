import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';

const routeState: { query: Record<string, any>; hash: string } = { query: {}, hash: '' }

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
        showConfig: vi.fn(async () => ({})),
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
        emits: ['update:value'],
        template: '<div class="filter-bar" />',
    },
}))

vi.mock('../components/analytics/SprintAnalyticsDialog.vue', () => ({
    default: { template: '<div class="analytics" />' },
}))

const sprintsRef = ref<any[]>([])

vi.mock('../composables/useSprints', () => ({
    useSprintFilterOptions: () => ref([]),
    useSprints: () => ({
        sprints: sprintsRef,
        loading: ref(false),
        refresh: vi.fn(async () => { }),
        missingSprints: ref<any[]>([]),
        hasMissing: ref(false),
    }),
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

function makeTask(idNum: number, sprints: number[] = []) {
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
        sprints,
        references: [],
        history: [],
    }
}

function makeSprint(id: number) {
    return {
        id,
        label: `Sprint ${id}`,
        display_name: `Sprint ${id}`,
        state: 'active',
        planned_start: null,
        planned_end: null,
        actual_start: null,
        actual_end: null,
        created: '2026-01-01T00:00:00Z',
    }
}

function sprintTable(wrapper: ReturnType<typeof mount>, sprintId: number) {
    const card = wrapper.find(`[data-sprint-id="${sprintId}"]`)
    return card.findAll('tbody tr.task-row')
}

function sprintPager(wrapper: ReturnType<typeof mount>, sprintId: number) {
    return wrapper.find(`nav[aria-label="Sprint #${sprintId} pagination"]`)
}

describe('SprintsList pagination', () => {
    beforeEach(async () => {
        routeState.query = {}
        routeState.hash = ''
        sprintsRef.value = []

        localStorage.clear()
        localStorage.setItem('lotar.sprints.sort', JSON.stringify({ key: 'id', dir: 'asc' }))

        const { api } = await import('../api/client')
            ; (api.listTasks as any).mockReset()
        ; (api.listTasks as any).mockResolvedValue({
            status: 'ok',
            count: 120,
            total: 120,
            limit: 200,
            offset: 0,
            tasks: Array.from({ length: 120 }, (_, idx) => makeTask(idx + 1)),
        })
    })

    it('shows the first page of backlog tasks using the tasks page-size preference', async () => {
        const wrapper = mount(SprintsList)
        await flushPromises()

        const rows = wrapper.findAll('#backlog tbody tr.task-row')
        expect(rows).toHaveLength(50)
        expect(rows[0]!.text()).toContain('1')
        // The backlog pager lives in the footer below the table, never in the header.
        expect(wrapper.find('#backlog nav[aria-label="Backlog pagination"]').exists()).toBe(true)
        expect(wrapper.find('#backlog .group-count button').exists()).toBe(false)
        wrapper.unmount()
    })

    it('pages forward through backlog tasks via the shared footer pager', async () => {
        const wrapper = mount(SprintsList)
        await flushPromises()

        const next = wrapper.find('#backlog nav[aria-label="Backlog pagination"] button[aria-label="Next page"]')
        expect(next.exists()).toBe(true)

        await next.trigger('click')
        await flushPromises()

        const rows = wrapper.findAll('#backlog tbody tr.task-row')
        expect(rows).toHaveLength(50)
        expect(rows[0]!.text()).toContain('51')

        // Numbered jumps use the same footer pager.
        await wrapper.find('#backlog nav[aria-label="Backlog pagination"] button[aria-label="Page 1"]').trigger('click')
        await flushPromises()
        expect(wrapper.findAll('#backlog tbody tr.task-row')[0]!.text()).toContain('1')
        wrapper.unmount()
    })

    it('keeps per-sprint offsets independent from each other and the backlog', async () => {
        sprintsRef.value = [makeSprint(1), makeSprint(2)]
        const { api } = await import('../api/client')
        const tasks = [
            ...Array.from({ length: 120 }, (_, idx) => makeTask(idx + 1, [1])),
            ...Array.from({ length: 60 }, (_, idx) => makeTask(200 + idx, [2])),
            ...Array.from({ length: 70 }, (_, idx) => makeTask(400 + idx, [])),
        ]
        ; (api.listTasks as any).mockResolvedValue({
            status: 'ok', count: tasks.length, total: tasks.length, limit: 200, offset: 0, tasks,
        })

        const wrapper = mount(SprintsList)
        await flushPromises()

        expect(sprintTable(wrapper, 1)).toHaveLength(50)
        expect(sprintTable(wrapper, 2)).toHaveLength(50)
        expect(wrapper.findAll('#backlog tbody tr.task-row')).toHaveLength(50)
        expect(sprintPager(wrapper, 2).exists()).toBe(true)

        await sprintPager(wrapper, 1).find('button[aria-label="Page 2"]').trigger('click')
        await flushPromises()

        // Only sprint #1 advanced; sprint #2 and the backlog stay on page 1.
        expect(sprintTable(wrapper, 1)[0]!.text()).toContain('051')
        expect(sprintTable(wrapper, 2)[0]!.text()).toContain('200')
        expect(wrapper.findAll('#backlog tbody tr.task-row')[0]!.text()).toContain('400')

        await sprintPager(wrapper, 2).find('button[aria-label="Page 2"]').trigger('click')
        await flushPromises()
        expect(sprintTable(wrapper, 2)[0]!.text()).toContain('250')
        expect(sprintTable(wrapper, 1)[0]!.text()).toContain('051')
        wrapper.unmount()
    })

    it('shows a multi-membership task in every sprint list it belongs to', async () => {
        sprintsRef.value = [makeSprint(1), makeSprint(2)]
        const { api } = await import('../api/client')
        const tasks = [
            ...Array.from({ length: 10 }, (_, idx) => makeTask(idx + 1, [1])),
            makeTask(50, [1, 2]),
            ...Array.from({ length: 10 }, (_, idx) => makeTask(100 + idx, [2])),
        ]
        ; (api.listTasks as any).mockResolvedValue({
            status: 'ok', count: tasks.length, total: tasks.length, limit: 200, offset: 0, tasks,
        })

        const wrapper = mount(SprintsList)
        await flushPromises()

        const inOne = sprintTable(wrapper, 1).map((row) => row.attributes('data-task-id'))
        const inTwo = sprintTable(wrapper, 2).map((row) => row.attributes('data-task-id'))
        expect(inOne).toContain('PRJ-050')
        expect(inTwo).toContain('PRJ-050')
        expect(inOne).toHaveLength(11)
        expect(inTwo).toHaveLength(11)
        // Single page for both sprints: no pager is rendered.
        expect(sprintPager(wrapper, 1).exists()).toBe(false)
        expect(sprintPager(wrapper, 2).exists()).toBe(false)
        wrapper.unmount()
    })

    it('resets every offset when the shared page-size preference changes', async () => {
        localStorage.setItem('lotar.preferences.tasks.pageSize', '25')
        const wrapper = mount(SprintsList)
        await flushPromises()
        expect((wrapper.vm as any).tasksPageSize).toBe(25)
        expect(wrapper.findAll('#backlog tbody tr.task-row')).toHaveLength(25)

        await wrapper.find('#backlog nav[aria-label="Backlog pagination"] button[aria-label="Page 2"]').trigger('click')
        await flushPromises()
        expect((wrapper.vm as any).backlogOffset).toBe(25)

        localStorage.setItem('lotar.preferences.tasks.pageSize', '50')
        window.dispatchEvent(new CustomEvent('lotar:preferences-changed', { detail: { key: 'lotar.preferences.tasks.pageSize' } }))
        await flushPromises()

        expect((wrapper.vm as any).tasksPageSize).toBe(50)
        expect((wrapper.vm as any).backlogOffset).toBe(0)
        expect(wrapper.findAll('#backlog tbody tr.task-row')).toHaveLength(50)
        wrapper.unmount()
    })

    it('clamps the backlog offset when the task list shrinks below the current page', async () => {
        const wrapper = mount(SprintsList)
        await flushPromises()

        await wrapper.find('#backlog nav[aria-label="Backlog pagination"] button[aria-label="Page 3"]').trigger('click')
        await flushPromises()
        expect((wrapper.vm as any).backlogOffset).toBe(100)

        const vm = wrapper.vm as any
        vm.tasks = vm.tasks.slice(0, 30)
        await flushPromises()

        expect(vm.backlogOffset).toBe(0)
        expect(wrapper.findAll('#backlog tbody tr.task-row')).toHaveLength(30)
        expect(wrapper.find('#backlog nav[aria-label="Backlog pagination"]').exists()).toBe(false)
        wrapper.unmount()
    })

    it('resets offsets when filters change or the sort order flips', async () => {
        sprintsRef.value = [makeSprint(1)]
        const { api } = await import('../api/client')
        const tasks = [
            ...Array.from({ length: 120 }, (_, idx) => makeTask(idx + 1, [1])),
            ...Array.from({ length: 120 }, (_, idx) => makeTask(900 + idx, [])),
        ]
        ; (api.listTasks as any).mockResolvedValue({
            status: 'ok', count: tasks.length, total: tasks.length, limit: 200, offset: 0, tasks,
        })

        const wrapper = mount(SprintsList)
        await flushPromises()

        await sprintPager(wrapper, 1).find('button[aria-label="Page 2"]').trigger('click')
        await wrapper.find('#backlog nav[aria-label="Backlog pagination"] button[aria-label="Page 2"]').trigger('click')
        await flushPromises()
        const vm = wrapper.vm as any
        expect(vm.backlogOffset).toBe(50)
        expect(vm.sprintTaskOffsets[1]).toBe(50)

        vm.filter = { status: 'open' }
        await flushPromises()
        expect(vm.backlogOffset).toBe(0)
        expect(vm.sprintTaskOffsets[1]).toBeUndefined()

        await sprintPager(wrapper, 1).find('button[aria-label="Page 3"]').trigger('click')
        await flushPromises()
        expect(vm.sprintTaskOffsets[1]).toBe(100)

        vm.sort.key = 'title'
        await flushPromises()
        expect(vm.sprintTaskOffsets[1]).toBeUndefined()
        wrapper.unmount()
    })

    it('evicts sprint offsets when the sprint leaves the visible window', async () => {
        sprintsRef.value = [makeSprint(1), makeSprint(2)]
        const { api } = await import('../api/client')
        const tasks = [
            ...Array.from({ length: 120 }, (_, idx) => makeTask(idx + 1, [1])),
            ...Array.from({ length: 120 }, (_, idx) => makeTask(300 + idx, [2])),
        ]
        ; (api.listTasks as any).mockResolvedValue({
            status: 'ok', count: tasks.length, total: tasks.length, limit: 200, offset: 0, tasks,
        })

        const wrapper = mount(SprintsList)
        await flushPromises()

        await sprintPager(wrapper, 2).find('button[aria-label="Page 2"]').trigger('click')
        await flushPromises()
        const vm = wrapper.vm as any
        expect(vm.sprintTaskOffsets[2]).toBe(50)

        sprintsRef.value = [makeSprint(1)]
        await flushPromises()

        expect(vm.sprintTaskOffsets[2]).toBeUndefined()
        wrapper.unmount()
    })

    it('does not issue requests when a sprint page is selected', async () => {
        sprintsRef.value = [makeSprint(1)]
        const { api } = await import('../api/client')
        const tasks = Array.from({ length: 120 }, (_, idx) => makeTask(idx + 1, [1]))
        ; (api.listTasks as any).mockResolvedValue({
            status: 'ok', count: 120, total: 120, limit: 200, offset: 0, tasks,
        })

        const wrapper = mount(SprintsList)
        await flushPromises()
        ; (api.listTasks as any).mockClear()

        await sprintPager(wrapper, 1).find('button[aria-label="Page 2"]').trigger('click')
        await flushPromises()

        expect((api.listTasks as any)).not.toHaveBeenCalled()
        expect(sprintTable(wrapper, 1)).toHaveLength(50)
        expect(sprintTable(wrapper, 1)[0]!.text()).toContain('051')
        wrapper.unmount()
    })
})
