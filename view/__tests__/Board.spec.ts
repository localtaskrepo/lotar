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

describe('Board group-by swimlanes', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        tasksStore.hydrateAll.mockClear()
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })
    afterEach(() => { vi.useRealTimers() })

    it('shows swimlane headers when grouped by assignee', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', assignee: 'alice', status: 'Todo' }),
            baseTask({ id: 'ACME-2', assignee: 'bob', status: 'Todo' }),
            baseTask({ id: 'ACME-3', assignee: null, status: 'Todo' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        // No swimlane headers initially
        expect(wrapper.findAll('.swimlane-header')).toHaveLength(0)

        // Select group-by assignee
        const select = wrapper.find('[data-testid="board-groupby"]')
        await select.setValue('assignee')
        await flushPromises()

        const headers = wrapper.findAll('.swimlane-header')
        expect(headers.length).toBeGreaterThanOrEqual(2) // alice, bob, (none)
        const headerTexts = headers.map(h => h.text())
        expect(headerTexts.some(t => t.includes('alice'))).toBe(true)
        expect(headerTexts.some(t => t.includes('bob'))).toBe(true)
    })

    it('shows swimlane headers when grouped by priority', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', priority: 'Critical', status: 'Todo' }),
            baseTask({ id: 'ACME-2', priority: 'Low', status: 'Todo' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const select = wrapper.find('[data-testid="board-groupby"]')
        await select.setValue('priority')
        await flushPromises()

        const headers = wrapper.findAll('.swimlane-header')
        expect(headers.length).toBeGreaterThanOrEqual(2)
        const headerTexts = headers.map(h => h.text())
        expect(headerTexts.some(t => t.includes('Critical'))).toBe(true)
        expect(headerTexts.some(t => t.includes('Low'))).toBe(true)
    })
})

describe('Board member badges', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        tasksStore.hydrateAll.mockClear()
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })
    afterEach(() => { vi.useRealTimers() })

    it('renders member badges with initials for assignee', async () => {
        const tasks = [baseTask({ id: 'ACME-1', assignee: 'alice', status: 'Todo' })]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const badge = wrapper.find('.member-badge.small')
        expect(badge.exists()).toBe(true)
        expect(badge.text()).toBe('AL')
    })
})

describe('Board progressive disclosure', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        tasksStore.hydrateAll.mockClear()
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })
    afterEach(() => { vi.useRealTimers() })

    it('shows "show more" button when column exceeds page size', async () => {
        // Create 35 tasks in Todo (default page size is 30)
        const tasks: TaskDTO[] = []
        for (let i = 1; i <= 35; i++) {
            tasks.push(baseTask({ id: `ACME-${i}`, title: `Task ${i}`, status: 'Todo', modified: `2026-01-02T${String(i).padStart(2, '0')}:00:00Z` }))
        }
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        // Should show "Show N more…" button
        const showMoreBtn = wrapper.find('.show-more-btn')
        expect(showMoreBtn.exists()).toBe(true)
        expect(showMoreBtn.text()).toContain('more')

        // Cards visible should be 30
        const cards = wrapper.findAll('article.card.task')
        expect(cards.length).toBe(30)

        // Click show more
        await showMoreBtn.trigger('click')
        await flushPromises()

        // Now all 35 should be visible
        expect(wrapper.findAll('article.card.task').length).toBe(35)
        expect(wrapper.find('.show-more-btn').exists()).toBe(false)
    })

    it('hides "show more" when all cards fit within page size', async () => {
        const tasks = [baseTask({ id: 'ACME-1', status: 'Todo' })]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        expect(wrapper.find('.show-more-btn').exists()).toBe(false)
    })
})

describe('Board group-by type', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        tasksStore.hydrateAll.mockClear()
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })
    afterEach(() => { vi.useRealTimers() })

    it('shows swimlane headers when grouped by type', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', task_type: 'bug', status: 'Todo' }),
            baseTask({ id: 'ACME-2', task_type: 'feature', status: 'Todo' }),
            baseTask({ id: 'ACME-3', task_type: 'bug', status: 'Doing' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const select = wrapper.find('[data-testid="board-groupby"]')
        await select.setValue('type')
        await flushPromises()

        const headers = wrapper.findAll('.swimlane-header')
        expect(headers.length).toBeGreaterThanOrEqual(2)
        const headerTexts = headers.map(h => h.text())
        expect(headerTexts.some(t => t.includes('bug'))).toBe(true)
        expect(headerTexts.some(t => t.includes('feature'))).toBe(true)
    })
})

describe('Board collapsible groups', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        tasksStore.hydrateAll.mockClear()
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })
    afterEach(() => { vi.useRealTimers() })

    it('collapses and expands groups when clicking the swimlane header', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', assignee: 'alice', status: 'Todo' }),
            baseTask({ id: 'ACME-2', assignee: 'bob', status: 'Todo' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        // Group by assignee
        const select = wrapper.find('[data-testid="board-groupby"]')
        await select.setValue('assignee')
        await flushPromises()

        // Both cards should be visible initially
        expect(wrapper.findAll('article.card.task').length).toBe(2)

        // Click the first swimlane header to collapse it
        const headers = wrapper.findAll('.swimlane-header')
        expect(headers.length).toBeGreaterThanOrEqual(2)
        await headers[0].trigger('click')
        await flushPromises()

        // One group's cards should be hidden
        expect(wrapper.findAll('article.card.task').length).toBe(1)

        // Header should have collapsed class
        expect(headers[0].classes()).toContain('collapsed')

        // Click again to expand
        await headers[0].trigger('click')
        await flushPromises()

        expect(wrapper.findAll('article.card.task').length).toBe(2)
    })
})

