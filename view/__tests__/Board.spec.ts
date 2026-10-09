import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, h, nextTick, ref, shallowRef } from 'vue'
import type { SprintListItem, TaskDTO } from '../api/types'

const routeState: { query: Record<string, any> } = { query: { project: 'ACME' } }
const routerPushMock = vi.fn()

const projectsStore = {
    projects: ref([{ prefix: 'ACME', name: 'Acme Co' }, { prefix: 'BETA', name: 'Beta Co' }]),
    refresh: vi.fn(async () => { }),
}

const taskMap = shallowRef(new Map<string, TaskDTO>())
const taskVersion = shallowRef(0)
function createBoardHandle(
    key: string,
    status = shallowRef('idle' as string),
    error = shallowRef(null as string | null),
    hasSnapshot = shallowRef(false),
    memberIds: Set<string> | null = null,
) {
    return {
        key,
        hasSnapshot,
        ids: computed(() => Array.from(taskMap.value.keys())),
        ranks: shallowRef(new Map<string, number>()),
        total: computed(() => taskMap.value.size),
        status,
        error,
        lastSyncAt: shallowRef(0),
        tasks: computed(() => {
            void taskVersion.value
            const all = Array.from(taskMap.value.values())
            // Optional per-query membership: when set, the handle serves only
            // its member rows, mirroring the server-scoped query contract.
            return memberIds ? all.filter((t) => memberIds.has(t.id)) : all
        }),
        refresh: vi.fn(async () => { status.value = 'ready'; hasSnapshot.value = true }),
        retain: vi.fn(),
        release: vi.fn(),
    }
}

function boardKeyOf(filter: Record<string, unknown> = {}): string {
    const source: Record<string, unknown> = { order: 'desc', ...(filter ?? {}) }
    return JSON.stringify(
        Object.entries(source)
            .filter(([k, v]) => !['limit', 'offset', 'page'].includes(k) && v !== undefined && v !== null && v !== '')
            .sort(([a], [b]) => a.localeCompare(b)),
    )
}

const boardQueryHandles = new Map<string, ReturnType<typeof createBoardHandle>>()
function boardHandleFor(filter: Record<string, unknown> = {}, memberIds?: Set<string>) {
    const key = boardKeyOf(filter)
    let handle = boardQueryHandles.get(key)
    if (!handle) {
        handle = createBoardHandle(key, undefined, undefined, undefined, memberIds ?? null)
        boardQueryHandles.set(key, handle)
    }
    return handle
}
const boardQueryHandle = boardHandleFor({ project: 'ACME' })

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
    getQuery: vi.fn((filter?: Record<string, unknown>) => boardHandleFor(filter)),
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

const showConfigMock = vi.fn(async (_project?: string) => ({}))
const setStatusMock = vi.fn(async (_id: string, _status: string) => { })

