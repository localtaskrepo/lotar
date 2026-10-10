import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, h, ref, type Slots } from 'vue'
import type { SprintListItem } from '../api/types'

const routeState: { query: Record<string, any> } = { query: { month: '2024-02', sprints: '1' } }
const routerPushMock = vi.fn()

const projectsStore = {
    projects: ref([{ prefix: 'ACME', name: 'Acme Co' }]),
    refresh: vi.fn(async () => { }),
}

const _calendarItems = ref<any[]>([])
const _calendarStatus = ref('idle' as string)
const _calendarError = ref(null as string | null)
const _calendarHasSnapshot = ref(false)

function createCalendarHandle(
    key: string,
    status: ReturnType<typeof ref<string>> = ref('idle' as string),
    error: ReturnType<typeof ref<string | null>> = ref(null as string | null),
    hasSnapshot: ReturnType<typeof ref<boolean>> = ref(false),
) {
    return {
        key,
        hasSnapshot,
        ids: computed(() => _calendarItems.value.map((t: any) => t.id)),
        ranks: ref(new Map<string, number>()),
        total: computed(() => _calendarItems.value.length),
        status,
        error,
        lastSyncAt: ref(1),
        tasks: computed(() => _calendarItems.value),
        refresh: vi.fn(async () => { status.value = 'ready'; hasSnapshot.value = true }),
        retain: vi.fn(),
        release: vi.fn(),
    }
}

function calendarKeyOf(filter: Record<string, unknown> = {}): string {
    const source: Record<string, unknown> = { order: 'desc', ...(filter ?? {}) }
    return JSON.stringify(
        Object.entries(source)
            .filter(([k, v]) => !['limit', 'offset', 'page'].includes(k) && v !== undefined && v !== null && v !== '')
            .sort(([a], [b]) => a.localeCompare(b)),
    )
}

const calendarHandles = new Map<string, ReturnType<typeof createCalendarHandle>>()
function calendarHandleFor(filter: Record<string, unknown> = {}) {
    const key = calendarKeyOf(filter)
    let handle = calendarHandles.get(key)
    if (!handle) {
        handle = createCalendarHandle(key)
        calendarHandles.set(key, handle)
    }
    return handle
}

const tasksStore = {
    items: _calendarItems,
    getQuery: vi.fn((filter?: Record<string, unknown>) => calendarHandleFor(filter)),
    hydrateAll: vi.fn(async () => { }),
    status: _calendarStatus,
}

/** Defer every refresh of the given handles until `complete()` is called. */
function deferHandleRefreshes(handles: Array<ReturnType<typeof createCalendarHandle>>) {
    const resolvers: Array<() => void> = []
    for (const handle of handles) {
        handle.refresh.mockImplementation(() => {
            handle.status.value = 'loading'
            return new Promise<void>((resolve) => { resolvers.push(resolve) })
        })
    }
    return {
        complete() {
            for (const handle of handles) {
                handle.status.value = 'ready'
                handle.hasSnapshot.value = true
            }
            resolvers.forEach((resolve) => resolve())
        },
    }
}

/** The handle the page adopted last. */
function adoptedCalendarHandle() {
    const results = tasksStore.getQuery.mock.results
    return results[results.length - 1]?.value as ReturnType<typeof createCalendarHandle> | undefined
}

const sprintsStore = {
    sprints: ref<SprintListItem[]>([]),
    refresh: vi.fn(async () => { }),
    loading: ref(false),
}

const openTaskPanelMock = vi.fn()

vi.mock('vue-router', () => ({
    useRoute: () => routeState,
    useRouter: () => ({ push: routerPushMock }),
}))

vi.mock('../components/TaskHoverCard.vue', () => ({
    default: {
        props: ['fields'],
        template: '<div class="task-hover" :data-fields="JSON.stringify(fields || null)"><slot /></div>',
    },
}))

vi.mock('../components/UiButton.vue', () => ({
    default: {
        emits: ['click'],
        template: '<button type="button" @click="$emit(\'click\', $event)"><slot /></button>',
    },
}))

