import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';
import type { TaskDTO } from '../api/types';

enableAutoUnmount(afterEach)
const orderEvents = vi.hoisted(() => ({ handlers: new Map<string, (event: MessageEvent) => void>(), close: vi.fn() }))
vi.mock('../composables/useSse', () => ({
    useSse: () => ({
        on: (kind: string, handler: (event: MessageEvent) => void) => orderEvents.handlers.set(kind, handler),
        close: orderEvents.close,
    }),
}))

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
        getBacklogOrder: vi.fn(),
        reorderTasks: vi.fn(),
        sprintAdd: vi.fn(),
        sprintRemove: vi.fn(),
        sprintUpdate: vi.fn(),
        updateTask: vi.fn(),
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

function dragTransfer() {
    if (typeof DataTransfer === 'function') return new DataTransfer()
    const data = new Map<string, string>()
    return {
        effectAllowed: 'uninitialized', dropEffect: 'none',
        setData: (type: string, value: string) => data.set(type, value),
        getData: (type: string) => data.get(type) ?? '',
        get types() { return [...data.keys()] },
    } as unknown as DataTransfer
}

function dragEvent(element: Element, kind: string, transfer: DataTransfer, clientY = -1, ctrlKey = false) {
    const event = typeof DragEvent === 'function'
        ? new DragEvent(kind, { bubbles: true, cancelable: true, dataTransfer: transfer, clientY, ctrlKey })
        : new Event(kind, { bubbles: true, cancelable: true })
    Object.defineProperties(event, { dataTransfer: { value: transfer }, clientY: { value: clientY }, ctrlKey: { value: ctrlKey } })
    element.dispatchEvent(event)
    return event
}