vi.mock('../api/client', () => ({
    api: {
        setStatus: (id: string, status: string) => setStatusMock(id, status),
        showConfig: (project?: string) => showConfigMock(project),
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
    default: {
        props: ['title', 'description'],
        template: '<div class="empty"><h3>{{ title }}</h3><p>{{ description }}</p><slot name="actions" /></div>',
    },
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
        name: 'FilterBar',
        props: ['statuses', 'priorities', 'types', 'value', 'showStatus', 'emitProjectKey', 'storageKey', 'customPresets', 'enableDueSoon', 'enableRecent'],
        emits: ['update:value'],
        setup(_props: any, { expose, slots }: { expose: (api: any) => void; slots: any }) {
            expose({ appendCustomFilter: () => { }, clear: () => { } })
            return () => h('div', { class: 'filter-bar' }, slots.actions?.())
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
    useSprintFilterOptions: (sprints: any) => ({
        value: (sprints?.value || []).map((s: any) => ({ id: s.id, label: s.display_name || `Sprint ${s.id}` })),
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
import { invalidateCompletionPolicies } from '../composables/useCompletionPolicy'

function policyConfig(overrides: Record<string, unknown> = {}) {
    return {
        issue_states: ['Todo', 'Doing', 'Done'],
        effective_done_states: ['Done'],
        done_states_mode: 'inferred',
        task_calendar_day: '2026-01-05',
        ...overrides,
    }
}

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
        boardQueryHandles.clear()
        tasksStore.hydrateAll.mockClear()
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
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

    it('offers an accessible project chooser instead of selecting the first project', async () => {
        routeState.query = {}
        const wrapper = mount(Board)
        await flushPromises()

        const selector = wrapper.find<HTMLSelectElement>('#board-project-select')
        expect(selector.exists()).toBe(true)
        expect(wrapper.find('label[for="board-project-select"]').text()).toBe('Project')
        expect(selector.element.value).toBe('')
        expect(selector.findAll('option').map(option => option.text())).toEqual([
            'Choose a project', 'Acme Co (ACME)', 'Beta Co (BETA)',
        ])
        expect(tasksStore.getQuery).not.toHaveBeenCalled()
        expect(routerPushMock).not.toHaveBeenCalled()

        const filters = wrapper.findComponent({ name: 'FilterBar' })
        filters.vm.$emit('update:value', { due: 'overdue' })
        await selector.setValue('BETA')
        await flushPromises()
        expect(routerPushMock).toHaveBeenCalledWith({ path: '/boards', query: { project: 'BETA' } })
        expect(filters.props('value')).toMatchObject({ project: 'BETA', due: 'overdue' })
        expect(wrapper.find('#board-project-select').exists()).toBe(false)
        wrapper.unmount()
    })

    it('keeps an explicitly selected project without showing the chooser', async () => {
        const wrapper = mount(Board)
        await flushPromises()
        expect(wrapper.find('#board-project-select').exists()).toBe(false)
        expect(wrapper.find('h1').text()).toContain('ACME')
        expect(tasksStore.getQuery).toHaveBeenCalledWith(expect.objectContaining({ project: 'ACME' }))
        wrapper.unmount()
    })

    it('disables the empty-state project chooser when there are no projects', async () => {
        const previous = projectsStore.projects.value
        projectsStore.projects.value = []
        routeState.query = {}
        const wrapper = mount(Board)
        try {
            await flushPromises()
            const selector = wrapper.find<HTMLSelectElement>('#board-project-select')
            expect(selector.element.disabled).toBe(true)
            expect(selector.text()).toContain('No projects available')
            expect(tasksStore.getQuery).not.toHaveBeenCalled()
        } finally {
            wrapper.unmount()
            projectsStore.projects.value = previous
        }
    })

    it('opens the create panel with the column status prefilled when the header add button is clicked', async () => {
        const tasks = [baseTask({ id: 'ACME-1', title: 'Alpha' })]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const addBtn = wrapper.find('[data-status="Todo"] .board-col-add')
        expect(addBtn.exists()).toBe(true)

        await addBtn.trigger('click')

        expect(openTaskPanelMock).toHaveBeenCalledWith(expect.objectContaining({
            taskId: 'new',
            initialStatus: 'Todo',
        }))
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

        const firstCard = wrapper.findAll('article.card.task')[0]!
        expect(firstCard.text()).toContain('ACME-1')
        expect(firstCard.text()).toContain('Alpha')
        expect(firstCard.text()).toContain('high')
        expect(firstCard.text()).toContain('alice')
        expect(firstCard.text()).toContain('Due')
        expect(firstCard.text()).toContain('one')

        // Open the popover so the field checkboxes are in the DOM.
        const fieldsButton = wrapper
            .findAll('button')
            .find((b) => b.text().trim() === 'Fields')
        if (fieldsButton) await fieldsButton.trigger('click')
        await flushPromises()

        // Open is not required for DOM presence in tests; just flip the checkboxes.
        const labels = wrapper.findAll('label.column-option')
        const labelTexts = labels.map((l) => l.text())
        const byLabel = (label: string) => {
            const labelEl = labels.find((l) => l.text().trim() === label)
            if (!labelEl) throw new Error(`Label not found: ${label}; available: ${labelTexts.join('|')}`)
            return labelEl.find('input[type="checkbox"]')
        }

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

        const saved = JSON.parse(localStorage.getItem('lotar.boardFields.columns::ACME') || '[]')
        for (const hidden of ['id', 'title', 'priority', 'assignee', 'due_date', 'tags', 'sprints']) {
            expect(saved).not.toContain(hidden)
        }

        // Different project should not inherit ACME settings.
        routeState.query = { project: 'BETA' }
        const wrapper2 = mount(Board)
        await flushPromises()

        const betaCard = wrapper2.findAll('article.card.task')[0]!
        expect(betaCard.text()).toContain('BETA-2')
        expect(betaCard.text()).toContain('Beta')
    })

    it('renders custom field values on cards when enabled', async () => {
        configStore.customFields.value = ['sprint']
        const tasks = [
            baseTask({ id: 'ACME-1', title: 'Alpha', custom_fields: { sprint: 'Sprint-42' } }),
            baseTask({ id: 'ACME-2', title: 'Beta', custom_fields: {} }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const cards = wrapper.findAll('article.card.task')
        expect(cards[0]!.text()).not.toContain('Sprint-42')

        const fieldsButton = wrapper
            .findAll('button')
            .find((b) => b.text().trim() === 'Fields')
        if (fieldsButton) await fieldsButton.trigger('click')
        await flushPromises()

        const labels = wrapper.findAll('label.column-option')
        const sprintCheck = labels
            .find((l) => l.text().trim() === 'sprint')
            ?.find('input[type="checkbox"]')
        expect(sprintCheck).toBeTruthy()
        await sprintCheck!.setValue(true)
        await flushPromises()

        expect(cards[0]!.text()).toContain('sprint:')
        expect(cards[0]!.text()).toContain('Sprint-42')
        expect(cards[1]!.text()).not.toContain('sprint:')

        const saved = JSON.parse(localStorage.getItem('lotar.boardFields.columns::ACME') || '[]')
        expect(saved).toContain('custom:sprint')
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
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
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
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
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
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
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
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
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
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
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
        await headers[0]!.trigger('click')
        await flushPromises()

        // One group's cards should be hidden
        expect(wrapper.findAll('article.card.task').length).toBe(1)

        // Header should have collapsed class
        expect(headers[0]!.classes()).toContain('collapsed')

        // Click again to expand
        await headers[0]!.trigger('click')
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
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
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
        await cards[0]!.trigger('click')
        await flushPromises()

        expect(cards[0]!.classes()).toContain('task--selected')
        expect(cards[1]!.classes()).not.toContain('task--selected')

        // Click same card again to deselect
        await cards[0]!.trigger('click')
        await flushPromises()

        expect(cards[0]!.classes()).not.toContain('task--selected')
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
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
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
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
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
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
        // Configured statuses: Todo, Doing, Done — server-resolved terminal
        // policy: Done (inferred legacy default).
        configStore.statuses.value = ['Todo', 'Doing', 'Done']
        showConfigMock.mockReset()
        showConfigMock.mockImplementation(async () => policyConfig())
        invalidateCompletionPolicies()
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

    it('suppresses overdue for a custom terminal status in the middle of the list', async () => {
        // Explicit policy: Done + Closed are terminal (Closed sits mid-list).
        configStore.statuses.value = ['Todo', 'Closed', 'Doing', 'Done']
        showConfigMock.mockImplementation(async () => policyConfig({
            issue_states: ['Todo', 'Closed', 'Doing', 'Done'],
            effective_done_states: ['Done', 'Closed'],
            done_states_mode: 'explicit',
        }))

        const tasks = [
            baseTask({ id: 'ACME-1', status: 'Closed', due_date: '2025-12-01' }),
            baseTask({ id: 'ACME-2', status: 'Doing', due_date: '2025-12-01' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const cards = wrapper.findAll('article.card.task')
        const closed = cards.find(c => c.text().includes('ACME-1'))!
        const doing = cards.find(c => c.text().includes('ACME-2'))!
        expect(closed.text()).not.toContain('Overdue')
        expect(doing.text()).toContain('Overdue')
        wrapper.unmount()
    })

    it('marks Done overdue again when the explicit policy excludes it', async () => {
        // Explicit policy names only Shipped: Done is no longer terminal.
        configStore.statuses.value = ['Todo', 'Doing', 'Done', 'Shipped']
        showConfigMock.mockImplementation(async () => policyConfig({
            issue_states: ['Todo', 'Doing', 'Done', 'Shipped'],
            effective_done_states: ['Shipped'],
            done_states_mode: 'explicit',
        }))

        const tasks = [
            baseTask({ id: 'ACME-1', status: 'Done', due_date: '2025-12-01' }),
            baseTask({ id: 'ACME-2', status: 'Shipped', due_date: '2025-12-01' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const cards = wrapper.findAll('article.card.task')
        const done = cards.find(c => c.text().includes('ACME-1'))!
        const shipped = cards.find(c => c.text().includes('ACME-2'))!
        expect(done.text()).toContain('Overdue')
        expect(shipped.text()).not.toContain('Overdue')
        wrapper.unmount()
    })

    it('does not treat a stale is_done flag as done after an optimistic status change', async () => {
        // Reopened: status Doing, but the cached server metadata still says done.
        const tasks = [baseTask({
            id: 'ACME-1',
            status: 'Doing',
            due_date: '2025-12-01',
            task_state: { is_done: true, due_bucket: null, calendar_day: '2026-01-05' },
        })]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const card = wrapper.find('article.card.task')
        expect(card.text()).toContain('Overdue')
        wrapper.unmount()
    })

    it('seeds the done-column statuses from the effective policy but keeps stored preferences', async () => {
        // No stored preference: seeded with the effective terminal labels.
        const wrapper = mount(Board)
        await flushPromises()
        wrapper.unmount()

        const stored = JSON.parse(window.localStorage.getItem('lotar.doneFilters::ACME') || 'null')
        expect(stored).toBeNull() // seeding must not freeze a preference

        // Explicitly toggling a non-terminal column persists the seeded
        // terminal labels plus the user's choice.
        const wrapper2 = mount(Board)
        await flushPromises()
        const doingToggle = wrapper2.findAll('.done-filter input[type="checkbox"]')
            .find((input) => input.element.closest('label')?.textContent?.includes('Doing'))
        expect(doingToggle).toBeTruthy()
        await doingToggle!.setValue(true)
        expect(JSON.parse(window.localStorage.getItem('lotar.doneFilters::ACME') || 'null')?.statuses).toEqual(['Done', 'Doing'])
        wrapper2.unmount()

        // A stored preference survives a remount even when the policy changes.
        showConfigMock.mockImplementation(async () => policyConfig({ effective_done_states: ['Done', 'Closed'] }))
        invalidateCompletionPolicies()
        const wrapper3 = mount(Board)
        await flushPromises()
        expect(JSON.parse(window.localStorage.getItem('lotar.doneFilters::ACME') || 'null')?.statuses).toEqual(['Done', 'Doing'])
        wrapper3.unmount()
    })

    it('shows a retry banner while retaining cards when a refresh fails with data', async () => {
        const tasks = [baseTask({ id: 'ACME-1', title: 'Alpha' })]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()
        expect(wrapper.findAll('article.card.task').length).toBeGreaterThan(0)

        boardHandleFor({ project: 'ACME' }).error.value = 'flaky network'
        await flushPromises()
        expect(wrapper.find('.refresh-error').exists()).toBe(true)
        expect(wrapper.find('.refresh-error').text()).toContain('flaky network')
        expect(wrapper.findAll('article.card.task').length).toBeGreaterThan(0)
        wrapper.unmount()
    })

    it('shows the full error state (no banner) when a refresh fails without rows', async () => {
        taskMap.value = new Map()
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()
        boardHandleFor({ project: 'ACME' }).error.value = 'hard failure'
        await flushPromises()
        expect(wrapper.find('.refresh-error').exists()).toBe(false)
        expect(wrapper.find('.empty').exists()).toBe(true)
        wrapper.unmount()
    })
    it('shows the first-load loader on a project switch until the new key publishes', async () => {
        const tasks = [baseTask({ id: 'ACME-1', title: 'Alpha' })]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()
        expect(wrapper.findAll('article.card.task').length).toBeGreaterThan(0)

        wrapper.unmount()

        // Switch projects (fresh mount on the new route): the new key's
        // first refresh is held open.
        const betaHandle = boardHandleFor({ project: 'BETA' })
        let finishBeta!: () => void
        betaHandle.refresh.mockImplementationOnce(() => {
            betaHandle.status.value = 'loading'
            return new Promise<void>((resolve) => { finishBeta = resolve })
        })
        routeState.query = { project: 'BETA' }
        const wrapper2 = mount(Board)
        await flushPromises()
        await nextTick()

        // First load of the new key: loader shows, grid is not rendered.
        expect(wrapper2.find('.loader').exists()).toBe(true)
        expect(wrapper2.find('.board.grid').exists()).toBe(false)
        expect(wrapper2.findAll('article.card.task').length).toBe(0)

        const betaTasks = [baseTask({ id: 'BETA-1', title: 'Beta task' })]
        taskMap.value = new Map(betaTasks.map(t => [t.id, t]))
        taskVersion.value++
        betaHandle.hasSnapshot.value = true
        betaHandle.status.value = 'ready'
        finishBeta()
        await flushPromises()
        await nextTick()

        expect(wrapper2.find('.loader').exists()).toBe(false)
        expect(wrapper2.find('.board.grid').exists()).toBe(true)
        expect(wrapper2.findAll('article.card.task').length).toBe(1)
        wrapper2.unmount()
    })
})

describe('Board unknown-status Other column (DEV-68)', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        boardQueryHandles.clear()
        tasksStore.hydrateAll.mockClear()
        tasksStore.getQuery.mockClear()
        boardQueryHandle.refresh.mockClear()
        configStore.statuses.value = ['Todo', 'Doing', 'Done']
        showConfigMock.mockReset()
        showConfigMock.mockImplementation(async () => ({}))
        setStatusMock.mockClear()
        invalidateCompletionPolicies()
        if (typeof localStorage !== 'undefined' && localStorage.clear) {
            localStorage.clear()
        }
    })
    afterEach(() => { vi.useRealTimers() })

    it('renders unknown-status tasks in a synthetic Other column, sorted like configured columns', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', status: 'Todo', modified: '2026-01-02T10:00:00Z' }),
            baseTask({ id: 'ACME-9', status: 'Blocked', modified: '2026-01-04T10:00:00Z' }),
            baseTask({ id: 'ACME-2', status: 'Blocked', modified: '2026-01-04T10:00:00Z' }),
            baseTask({ id: 'ACME-3', status: 'Blocked', modified: '2026-01-03T10:00:00Z' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        // Synthetic column is addressable by kind; its data-status keeps the
        // reserved sentinel.
        const otherCol = wrapper.find('.board.grid > .col.column[data-column-kind="other"]')
        expect(otherCol.exists()).toBe(true)
        expect(otherCol.attributes('data-status')).toBe('__other__')

        // Header count and title.
        expect(otherCol.find('.col-header strong').text()).toBe('Other')
        expect(otherCol.find('.col-header .muted').text()).toContain('3')

        // Same deterministic order as configured columns: modified desc with
        // id-asc tie-breaks.
        const ids = otherCol.findAll('article.card.task').map(c => c.find('.id').text())
        expect(ids).toEqual(['ACME-2', 'ACME-9', 'ACME-3'])

        // Every project task renders; grid tracks equal rendered columns.
        expect(wrapper.findAll('article.card.task').length).toBe(4)
        expect(wrapper.findAll('.board.grid > .col.column').length).toBe(4)
        expect(wrapper.find('.board.grid').attributes('style')).toContain('repeat(4')
        wrapper.unmount()
    })

    it('includes the synthetic column in grouped swimlanes, group counts, and grid tracks', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', assignee: 'alice', status: 'Todo' }),
            baseTask({ id: 'ACME-2', assignee: 'bob', status: 'Blocked' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const select = wrapper.find('[data-testid="board-groupby"]')
        await select.setValue('assignee')
        await flushPromises()

        // A group whose only task has an unknown status counts and renders.
        const headers = wrapper.findAll('.swimlane-header')
        const bobHeader = headers.find(h => h.text().includes('bob'))
        expect(bobHeader).toBeTruthy()
        expect(bobHeader!.find('.swimlane-count').text()).toBe('1')

        // Cells exist per group per column, including the synthetic one.
        expect(wrapper.findAll('.column-group-cell').length).toBe(8) // 2 groups x 4 columns
        expect(wrapper.findAll('.column-group-cell[data-column-kind="other"]').length).toBe(2)
        expect(wrapper.findAll('article.card.task').length).toBe(2)
        expect(wrapper.findAll('.board-col-header').length).toBe(4)
        expect(wrapper.find('.board.grid').attributes('style')).toContain('repeat(4')
        wrapper.unmount()
    })

    it('keeps a configured literal __other__ status writable and distinct from the synthetic column', async () => {
        configStore.statuses.value = ['Todo', 'Doing', 'Done', '__other__']
        const tasks = [
            baseTask({ id: 'ACME-1', status: '__other__' }),
            baseTask({ id: 'ACME-2', status: 'Mystery' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        // Configured literal __other__ stays a real status column; the
        // synthetic fallback is distinguished by kind, not by renaming.
        const configured = wrapper.find('.board.grid > .col.column[data-column-kind="status"][data-status="__other__"]')
        expect(configured.exists()).toBe(true)
        expect(configured.find('.col-header strong').text()).toBe('__other__')
        expect(configured.find('article.card.task').text()).toContain('ACME-1')

        const synthetic = wrapper.find('.board.grid > .col.column[data-column-kind="other"]')
        expect(synthetic.exists()).toBe(true)
        expect(synthetic.find('article.card.task').text()).toContain('ACME-2')

        // Five distinct columns: 4 configured + 1 synthetic.
        expect(wrapper.findAll('.board.grid > .col.column').length).toBe(5)

        // Dropping on the configured literal writes the real status value.
        const mysteryCard = wrapper.findAll('article.card.task').find(c => c.text().includes('ACME-2'))!
        await mysteryCard.trigger('dragstart')
        await configured.trigger('drop')
        await flushPromises()
        expect(setStatusMock).toHaveBeenCalledWith('ACME-2', '__other__')
        wrapper.unmount()
    })

    it('never writes a status for drops on the synthetic Other column', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', status: 'Todo' }),
            baseTask({ id: 'ACME-2', status: 'Mystery' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const synthetic = wrapper.find('.board.grid > .col.column[data-column-kind="other"]')
        expect(synthetic.exists()).toBe(true)

        const card = wrapper.findAll('article.card.task').find(c => c.text().includes('ACME-2'))!
        await card.trigger('dragstart')
        await synthetic.trigger('drop')
        await synthetic.trigger('keydown', { key: 'Enter' })
        await wrapper.find('.column[data-column-kind="status"][data-status="Todo"]').trigger('keydown', { key: 'Enter' })
        await flushPromises()

        expect(setStatusMock).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('keeps a configured Other status selectable in done filters while the synthetic column is not', async () => {
        configStore.statuses.value = ['Todo', 'Doing', 'Done', 'Other']
        showConfigMock.mockImplementation(async () => policyConfig({
            issue_states: ['Todo', 'Doing', 'Done', 'Other'],
            effective_done_states: ['Done'],
        }))
        const tasks = [
            baseTask({ id: 'ACME-1', status: 'Other' }),
            baseTask({ id: 'ACME-2', status: 'Blocked' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        // Configured "Other" is a real column; the synthetic fallback coexists.
        const configuredOther = wrapper.find('.board.grid > .col.column[data-column-kind="status"][data-status="Other"]')
        expect(configuredOther.exists()).toBe(true)
        expect(configuredOther.find('article.card.task').text()).toContain('ACME-1')
        expect(wrapper.find('.board.grid > .col.column[data-column-kind="other"]').exists()).toBe(true)

        // Done-column checkboxes list configured labels only — exactly once
        // each, never the synthetic fallback.
        const labels = wrapper.findAll('.done-filter input[type="checkbox"]')
            .map(input => input.element.closest('label')?.textContent?.trim())
        expect(labels).toEqual(['Todo', 'Doing', 'Done', 'Other'])

        // Seeding and persistence stay untouched by unknown-status tasks.
        const doingToggle = wrapper.findAll('.done-filter input[type="checkbox"]')
            .find(input => input.element.closest('label')?.textContent?.includes('Doing'))
        await doingToggle!.setValue(true)
        expect(JSON.parse(window.localStorage.getItem('lotar.doneFilters::ACME') || 'null')?.statuses).toEqual(['Done', 'Doing'])
        wrapper.unmount()
    })

    it('paginates the synthetic Other column like normal columns', async () => {
        const tasks: TaskDTO[] = []
        for (let i = 1; i <= 32; i++) {
            tasks.push(baseTask({ id: `ACME-${i}`, status: 'Blocked', modified: `2026-01-02T${String(i % 24).padStart(2, '0')}:00:00Z` }))
        }
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        const otherCol = wrapper.find('.board.grid > .col.column[data-column-kind="other"]')
        expect(otherCol.findAll('article.card.task').length).toBe(30)
        const showMoreBtn = otherCol.find('.show-more-btn')
        expect(showMoreBtn.exists()).toBe(true)
        expect(showMoreBtn.text()).toContain('Show 2 more')

        await showMoreBtn.trigger('click')
        await flushPromises()
        expect(wrapper.find('.board.grid > .col.column[data-column-kind="other"]').findAll('article.card.task').length).toBe(32)
        expect(wrapper.find('.board.grid > .col.column[data-column-kind="other"]').find('.show-more-btn').exists()).toBe(false)
        wrapper.unmount()
    })

    it('excludes foreign-project unknown-status tasks from the project-scoped board', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', status: 'Todo' }),
            baseTask({ id: 'ACME-2', status: 'Blocked' }),
            baseTask({ id: 'BETA-1', status: 'Blocked' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        const wrapper = mount(Board)
        await flushPromises()

        expect(wrapper.findAll('article.card.task').length).toBe(2)
        expect(wrapper.text()).not.toContain('BETA-1')
        const otherCol = wrapper.find('.board.grid > .col.column[data-column-kind="other"]')
        expect(otherCol.find('.col-header .muted').text()).toContain('1')
        wrapper.unmount()

        routeState.query = { project: 'BETA' }
        const wrapper2 = mount(Board)
        await flushPromises()
        const betaOther = wrapper2.find('.board.grid > .col.column[data-column-kind="other"]')
        expect(betaOther.exists()).toBe(true)
        expect(betaOther.find('article.card.task').text()).toContain('BETA-1')
        wrapper2.unmount()
        routeState.query = { project: 'ACME' }
    })

    it('renders only the active query membership, not every cached insertion', async () => {
        const tasks = [
            baseTask({ id: 'ACME-1', status: 'Todo' }),
            baseTask({ id: 'ACME-2', status: 'Todo' }),
        ]
        taskMap.value = new Map(tasks.map(t => [t.id, t]))
        taskVersion.value++

        // Server-scoped query membership: the base ACME query contains only
        // ACME-1 even though the shared cache also holds ACME-2.
        boardHandleFor({ project: 'ACME' }, new Set(['ACME-1']))
        boardHandleFor({ project: 'ACME', due: 'overdue' }, new Set(['ACME-2']))

        const wrapper = mount(Board)
        await flushPromises()
        expect(wrapper.text()).toContain('ACME-1')
        expect(wrapper.text()).not.toContain('ACME-2')

        // Switching filters adopts the new query and its membership only.
        const filters = wrapper.findComponent({ name: 'FilterBar' })
        filters.vm.$emit('update:value', { due: 'overdue' })
        await vi.advanceTimersByTimeAsync(150)
        await flushPromises()

        expect(wrapper.text()).not.toContain('ACME-1')
        expect(wrapper.text()).toContain('ACME-2')
        wrapper.unmount()
    })
})

describe('Board semantic swimlane order (DEV-87)', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        routeState.query = { project: 'ACME' }
        taskMap.value = new Map()
        taskVersion.value = 0
        boardQueryHandles.clear()
        configStore.statuses.value = ['Todo', 'Doing', 'Done']
        configStore.priorities.value = ['low', 'med', 'high']
        configStore.types.value = ['task']
        configStore.refresh.mockReset().mockResolvedValue(undefined)
        showConfigMock.mockReset().mockResolvedValue({})
        invalidateCompletionPolicies()
        localStorage.clear()
    })
    afterEach(() => {
        configStore.priorities.value = ['low', 'med', 'high']
        configStore.types.value = ['task']
        vi.useRealTimers()
    })

    it.each(['priority', 'type'] as const)('uses configured %s order, then unknown values and missing last, including Other', async mode => {
        const configured = mode === 'priority' ? ['Later', 'Normal', 'Urgent'] : ['Feature', 'Bug', 'Chore']
        const expected = mode === 'priority' ? [...configured].reverse() : configured
        const source = mode === 'priority' ? configStore.priorities : configStore.types
        source.value = ['Unused', ...configured]
        const field = mode === 'priority' ? 'priority' : 'task_type'
        const tasks = [...configured, 'Zulu', 'Alpha', ''].map((label, index) => baseTask({
            id: `ACME-${index + 1}`, [field]: label, status: index === 1 ? 'Legacy' : 'Todo',
        }))
        tasks.push(baseTask({ id: 'BETA-1', [field]: 'Foreign' }))
        taskMap.value = new Map(tasks.map(task => [task.id, task]))
        const wrapper = mount(Board)
        try {
            await flushPromises()
            await wrapper.find('[data-testid="board-groupby"]').setValue(mode)
            const labels = () => wrapper.findAll('.swimlane-label').map(label => label.text())
            expect(labels()).toEqual([...expected, 'Alpha', 'Zulu', '(none)'])
            expect(wrapper.findAll('.swimlane-count').map(count => count.text())).toEqual(Array(6).fill('1'))
            expect(wrapper.findAll('article.task')).toHaveLength(6)
            expect(wrapper.findAll('[data-column-kind="other"] article.task')).toHaveLength(1)

            await wrapper.findAll('.swimlane-header')[0]!.trigger('click')
            source.value = [...configured].reverse()
            await nextTick()
            const reordered = mode === 'priority' ? configured : [...configured].reverse()
            expect(labels()).toEqual(reordered.concat(['Alpha', 'Zulu', '(none)']))
            expect(wrapper.find('.swimlane-header.collapsed .swimlane-label').text()).toBe(expected[0])
            expect(wrapper.findAll('article.task')).toHaveLength(5)
        } finally {
            wrapper.unmount()
        }
    })

    it('keeps assignees alphabetical and missing values last', async () => {
        const tasks = ['zoe', 'amy', '', 'ben'].map((assignee, index) => baseTask({ id: `ACME-${index + 1}`, assignee }))
        taskMap.value = new Map(tasks.map(task => [task.id, task]))
        const wrapper = mount(Board)
        try {
            await flushPromises()
            await wrapper.find('[data-testid="board-groupby"]').setValue('assignee')
            expect(wrapper.findAll('.swimlane-label').map(label => label.text())).toEqual(['amy', 'ben', 'zoe', '(none)'])
        } finally {
            wrapper.unmount()
        }
    })

    it('matches backend ASCII case/separators without merging distinct Unicode labels', async () => {
        configStore.priorities.value = ['\u00e4', 'Very High', 'Zed', '\u00c4', 'Low', 'very-high']
        const tasks = ['VERY_HIGH', 'low', '\u00c4', '\u00e4', 'Zed'].map((priority, index) => baseTask({ id: `ACME-${index + 1}`, priority }))
        taskMap.value = new Map(tasks.map(task => [task.id, task]))
        const wrapper = mount(Board)
        try {
            await flushPromises()
            await wrapper.find('[data-testid="board-groupby"]').setValue('priority')
            expect(wrapper.findAll('.swimlane-label').map(label => label.text())).toEqual(['low', '\u00c4', 'Zed', 'VERY_HIGH', '\u00e4'])
        } finally {
            wrapper.unmount()
        }
    })

    it('falls back to alphabetical order when the configuration has no values', async () => {
        configStore.priorities.value = []
        const tasks = ['Medium', '', 'High', 'Low'].map((priority, index) => baseTask({ id: `ACME-${index + 1}`, priority }))
        taskMap.value = new Map(tasks.map(task => [task.id, task]))
        const wrapper = mount(Board)
        try {
            await flushPromises()
            await wrapper.find('[data-testid="board-groupby"]').setValue('priority')
            expect(wrapper.findAll('.swimlane-label').map(label => label.text())).toEqual(['High', 'Low', 'Medium', '(none)'])
        } finally {
            wrapper.unmount()
        }
    })

    it('puts default Critical and High lanes above Medium and Low', async () => {
        configStore.priorities.value = ['Low', 'Medium', 'High', 'Critical']
        const tasks = ['Low', 'High', 'Critical', 'Medium'].map((priority, index) => baseTask({ id: `ACME-${index + 1}`, priority }))
        taskMap.value = new Map(tasks.map(task => [task.id, task]))
        const wrapper = mount(Board)
        try {
            await flushPromises()
            await wrapper.find('[data-testid="board-groupby"]').setValue('priority')
            expect(wrapper.findAll('.swimlane-label').map(label => label.text())).toEqual(['Critical', 'High', 'Medium', 'Low'])
        } finally {
            wrapper.unmount()
        }
    })

    it('orders only the active query members, not unrelated cached groups', async () => {
        configStore.priorities.value = ['High', 'Medium', 'Low']
        const tasks = [baseTask({ id: 'ACME-1', priority: 'High' }), baseTask({ id: 'ACME-2', priority: 'Medium', status: 'Legacy' })]
        taskMap.value = new Map(tasks.map(task => [task.id, task]))
        boardHandleFor({ project: 'ACME' }, new Set(['ACME-2']))
        const wrapper = mount(Board)
        try {
            await flushPromises()
            await wrapper.find('[data-testid="board-groupby"]').setValue('priority')
            expect(wrapper.findAll('.swimlane-label').map(label => label.text())).toEqual(['Medium'])
            expect(wrapper.find('.swimlane-count').text()).toBe('1')
            expect(wrapper.findAll('article.task')).toHaveLength(1)
        } finally {
            wrapper.unmount()
        }
    })
})
