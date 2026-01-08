import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { h, ref, type Slots } from 'vue'
import type { SprintListItem } from '../api/types'

const routeState: { query: Record<string, any> } = { query: { month: '2024-02', sprints: '1' } }
const routerPushMock = vi.fn()

const projectsStore = {
    projects: ref([{ prefix: 'ACME', name: 'Acme Co' }]),
    refresh: vi.fn(async () => { }),
}

const tasksStore = {
    items: ref<any[]>([]),
    refresh: vi.fn(async () => { }),
    loading: ref(false),
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

vi.mock('../composables/useTasks', () => ({
    useTasks: () => tasksStore,
}))

vi.mock('../composables/useSprints', () => ({
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
    return {
        id: 1,
        display_name: 'Sprint',
        state: 'active',
        planned_start: '2024-02-01',
        planned_end: '2024-02-05',
        plan_length: null,
        actual_start: null,
        actual_end: null,
        computed_end: null,
        warnings: [],
        label: null,
        goal: null,
        overdue_after: null,
        notes: null,
        capacity_points: null,
        capacity_hours: null,
        ...overrides,
    }
}

describe('Calendar sprint overlay', () => {
    beforeEach(() => {
        routeState.query = { month: '2024-02', sprints: '1' }
        projectsStore.projects.value = [{ prefix: 'ACME', name: 'Acme Co' }]
        tasksStore.items.value = []
        tasksStore.loading.value = false
        tasksStore.refresh.mockClear()
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
                planned_end: null,
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

        const overlay = document.querySelector('.calendar-day-dialog__overlay') as HTMLElement | null
        expect(overlay).not.toBeNull()
        if (!overlay) return

        const items = overlay.querySelectorAll('.calendar-day-dialog__item')
        expect(items.length).toBe(7)

            ; (items[0] as HTMLElement).click()
        await flushPromises()

        expect(openTaskPanelMock).toHaveBeenCalled()
        expect(document.querySelector('.calendar-day-dialog__overlay')).toBeNull()
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
        localStorage.setItem('lotar.calendarHoverFields::ACME', JSON.stringify({ tags: false }))

        const wrapper = mount(Calendar)
        await flushPromises()

        const hover = wrapper.find('.task-hover')
        expect(hover.exists()).toBe(true)
        const raw = hover.attributes('data-fields') || ''
        expect(raw).toContain('"tags":false')
    })
})