vi.mock('../components/UiLoader.vue', () => ({
    default: { template: '<div class="loader"><slot /></div>' },
}))

vi.mock('../components/UiSelect.vue', () => ({
    default: {
        props: ['modelValue'],
        emits: ['update:modelValue', 'change'],
        setup(props: { modelValue?: string }, { emit, slots }: { emit: (event: string, value: string) => void; slots: Slots }) {
            const onChange = (event: Event) => {
                const target = event.target as HTMLSelectElement
                const value = target?.value ?? ''
                emit('update:modelValue', value)
                emit('change', value)
            }
            return () => h('select', { value: props.modelValue, onChange }, slots.default?.())
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
    useSprintFilterOptions: () => ref([]),
    useSprints: () => ({
        sprints: sprintsStore.sprints,
        refresh: sprintsStore.refresh,
        loading: sprintsStore.loading,
    }),
}))

vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({ openTaskPanel: openTaskPanelMock }),
}))

import Calendar from '../pages/Calendar.vue'

function baseSprint(overrides: Partial<SprintListItem> = {}): SprintListItem {
    // Matches the wire shape: unset optional members are omitted, never null.
    return {
        id: 1,
        display_name: 'Sprint',
        state: 'active',
        planned_start: '2024-02-01',
        planned_end: '2024-02-05',
        ...overrides,
    }
}

describe('Calendar sprint overlay', () => {
    beforeEach(() => {
        routeState.query = { month: '2024-02', sprints: '1' }
        projectsStore.projects.value = [{ prefix: 'ACME', name: 'Acme Co' }]
        tasksStore.items.value = []
        tasksStore.status.value = 'idle'
        _calendarError.value = null
        _calendarHasSnapshot.value = false
        calendarHandles.clear()
        tasksStore.hydrateAll.mockClear()
        tasksStore.getQuery.mockClear()
        projectsStore.refresh.mockClear()
        sprintsStore.refresh.mockClear()
        routerPushMock.mockClear()
        openTaskPanelMock.mockClear()
    })

    it('renders sprint pills across plan length and exposes testing hooks', async () => {
        sprintsStore.sprints.value = [
            baseSprint({
                id: 501,
                display_name: 'Plan Length Sprint',
                planned_start: '2024-02-05',
                planned_end: undefined,
                plan_length: '10d',
            }),
        ]

        const wrapper = mount(Calendar)
        await flushPromises()

        const startCell = wrapper.find('[data-date="2024-02-05"]')
        expect(startCell.exists()).toBe(true)
        const startPill = startCell.find('[data-sprint-id="501"]')
        expect(startPill.exists()).toBe(true)
        expect(startPill.attributes('data-sprint-state')).toBe('active')

        const finalCell = wrapper.find('[data-date="2024-02-14"]')
        expect(finalCell.exists()).toBe(true)
        expect(finalCell.find('[data-sprint-id="501"]').exists()).toBe(true)

        const afterWindow = wrapper.find('[data-date="2024-02-15"]')
        expect(afterWindow.exists()).toBe(true)
        expect(afterWindow.find('[data-sprint-id="501"]').exists()).toBe(false)
    })

    it('refreshes sprint data when enabling overlay and uses shared palette colors', async () => {
        routeState.query = { month: '2024-02' }
        const overdue = baseSprint({
            id: 999,
            display_name: 'Overdue Sprint',
            state: 'overdue',
            planned_start: '2024-02-02',
            planned_end: '2024-02-05',
        })
            ; (overdue as any).state = 'OVERDUE'
        sprintsStore.sprints.value = [overdue]

        const wrapper = mount(Calendar)
        await flushPromises()
        expect(sprintsStore.refresh).toHaveBeenCalledTimes(1)

        const toggle = wrapper.find('button.toggle-sprints')
        expect(toggle.exists()).toBe(true)
        await toggle.trigger('click')
        await flushPromises()

        expect(sprintsStore.refresh).toHaveBeenCalledTimes(2)
        const overduePill = wrapper.find('[data-sprint-id="999"]')
        expect(overduePill.exists()).toBe(true)
        expect(overduePill.attributes('data-sprint-state')).toBe('OVERDUE')
        expect(overduePill.attributes('style') || '').toContain('var(--color-danger)')
    })

    it('shows planned window even when actual dates differ and dims the overflow', async () => {
        sprintsStore.sprints.value = [
            baseSprint({
                id: 777,
                planned_start: '2024-02-01',
                planned_end: '2024-02-07',
                actual_start: '2024-02-03',
                actual_end: '2024-02-04',
            }),
        ]

        const wrapper = mount(Calendar)
        await flushPromises()

        const plannedStartCell = wrapper.find('[data-date="2024-02-01"]')
        const plannedStartPill = plannedStartCell.find('[data-sprint-id="777"]')
        expect(plannedStartPill.exists()).toBe(true)
        expect(plannedStartPill.classes()).toContain('dim-before')

        const actualEndCell = wrapper.find('[data-date="2024-02-04"]')
        const actualEndPill = actualEndCell.find('[data-sprint-id="777"]')
        expect(actualEndPill.classes()).toContain('actual-end')

        const afterActualEndCell = wrapper.find('[data-date="2024-02-06"]')
        const afterActualEndPill = afterActualEndCell.find('[data-sprint-id="777"]')
        expect(afterActualEndPill.classes()).toContain('dim-after')
        expect(afterActualEndPill.attributes('data-actual-phase')).toBe('after-end')
    })
})