describe('Board ticket highlight', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        tasksStore.hydrateAll.mockClear()
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })
    afterEach(() => { vi.useRealTimers() })

    it('highlights a card on single click and deselects on second click', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', status: 'Todo' }),
            baseTask({ id: 'ACME-2', status: 'Todo' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const cards = wrapper.findAll('article.card.task')
        expect(cards.length).toBe(2)

        // No card should be highlighted initially
        expect(wrapper.findAll('.task--selected').length).toBe(0)

        // Click first card
        await cards[0].trigger('click')
        await flushPromises()

        expect(cards[0].classes()).toContain('task--selected')
        expect(cards[1].classes()).not.toContain('task--selected')

        // Click same card again to deselect
        await cards[0].trigger('click')
        await flushPromises()

        expect(cards[0].classes()).not.toContain('task--selected')
    })
})

describe('Board aligned swimlanes', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        tasksStore.hydrateAll.mockClear()
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })
    afterEach(() => { vi.useRealTimers() })

    it('aligns groups across columns with shared swimlane headers', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', assignee: 'alice', status: 'Todo' }),
            baseTask({ id: 'ACME-2', assignee: 'bob', status: 'Todo' }),
            baseTask({ id: 'ACME-3', assignee: 'alice', status: 'Doing' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const select = wrapper.find('[data-testid="board-groupby"]')
        await select.setValue('assignee')
        await flushPromises()

        // Swimlane headers should span all columns (one per unique group)
        const swimlaneRows = wrapper.findAll('.swimlane-row')
        expect(swimlaneRows.length).toBe(2) // alice, bob

        // Each swimlane row should have gridColumn 1 / -1
        for (const row of swimlaneRows) {
            expect(row.attributes('style')).toContain('grid-column')
        }

        // All 3 tasks should be visible
        expect(wrapper.findAll('article.card.task').length).toBe(3)
    })
})

describe('Board grouping persistence', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        tasksStore.hydrateAll.mockClear()
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })
    afterEach(() => { vi.useRealTimers() })

    it('persists groupBy to localStorage when changed', async () => {
        const tasks = [baseTask({ id: 'ACME-1', status: 'Todo' })]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const select = wrapper.find('[data-testid="board-groupby"]')
        await select.setValue('priority')
        await flushPromises()

        expect(localStorage.getItem('lotar.boardGroupBy::ACME')).toBe('priority')
    })

    it('restores groupBy from localStorage on mount', async () => {
        localStorage.setItem('lotar.boardGroupBy::ACME', 'assignee')
        const tasks = [
            baseTask({ id: 'ACME-1', assignee: 'alice', status: 'Todo' }),
            baseTask({ id: 'ACME-2', assignee: 'bob', status: 'Todo' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        // Should already show swimlane headers without user action
        const headers = wrapper.findAll('.swimlane-header')
        expect(headers.length).toBeGreaterThanOrEqual(2)
    })

    it('resets groupBy when clear-filters button is clicked', async () => {
        const tasks = [baseTask({ id: 'ACME-1', assignee: 'alice', status: 'Todo' })]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        // Set group-by
        const select = wrapper.find('[data-testid="board-groupby"]')
        await select.setValue('assignee')
        await flushPromises()
        expect(wrapper.findAll('.swimlane-header').length).toBeGreaterThanOrEqual(1)

        // Click the clear filters button (the UiButton mock renders as a plain <button class="btn">)
        // It's the one between the group-by select and the reload button
        const allBtns = wrapper.findAll('button.btn')
        // The clear-filters button emits 'click' and is not the reload button
        const clearBtn = allBtns.find(b => {
          const inner = b.find('.icon')
          return inner.exists()
        })!
        await clearBtn.trigger('click')
        await flushPromises()

        // Grouping should be reset
        expect(wrapper.findAll('.swimlane-header').length).toBe(0)
        expect(localStorage.getItem('lotar.boardGroupBy::ACME')).toBe('none')
    })
})

describe('Board overdue suppression for done tasks', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        tasksStore.hydrateAll.mockClear()
        // Configured statuses: Todo, Doing, Done — "Done" is the final/done status
        configStore.statuses.value = ['Todo', 'Doing', 'Done']
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })
    afterEach(() => { vi.useRealTimers() })

    it('shows overdue for tasks not in the last status', async () => {
        const tasks = [baseTask({ id: 'ACME-1', status: 'Todo', due_date: '2025-12-01' })]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const card = wrapper.find('article.card.task')
        expect(card.text()).toContain('Overdue')
    })

    it('does not show overdue for tasks in the last configured status', async () => {
        const tasks = [baseTask({ id: 'ACME-1', status: 'Done', due_date: '2025-12-01' })]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const card = wrapper.find('article.card.task')
        expect(card.text()).toContain('Due')
        expect(card.text()).not.toContain('Overdue')
    })
})