describe('SprintsList pagination', () => {
    beforeEach(async () => {
        routeState.query = {}
        routeState.hash = ''
        sprintsRef.value = []

        localStorage.clear()
        localStorage.setItem('lotar.sprints.sort', JSON.stringify({ key: 'id', dir: 'asc' }))

        const { api } = await import('../api/client')
        orderEvents.handlers.clear()
        vi.mocked(api.getBacklogOrder).mockReset().mockResolvedValue({ status: 'ok', tasks: [], changed: false })
        vi.mocked(api.reorderTasks).mockReset().mockResolvedValue({ status: 'ok', tasks: [], changed: false })
        vi.mocked(api.sprintAdd).mockReset()
        vi.mocked(api.sprintRemove).mockReset()
        vi.mocked(api.sprintUpdate).mockReset()
        vi.mocked(api.updateTask).mockReset()
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

    it('applies persisted manual backlog order before paging without resetting sprint pages', async () => {
        sprintsRef.value = [makeSprint(1)]
        const { api } = await import('../api/client')
        const tasks: TaskDTO[] = [
            ...Array.from({ length: 120 }, (_, index) => makeTask(index + 1)),
            ...Array.from({ length: 60 }, (_, index) => makeTask(200 + index, [1])),
        ]
        vi.mocked(api.getBacklogOrder).mockResolvedValue({ status: 'ok', tasks: ['PRJ-120', 'PRJ-118', 'PRJ-119'], changed: false })
        ; (api.listTasks as any).mockResolvedValue({
            status: 'ok', count: tasks.length, total: tasks.length, limit: 200, offset: 0, tasks,
        })
        let wrapper = mount(SprintsList)
        try {
            await flushPromises()
            await sprintPager(wrapper, 1).find('button[aria-label="Page 2"]').trigger('click')
            await wrapper.find('#backlog button[aria-label="Page 2"]').trigger('click')
            await flushPromises()
            expect(sprintTable(wrapper, 1)[0]!.attributes('data-task-id')).toBe('PRJ-250')
            expect(wrapper.find('#backlog tbody tr').attributes('data-task-id')).toBe('PRJ-051')

            await wrapper.find('[data-testid="backlog-manual-order"]').trigger('click')
            await flushPromises()
            const rows = wrapper.findAll('#backlog tbody tr')
            expect(rows).toHaveLength(50)
            expect(rows.slice(0, 4).map(row => row.attributes('data-task-id'))).toEqual([
                'PRJ-120', 'PRJ-118', 'PRJ-119', 'PRJ-001',
            ])
            expect(sprintTable(wrapper, 1)[0]!.attributes('data-task-id')).toBe('PRJ-250')
            expect(wrapper.find('#backlog th[data-column="id"]').attributes('aria-sort')).toBe('none')
            await wrapper.find('#backlog button[aria-label="Page 2"]').trigger('click')
            await flushPromises()
            expect(wrapper.find('#backlog tbody tr').attributes('data-task-id')).toBe('PRJ-048')

            wrapper.unmount()
            wrapper = mount(SprintsList)
            await flushPromises()
            expect(wrapper.find('[data-testid="backlog-manual-order"]').attributes('aria-pressed')).toBe('true')
            expect(wrapper.find('#backlog tbody tr').attributes('data-task-id')).toBe('PRJ-120')
            await wrapper.find('#backlog th[data-column="title"] button').trigger('click')
            await flushPromises()
            expect(wrapper.find('[data-testid="backlog-manual-order"]').attributes('aria-pressed')).toBe('false')
            expect(wrapper.find('#backlog tbody tr').attributes('data-task-id')).toBe('PRJ-001')
        } finally {
            wrapper.unmount()
        }
    })

    it('matches canonical ID ordering for Unicode project prefixes', async () => {
        const { api } = await import('../api/client')
        const tasks = [
            { ...makeTask(1), id: '\u{10400}-1' },
            { ...makeTask(2), id: '\u{F900}-2' },
        ]
        ; (api.listTasks as any).mockResolvedValue({ status: 'ok', tasks, count: 2, total: 2, limit: 200, offset: 0 })
        const wrapper = mount(SprintsList)
        try {
            await flushPromises()
            await wrapper.find('[data-testid="backlog-manual-order"]').trigger('click')
            await flushPromises()
            expect(wrapper.findAll('#backlog tbody tr').map(row => row.attributes('data-task-id'))).toEqual([
                '\u{F900}-2', '\u{10400}-1',
            ])
        } finally { wrapper.unmount() }
    })

    it('projects global order onto the current result and refreshes without mutating membership', async () => {
        const { api } = await import('../api/client')
        const tasks: TaskDTO[] = [
            makeTask(9),
            makeTask(2),
            makeTask(6),
            makeTask(1, [1]),
        ]
        vi.mocked(api.getBacklogOrder).mockResolvedValue({ status: 'ok', tasks: ['HIDDEN-1', 'PRJ-002', 'PRJ-009'], changed: false })
        ; (api.listTasks as any).mockResolvedValue({
            status: 'ok', count: tasks.length, total: tasks.length, limit: 200, offset: 0, tasks,
        })
        const wrapper = mount(SprintsList)
        try {
            await flushPromises()
            await wrapper.find('[data-testid="backlog-manual-order"]').trigger('click')
            await flushPromises()
            const ids = () => wrapper.findAll('#backlog tbody tr').map(row => row.attributes('data-task-id'))
            expect(ids()).toEqual(['PRJ-002', 'PRJ-009', 'PRJ-006'])
            vi.mocked(api.getBacklogOrder).mockResolvedValue({ status: 'ok', tasks: ['PRJ-009', 'HIDDEN-1', 'PRJ-002'], changed: false })
            await wrapper.find('button.reload').trigger('click')
            await flushPromises()
            expect(ids()).toEqual(['PRJ-009', 'PRJ-002', 'PRJ-006'])
            expect(tasks[3]!.sprints).toEqual([1])
        } finally {
            wrapper.unmount()
        }
    })

    it.each([null, 1])('blocks automatic same-list ordering without changing page or membership in container %s', async sprint => {
        const { api } = await import('../api/client')
        sprintsRef.value = sprint === null ? [] : [makeSprint(sprint)]
        const tasks = Array.from({ length: 120 }, (_, index) => makeTask(index + 1, sprint === null ? [] : [sprint]))
        vi.mocked(api.listTasks).mockResolvedValue({ tasks, total: 120, limit: 200, offset: 0 })
        const wrapper = mount(SprintsList)
        await flushPromises()
        const group = wrapper.find(sprint === null ? '#backlog' : `[data-sprint-id="${sprint}"]`)
        await group.find('button[aria-label="Page 2"]').trigger('click')
        const ids = () => group.findAll('tbody tr.task-row').map(row => row.attributes('data-task-id'))
        const before = ids()
        const mode = group.find(`[data-testid="${sprint === null ? 'backlog' : 'sprint'}-manual-order"]`)
        const rows = group.findAll('tbody tr.task-row')
        const message = 'Automatic sorting is on. Switch to Manual order to reorder tasks.'
        expect(rows[0]!.attributes('draggable')).toBe('true')
        expect(rows[0]!.attributes('title')).toBe(message)
        const transfer = dragTransfer()
        expect(dragEvent(rows[0]!.element, 'dragstart', transfer).defaultPrevented).toBe(false)
        await flushPromises()
        expect(rows[0]!.classes()).not.toContain('task-row--dragging')
        expect(document.body.querySelector('#task-order-tooltip[role="tooltip"]')?.textContent).toBe(message)
        await rows[0]!.trigger('mouseleave')
        await flushPromises()
        expect(document.body.querySelector('#task-order-tooltip')).toBeNull()
        dragEvent(rows[1]!.element, 'dragover', transfer)
        await flushPromises()
        expect(rows[1]!.classes()).not.toContain('task-row--before')
        expect(rows[1]!.classes()).not.toContain('task-row--after')
        expect(document.body.querySelector('#task-order-tooltip')?.textContent).toBe(message)
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
        await flushPromises()
        expect(document.body.querySelector('#task-order-tooltip')).toBeNull()
        dragEvent(rows[1]!.element, 'drop', transfer)
        await flushPromises()
        expect(document.body.querySelector('#task-order-tooltip')?.textContent).toBe(message)
        expect(ids()).toEqual(before)
        expect(mode.attributes('aria-pressed')).toBe('false')
        expect(api.reorderTasks).not.toHaveBeenCalled()
        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.sprintRemove).not.toHaveBeenCalled()
        expect(api.updateTask).not.toHaveBeenCalled()
        expect(tasks.every(task => JSON.stringify(task.sprints) === JSON.stringify(sprint === null ? [] : [sprint]))).toBe(true)
        expect(mode.text()).toContain('Manual order')
        expect(mode.find('.icon[name="list"]').exists()).toBe(true)
        expect(mode.find('.icon[name="list"]').attributes('aria-hidden')).toBe('true')
        await mode.trigger('click')
        await flushPromises()
        expect(mode.attributes('aria-pressed')).toBe('true')
        expect(document.body.querySelector('#task-order-tooltip')).toBeNull()
        await mode.trigger('click')
        dragEvent(group.find('tbody tr.task-row').element, 'dragstart', dragTransfer())
        await flushPromises()
        wrapper.unmount()
        expect(document.body.querySelector('#task-order-tooltip')).toBeNull()
    })

    it.each([null, 1])('blocks automatic same-container background drops without collapsing memberships in container %s', async sprint => {
        const { api } = await import('../api/client')
        sprintsRef.value = [makeSprint(1), makeSprint(2)]
        const tasks = [makeTask(1, [1, 2]), makeTask(2, [1]), makeTask(3)]
        const tasksBefore = JSON.parse(JSON.stringify(tasks))
        vi.mocked(api.listTasks).mockResolvedValue({ tasks, total: 3, limit: 200, offset: 0 })
        const wrapper = mount(SprintsList)
        await flushPromises()
        const group = wrapper.find(sprint === null ? '#backlog' : '[data-sprint-id="1"]')
        const source = group.find(`[data-task-id="${sprint === null ? 'PRJ-003' : 'PRJ-001'}"]`)
        const rowIds = () => ['#backlog', '[data-sprint-id="1"]', '[data-sprint-id="2"]'].map(selector =>
            wrapper.findAll(`${selector} tbody tr.task-row`).map(row => row.attributes('data-task-id')))
        const counts = () => ['#backlog', '[data-sprint-id="1"]', '[data-sprint-id="2"]'].map(selector =>
            wrapper.find(`${selector} .group-count`).text())
        const rowsBefore = rowIds()
        const countsBefore = counts()
        const transfer = dragTransfer()
        expect(source.attributes('draggable')).toBe('true')
        dragEvent(source.element, 'dragstart', transfer)
        await flushPromises()
        await source.trigger('mouseleave')
        await flushPromises()
        expect(document.body.querySelector('#task-order-tooltip')).toBeNull()

        // Dispatch on the region itself, not a row insertion target.
        dragEvent(group.element, 'drop', transfer, 42)
        await flushPromises()

        expect(document.body.querySelector('#task-order-tooltip[role="tooltip"]')?.textContent)
            .toBe('Automatic sorting is on. Switch to Manual order to reorder tasks.')
        expect(group.find(`[data-testid="${sprint === null ? 'backlog' : 'sprint'}-manual-order"]`).attributes('aria-pressed')).toBe('false')
        expect(rowIds()).toEqual(rowsBefore)
        expect(counts()).toEqual(countsBefore)
        expect(tasks).toEqual(tasksBefore)
        expect(tasks[0]!.sprints).toEqual([1, 2])
        expect(sprintTable(wrapper, 1).map(row => row.attributes('data-task-id'))).toEqual(['PRJ-001', 'PRJ-002'])
        expect(sprintTable(wrapper, 2).map(row => row.attributes('data-task-id'))).toEqual(['PRJ-001'])
        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.sprintRemove).not.toHaveBeenCalled()
        expect(api.reorderTasks).not.toHaveBeenCalled()
        expect(api.updateTask).not.toHaveBeenCalled()
        await group.trigger('mouseleave')
        await flushPromises()
        expect(document.body.querySelector('#task-order-tooltip')).toBeNull()
        dragEvent(source.element, 'dragstart', transfer)
        dragEvent(group.element, 'drop', transfer, 42)
        await flushPromises()
        expect(document.body.querySelector('#task-order-tooltip')).not.toBeNull()
        wrapper.unmount()
        expect(document.body.querySelector('#task-order-tooltip')).toBeNull()
    })

    it.each([null, 1])('only changes membership when moving into automatic container %s', async destination => {
        const { api } = await import('../api/client')
        sprintsRef.value = [makeSprint(1)]
        const tasks = [makeTask(1), makeTask(2, [1])]
        vi.mocked(api.listTasks).mockImplementation(async () => ({ tasks, total: 2, limit: 200, offset: 0 }))
        vi.mocked(api.sprintAdd).mockImplementation(async () => {
            tasks[0]!.sprints = [1]
            return { status: 'ok', modified: ['PRJ-001'], sprint_id: 1 } as any
        })
        vi.mocked(api.sprintRemove).mockImplementation(async () => {
            tasks[1]!.sprints = []
            return { status: 'ok', modified: ['PRJ-002'], sprint_id: 1 } as any
        })
        const wrapper = mount(SprintsList)
        await flushPromises()
        const source = wrapper.find(destination === null ? '[data-sprint-id="1"] tbody tr' : '#backlog tbody tr')
        const target = wrapper.find(destination === null ? '#backlog tbody tr' : '[data-sprint-id="1"] tbody tr')
        const transfer = dragTransfer()
        expect(source.attributes('draggable')).toBe('true')
        dragEvent(source.element, 'dragstart', transfer)
        dragEvent(target.element, 'dragover', transfer)
        await flushPromises()
        expect(target.classes()).not.toContain('task-row--before')
        expect(target.classes()).not.toContain('task-row--after')
        dragEvent(target.element, 'drop', transfer)
        await flushPromises()
        expect(destination === null ? api.sprintRemove : api.sprintAdd).toHaveBeenCalledTimes(1)
        expect(destination === null ? api.sprintAdd : api.sprintRemove).not.toHaveBeenCalled()
        expect(api.reorderTasks).not.toHaveBeenCalled()
        expect(api.updateTask).not.toHaveBeenCalled()
        expect(wrapper.find('[data-testid="backlog-manual-order"]').attributes('aria-pressed')).toBe('false')
        expect(wrapper.find('[data-testid="sprint-manual-order"]').attributes('aria-pressed')).toBe('false')
        expect(wrapper.findAll(destination === null ? '#backlog tbody tr.task-row' : '[data-sprint-id="1"] tbody tr.task-row')).toHaveLength(2)
    })

    it.each(['before', 'after'] as const)('drops %s a row using a server-relative anchor, not a visible permutation', async position => {
        const { api } = await import('../api/client')
        vi.mocked(api.getBacklogOrder).mockResolvedValue({ status: 'ok', tasks: ['HIDDEN-1', 'PRJ-052', 'HIDDEN-2', 'PRJ-051'], changed: false })
        vi.mocked(api.reorderTasks).mockResolvedValue({ status: 'ok', tasks: ['HIDDEN-1', 'PRJ-052', 'PRJ-051', 'HIDDEN-2'], changed: true })
        const wrapper = mount(SprintsList)
        await flushPromises()
        await wrapper.find('[data-testid="backlog-manual-order"]').trigger('click')
        await flushPromises()
        await wrapper.find('#backlog button[aria-label="Page 2"]').trigger('click')
        const source = wrapper.find('#backlog [data-task-id="PRJ-049"]')
        const target = wrapper.find('#backlog [data-task-id="PRJ-050"]')
        const transfer = dragTransfer()
        dragEvent(source.element, 'dragstart', transfer)
        const y = position === 'before' ? -1 : 1
        dragEvent(target.element, 'dragover', transfer, y)
        await flushPromises()
        expect(target.classes()).toContain(`task-row--${position}`)
        dragEvent(target.element, 'drop', transfer, y)
        await flushPromises()
        expect(api.reorderTasks).toHaveBeenCalledExactlyOnceWith({ task: 'PRJ-049', sprint: null, [position]: 'PRJ-050' })
        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.sprintRemove).not.toHaveBeenCalled()
        expect(api.updateTask).not.toHaveBeenCalled()
        expect(wrapper.find('[data-testid="backlog-manual-order"]').attributes('aria-pressed')).toBe('true')
        expect(wrapper.find('#backlog th[data-column="id"]').attributes('aria-sort')).toBe('none')
        expect(localStorage.getItem('lotar.sprints.sort')).toContain('"id"')
    })

    it.each([null, 1])('does not reinterpret a copy gesture as a reorder within container %s', async sprint => {
        const { api } = await import('../api/client')
        const { showToast } = await import('../components/toast')
        sprintsRef.value = sprint === null ? [] : [makeSprint(sprint)]
        const tasks = [makeTask(1, sprint === null ? [] : [sprint]), makeTask(2, sprint === null ? [] : [sprint])]
        vi.mocked(api.listTasks).mockResolvedValue({ tasks, total: 2, limit: 200, offset: 0 })
        const wrapper = mount(SprintsList)
        await flushPromises()
        await wrapper.find(sprint === null ? '[data-testid="backlog-manual-order"]' : `[data-sprint-id="${sprint}"] [data-testid="sprint-manual-order"]`).trigger('click')
        const rows = sprint === null ? wrapper.findAll('#backlog tbody tr') : sprintTable(wrapper, sprint)
        const transfer = dragTransfer()
        dragEvent(rows[0]!.element, 'dragstart', transfer, -1, true)
        dragEvent(rows[1]!.element, 'drop', transfer, -1, true)
        await flushPromises()
        expect(api.reorderTasks).not.toHaveBeenCalled()
        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.sprintRemove).not.toHaveBeenCalled()
        expect(showToast).toHaveBeenCalledWith('Release the copy modifier to reorder tasks within this list.')
    })

    it('reorders sprint vectors without membership writes and preserves counts and other manual modes', async () => {
        const { api } = await import('../api/client')
        sprintsRef.value = [makeSprint(1), makeSprint(2)]
        const tasks: TaskDTO[] = [
            { ...makeTask(1, [1, 2]), sprint_order: { 1: 1, 2: 2 } },
            { ...makeTask(2, [1, 2]), sprint_order: { 1: 2, 2: 1 } },
        ]
        vi.mocked(api.listTasks).mockResolvedValue({ tasks, total: 2, limit: 200, offset: 0 })
        vi.mocked(api.reorderTasks).mockResolvedValue({ status: 'ok', tasks: ['PRJ-002', 'PRJ-001'], changed: true, sprint_id: 1 })
        const wrapper = mount(SprintsList)
        await flushPromises()
        await wrapper.find('[data-sprint-id="2"] [data-testid="sprint-manual-order"]').trigger('click')
        await wrapper.find('[data-sprint-id="1"] [data-testid="sprint-manual-order"]').trigger('click')
        const transfer = dragTransfer()
        dragEvent(sprintTable(wrapper, 1)[1]!.element, 'dragstart', transfer)
        dragEvent(sprintTable(wrapper, 1)[0]!.element, 'drop', transfer)
        await flushPromises()
        expect(api.reorderTasks).toHaveBeenCalledExactlyOnceWith({ task: 'PRJ-002', sprint: 1, before: 'PRJ-001' })
        expect(sprintTable(wrapper, 1).map(row => row.attributes('data-task-id'))).toEqual(['PRJ-002', 'PRJ-001'])
        expect(wrapper.find('[data-sprint-id="1"] .group-count').text()).toBe('2 tasks')
        expect(wrapper.find('[data-sprint-id="2"] [data-testid="sprint-manual-order"]').attributes('aria-pressed')).toBe('true')
        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.sprintRemove).not.toHaveBeenCalled()
        expect(api.updateTask).not.toHaveBeenCalled()
        expect(JSON.parse(localStorage.getItem('lotar.sprints.manualOrder')!)).toEqual([2, 1])
    })

    it('stops same-row drops, accepts server no-ops, and reports stale-anchor errors without changing sort mode', async () => {
        const { api } = await import('../api/client')
        const { showToast } = await import('../components/toast')
        const wrapper = mount(SprintsList)
        await flushPromises()
        await wrapper.find('[data-testid="backlog-manual-order"]').trigger('click')
        const rows = wrapper.findAll('#backlog tbody tr')
        const transfer = dragTransfer()
        dragEvent(rows[0]!.element, 'dragstart', transfer)
        dragEvent(rows[0]!.element, 'drop', transfer)
        await flushPromises()
        expect(api.reorderTasks).not.toHaveBeenCalled()
        vi.mocked(api.reorderTasks).mockRejectedValueOnce(new Error('Anchor no longer belongs to backlog'))
        dragEvent(rows[0]!.element, 'dragstart', transfer)
        dragEvent(rows[1]!.element, 'drop', transfer)
        await flushPromises()
        expect(showToast).toHaveBeenCalledWith('Could not reorder tasks: Anchor no longer belongs to backlog')
        expect(wrapper.find('[data-testid="backlog-manual-order"]').attributes('aria-pressed')).toBe('true')
        dragEvent(rows[0]!.element, 'dragstart', transfer)
        dragEvent(rows[1]!.element, 'drop', transfer)
        await flushPromises()
        expect(wrapper.find('[data-testid="backlog-manual-order"]').attributes('aria-pressed')).toBe('true')
        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.sprintRemove).not.toHaveBeenCalled()
    })

    it('ignores late write responses after newer shared-stream ordering and blocks concurrent drags', async () => {
        const { api } = await import('../api/client')
        let finish!: (value: { status: 'ok'; tasks: string[]; changed: boolean }) => void
        vi.mocked(api.reorderTasks).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
        const wrapper = mount(SprintsList)
        await flushPromises()
        await wrapper.find('[data-testid="backlog-manual-order"]').trigger('click')
        const rows = wrapper.findAll('#backlog tbody tr')
        const transfer = dragTransfer()
        dragEvent(rows[0]!.element, 'dragstart', transfer)
        dragEvent(rows[1]!.element, 'drop', transfer)
        await flushPromises()
        expect(wrapper.find('#backlog tbody tr').attributes('draggable')).toBe('false')
        expect(dragEvent(rows[2]!.element, 'dragstart', transfer).defaultPrevented).toBe(true)
        vi.mocked(api.getBacklogOrder).mockResolvedValue({ status: 'ok', tasks: ['PRJ-003', 'PRJ-002', 'PRJ-001'], changed: false })
        orderEvents.handlers.get('task_order_changed')!(new MessageEvent('task_order_changed', {
            data: JSON.stringify({ sprint_id: null, tasks: ['PRJ-003', 'PRJ-002', 'PRJ-001'] }),
        }))
        await flushPromises()
        finish({ status: 'ok', tasks: ['PRJ-001', 'PRJ-002'], changed: true })
        await flushPromises()
        expect(wrapper.find('#backlog tbody tr').attributes('data-task-id')).toBe('PRJ-003')
        expect(api.reorderTasks).toHaveBeenCalledTimes(1)
        wrapper.unmount()
        expect(orderEvents.close).toHaveBeenCalled()
    })

    it('reconciles current metadata despite a stale notification and an older GET finishing late', async () => {
        const { api } = await import('../api/client')
        let finish!: (value: { status: 'ok'; tasks: string[]; changed: boolean }) => void
        vi.mocked(api.getBacklogOrder).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
            .mockResolvedValue({ status: 'ok', tasks: ['PRJ-003', 'PRJ-002'], changed: false })
        localStorage.setItem('lotar.sprints.backlogManualOrder', 'true')
        const wrapper = mount(SprintsList)
        await flushPromises()
        orderEvents.handlers.get('task_order_changed')!(new MessageEvent('task_order_changed', {
            data: JSON.stringify({ sprint_id: null, tasks: ['PRJ-001', 'PRJ-002'] }),
        }))
        await flushPromises()
        finish({ status: 'ok', tasks: ['PRJ-001', 'PRJ-002'], changed: false })
        await flushPromises()
        expect(wrapper.find('#backlog tbody tr').attributes('data-task-id')).toBe('PRJ-003')
    })

    it('moves across containers before placing relative to a destination row and reports placement failure truthfully', async () => {
        const { api } = await import('../api/client')
        const { showToast } = await import('../components/toast')
        sprintsRef.value = [makeSprint(1)]
        const tasks = [makeTask(1), makeTask(2, [1])]
        vi.mocked(api.listTasks).mockImplementation(async () => ({ status: 'ok', tasks, total: 2, count: 2, limit: 200, offset: 0 }))
        vi.mocked(api.sprintAdd).mockImplementation(async () => {
            tasks[0]!.sprints = [1]
            return { status: 'ok', modified: ['PRJ-001'], sprint_id: 1 } as any
        })
        vi.mocked(api.reorderTasks).mockRejectedValueOnce(new Error('Anchor disappeared'))
        const wrapper = mount(SprintsList)
        await flushPromises()
        await wrapper.find('[data-sprint-id="1"] [data-testid="sprint-manual-order"]').trigger('click')
        const transfer = dragTransfer()
        dragEvent(wrapper.find('#backlog [data-task-id="PRJ-001"]').element, 'dragstart', transfer)
        dragEvent(wrapper.find('[data-sprint-id="1"] [data-task-id="PRJ-002"]').element, 'drop', transfer)
        await flushPromises()
        expect(api.sprintAdd).toHaveBeenCalledTimes(1)
        expect(api.reorderTasks).toHaveBeenCalledExactlyOnceWith({ task: 'PRJ-001', sprint: 1, before: 'PRJ-002' })
        expect(showToast).toHaveBeenCalledWith('Task moved, but placement failed: Anchor disappeared')
        expect(wrapper.find('#backlog [data-task-id="PRJ-001"]').exists()).toBe(false)
        expect(sprintTable(wrapper, 1)).toHaveLength(2)
    })

    it.each(['reload', 'order', 'lifecycle'] as const)('blocks row drags and drops while %s is pending', async operation => {
        const { api } = await import('../api/client')
        sprintsRef.value = [makeSprint(1)]
        const tasks = [makeTask(1), makeTask(2), makeTask(3, [1])]
        const response = { tasks, total: 3, count: 3, limit: 200, offset: 0 }
        vi.mocked(api.listTasks).mockResolvedValue(response)
        let finish = () => { }
        const wrapper = mount(SprintsList)
        try {
            await flushPromises()
            await wrapper.find('[data-testid="backlog-manual-order"]').trigger('click')
            const rows = wrapper.findAll('#backlog tbody tr')
            const transfer = dragTransfer()
            dragEvent(rows[0]!.element, 'dragstart', transfer)
            if (operation === 'reload') {
                vi.mocked(api.listTasks).mockImplementationOnce(() => new Promise(resolve => { finish = () => resolve(response) }))
                await wrapper.find('button.reload').trigger('click')
            } else if (operation === 'order') {
                vi.mocked(api.getBacklogOrder).mockImplementationOnce(() => new Promise(resolve => {
                    finish = () => resolve({ status: 'ok', tasks: [], changed: false })
                }))
                await wrapper.find('button.reload').trigger('click')
            } else {
                vi.mocked(api.sprintUpdate).mockImplementationOnce(() => new Promise((_, reject) => {
                    finish = () => reject(new Error('Lifecycle test finished'))
                }))
                await wrapper.findAll('[data-sprint-id="1"] button').find(button => button.text() === 'Start')!.trigger('click')
            }
            await flushPromises()
            expect(rows[0]!.attributes('draggable')).toBe('false')
            dragEvent(rows[1]!.element, 'drop', transfer)
            expect(dragEvent(rows[0]!.element, 'dragstart', transfer).defaultPrevented).toBe(true)
            await flushPromises()
            expect(api.reorderTasks).not.toHaveBeenCalled()
            expect(api.sprintAdd).not.toHaveBeenCalled()
            expect(api.sprintRemove).not.toHaveBeenCalled()
        } finally {
            finish()
            await flushPromises()
        }
    })

    it('blocks row placement into closed sprints without making membership or order requests', async () => {
        const { api } = await import('../api/client')
        localStorage.setItem('lotar.sprints.window.v2', 'all')
        localStorage.setItem('lotar.sprints.manualOrder', '[1]')
        sprintsRef.value = [{ ...makeSprint(1), state: 'complete' }]
        const tasks = [makeTask(1), makeTask(2, [1])]
        vi.mocked(api.listTasks).mockResolvedValue({ tasks, total: 2, limit: 200, offset: 0 })
        const wrapper = mount(SprintsList)
        await flushPromises()
        const transfer = dragTransfer()
        dragEvent(wrapper.find('#backlog tbody tr').element, 'dragstart', transfer)
        dragEvent(sprintTable(wrapper, 1)[0]!.element, 'drop', transfer)
        await flushPromises()
        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.reorderTasks).not.toHaveBeenCalled()
    })

    it('does not offer task-row insertion targets for column drags', async () => {
        const { api } = await import('../api/client')
        const wrapper = mount(SprintsList)
        await flushPromises()
        await wrapper.find('[data-testid="backlog-manual-order"]').trigger('click')
        const transfer = dragTransfer()
        dragEvent(wrapper.find('#backlog th button').element, 'dragstart', transfer)
        const target = wrapper.find('#backlog tbody tr')
        dragEvent(target.element, 'dragover', transfer)
        dragEvent(target.element, 'drop', transfer)
        await flushPromises()
        expect(target.classes()).not.toContain('task-row--before')
        expect(api.reorderTasks).not.toHaveBeenCalled()
        expect(api.sprintAdd).not.toHaveBeenCalled()
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