describe('Calendar task hover cards', () => {
    beforeEach(() => {
        routeState.query = { month: '2024-02', project: 'ACME' }
        localStorage.clear()
        calendarHandles.clear()
        tasksStore.getQuery.mockClear()
        _calendarStatus.value = 'idle'
        _calendarError.value = null
        _calendarHasSnapshot.value = false
        tasksStore.items.value = [
            {
                id: 'ACME-123',
                title: 'Hover card title',
                due_date: '2024-02-05',
                modified: '2024-02-01T00:00:00Z',
            },
        ]
        openTaskPanelMock.mockClear()
    })

    it('keeps inline rows single-line and opens a day dialog for overflow', async () => {
        tasksStore.items.value = Array.from({ length: 7 }).map((_, idx) => ({
            id: `ACME-${idx + 1}`,
            title: `Task ${idx + 1} has a very long title that should be ellipsized`,
            status: 'Todo',
            due_date: '2024-02-05',
            modified: '2024-02-01T00:00:00Z',
        }))

        const wrapper = mount(Calendar)
        await flushPromises()

        const cell = wrapper.find('[data-date="2024-02-05"]')
        expect(cell.exists()).toBe(true)

        // Inline calendar rows should not show meta like status.
        expect(cell.text()).not.toContain('Todo')

        const more = cell.find('li.more')
        expect(more.exists()).toBe(true)
        expect(more.text()).toMatch(/more ticket/)

        await more.trigger('click')
        await flushPromises()

        const overlay = document.querySelector('.ui-modal__overlay') as HTMLElement | null
        expect(overlay).not.toBeNull()
        if (!overlay) return

        const items = overlay.querySelectorAll('.calendar-day-dialog__item')
        expect(items.length).toBe(7)

            ; (items[0] as HTMLElement).click()
        await flushPromises()

        expect(openTaskPanelMock).toHaveBeenCalled()
        expect(document.querySelector('.ui-modal__overlay')).toBeNull()
    })

    it('opens the create panel with the due date prefilled when the cell add button is clicked', async () => {
        const wrapper = mount(Calendar)
        await flushPromises()

        const cell = wrapper.find('[data-date="2024-02-05"]')
        const addBtn = cell.find('.cell-add__btn')
        expect(addBtn.exists()).toBe(true)

        await addBtn.trigger('click')

        expect(openTaskPanelMock).toHaveBeenCalledWith(expect.objectContaining({
            taskId: 'new',
            initialDueDate: '2024-02-05',
        }))
    })

    it('wraps due tasks in hover cards and keeps click-to-open behavior', async () => {
        const wrapper = mount(Calendar)
        await flushPromises()

        const cell = wrapper.find('[data-date="2024-02-05"]')
        expect(cell.exists()).toBe(true)

        const taskItem = cell.find('.task-item')
        expect(taskItem.exists()).toBe(true)
        expect(taskItem.find('.task-hover').exists()).toBe(true)

        await taskItem.trigger('click')
        expect(openTaskPanelMock).toHaveBeenCalledWith({ taskId: 'ACME-123' })
    })

    it('loads persisted hover card field visibility per project', async () => {
        localStorage.setItem(
            'lotar.calendarHoverFields.columns::ACME',
            JSON.stringify(['id', 'title', 'status', 'priority', 'reporter', 'assignee', 'sprints', 'due_date', 'modified']),
        )

        const wrapper = mount(Calendar)
        await flushPromises()

        const hover = wrapper.find('.task-hover')
        expect(hover.exists()).toBe(true)
        const raw = hover.attributes('data-fields') || ''
        expect(raw).toContain('"tags":false')
    })
  it('keeps the month grid mounted during background refreshes and surfaces refresh failures', async () => {
    // The lone project is auto-selected, so the page's first query carries
    // project ACME. Hold that handle's first refresh open.
    // The mount may adopt the bare key and/or the auto-selected ACME key;
    // defer every candidate so the first load is deterministically pending.
    const deferred = deferHandleRefreshes([
      calendarHandleFor({}),
      calendarHandleFor({ project: 'ACME' }),
    ])
    const wrapper = mount(Calendar)
    await flushPromises()
    expect(wrapper.find('.loader').exists()).toBe(true)
    expect(wrapper.find('.grid.body').exists()).toBe(false)

    // First load completes (still in a loading state): the grid renders and
    // stays mounted — background refreshes never unmount it.
    deferred.complete()
    await flushPromises()
    expect(wrapper.find('.grid.body').exists()).toBe(true)
    expect(wrapper.find('.loader').exists()).toBe(false)

    // A refresh failure surfaces as a retry banner; the grid is retained.
    const adopted = adoptedCalendarHandle()!
    adopted.error.value = 'flaky network'
    adopted.status.value = 'error'
    await flushPromises()
    expect(wrapper.find('.refresh-error').exists()).toBe(true)
    expect(wrapper.find('.refresh-error').text()).toContain('flaky network')
    expect(wrapper.find('.grid.body').exists()).toBe(true)
    wrapper.unmount()
  })
  it('shows the first-load loader on a project switch until the new key publishes', async () => {
    const wrapper = mount(Calendar)
    await flushPromises()
    expect(wrapper.find('.grid.body').exists()).toBe(true)

    wrapper.unmount()
    // The real FilterBar persists its snapshot and force-selects a LONE
    // project; drop the snapshot and offer two projects so the next mount
    // follows the routed project (BETA) instead of snapping back to ACME.
    localStorage.clear()
    projectsStore.projects.value = [
        { prefix: 'ACME', name: 'Acme Co' },
        { prefix: 'BETA', name: 'Beta Co' },
    ]

    // Switch projects (fresh mount on the new route): the new key's first
    // refresh is held open.
    const deferred = deferHandleRefreshes([
      calendarHandleFor({}),
      calendarHandleFor({ project: 'BETA' }),
    ])
    routeState.query = { month: '2024-02', sprints: '1', project: 'BETA' }
    const wrapper2 = mount(Calendar)
    await flushPromises()

    // First load of the new key: loader shows, grid unmounted.
    expect(wrapper2.find('.loader').exists()).toBe(true)
    expect(wrapper2.find('.grid.body').exists()).toBe(false)

    deferred.complete()
    await flushPromises()
    expect(wrapper2.find('.loader').exists()).toBe(false)
    expect(wrapper2.find('.grid.body').exists()).toBe(true)
    wrapper2.unmount()
  })
})
