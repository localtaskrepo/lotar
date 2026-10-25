import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { reactive } from 'vue'
import type { TaskDTO } from '../api/types'
import { invalidateCompletionPolicies } from '../composables/useCompletionPolicy'
import { _resetTaskStore } from '../composables/useTaskStore'

const api = vi.hoisted(() => ({
    listTasks: vi.fn(),
    showConfig: vi.fn(),
    whoami: vi.fn(),
    projectStats: vi.fn(),
    activityFeed: vi.fn(),
    listSprints: vi.fn(),
    sprintList: vi.fn(),
}))
vi.mock('../api/client', () => ({ api }))
vi.mock('../components/toast', () => ({ showToast: vi.fn() }))
vi.mock('../composables/useSse', () => ({
    useSse: vi.fn(() => ({ on: vi.fn(), off: vi.fn(), close: vi.fn() })),
}))
vi.mock('../composables/useSprints', async () => {
    const { ref } = await import('vue')
    return {
        useSprints: () => ({ sprints: ref([]), refresh: vi.fn(async () => {}) }),
        useSprintFilterOptions: () => ref([]),
    }
})
const openTaskPanel = vi.hoisted(() => vi.fn())
vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({ openTaskPanel }),
}))

const routeState = reactive({ path: '/insights', query: {} as Record<string, string> })
const routerPush = vi.hoisted(() => vi.fn())
const routerReplace = vi.hoisted(() => vi.fn(async () => {}))
vi.mock('vue-router', () => ({
    useRoute: () => routeState,
    useRouter: () => ({ push: routerPush, replace: routerReplace }),
    RouterLink: { name: 'RouterLink', props: ['to'], template: '<a><slot /></a>' },
}))

const DAY = 86_400_000
const NOW = Date.parse('2026-01-05T12:00:00Z')
const iso = (offset: number) => new Date(NOW + offset * DAY).toISOString()

function task(id: string, overrides: Partial<TaskDTO> = {}): TaskDTO {
    return {
        id,
        title: `Task ${id}`,
        status: 'Todo',
        priority: 'Medium',
        task_type: 'Feature',
        created: iso(-20),
        modified: iso(-1),
        tags: [],
        relationships: {},
        comments: [],
        references: [],
        sprints: [],
        history: [],
        custom_fields: {},
        ...overrides,
    } as TaskDTO
}

function deferred<T>() {
    let resolve!: (value: T) => void
    const promise = new Promise<T>((yes) => (resolve = yes))
    return { promise, resolve }
}

describe('ProjectInsights', () => {
    let wrapper: VueWrapper<any> | undefined

    async function mountInsights() {
        const { default: ProjectInsights } = await import('../pages/ProjectInsights.vue')
        wrapper = mount(ProjectInsights, { global: { stubs: { FilterBar: true } } })
        await flushPromises()
        return wrapper
    }

    function tile(label: string): string {
        const found = wrapper!.findAll('.stat-tile').find((el) => el.find('.stat-tile__label').text() === label)
        expect(found, `tile ${label}`).toBeTruthy()
        return found!.find('.stat-tile__value').text()
    }

    async function openLens(key: string) {
        await wrapper!.get(`[data-testid="insights-tab-${key}"]`).trigger('click')
        await flushPromises()
    }

    beforeEach(() => {
        vi.clearAllMocks()
        vi.useFakeTimers({ toFake: ['Date'] })
        vi.setSystemTime(NOW)
        _resetTaskStore()
        localStorage.clear()
        routeState.path = '/insights'
        routeState.query = {}
        api.listTasks.mockResolvedValue({ total: 0, tasks: [] })
        api.whoami.mockResolvedValue('ana')
        api.showConfig.mockImplementation(async (project?: string) => ({
            issue_states: ['Todo', 'Doing', 'Done', 'Closed'],
            issue_priorities: ['Low', 'Medium', 'High'],
            issue_types: ['Feature', 'Bug'],
            effective_done_states: project === 'B' ? ['Closed'] : ['Done'],
            done_states_mode: 'explicit',
            task_calendar_day: '2026-01-05',
        }))
        invalidateCompletionPolicies()
    })

    afterEach(() => {
        wrapper?.unmount()
        wrapper = undefined
        vi.useRealTimers()
    })

    it('loads only the task list, never per-project stats or the git activity feed', async () => {
        await mountInsights()
        expect(api.listTasks).toHaveBeenCalled()
        expect(api.projectStats).not.toHaveBeenCalled()
        expect(api.activityFeed).not.toHaveBeenCalled()
    })

    it('counts open and overdue work with each project policy, excluding due-today', async () => {
        api.listTasks.mockResolvedValue({
            total: 5,
            tasks: [
                task('A-1', { due_date: '2026-01-01' }),
                task('A-2', { status: 'Closed', due_date: '2026-01-01' }),
                task('B-1', { status: 'Closed', due_date: '2026-01-01' }),
                task('B-2', { due_date: '2026-01-05' }),
                task('A-3', { status: 'Done' }),
            ],
        })
        await mountInsights()
        // A uses Done and B uses Closed as terminal, so A-2 (Closed) is open.
        expect(tile('Open')).toBe('3')
        expect(tile('Overdue')).toBe('2')

        const overdueTile = wrapper!.findAll('.stat-tile').find((el) => el.text().includes('Overdue'))!
        await overdueTile.trigger('click')
        expect(routerPush).toHaveBeenCalledWith({ path: '/', query: { due: 'overdue' } })
    })

    it('derives cycle and lead time from status history', async () => {
        api.listTasks.mockResolvedValue({
            total: 1,
            tasks: [
                task('A-1', {
                    status: 'Done',
                    created: iso(-10),
                    history: [
                        { at: iso(-8), actor: 'ana', changes: [{ field: 'status', old: 'Todo', new: 'Doing' }] },
                        { at: iso(-4), actor: 'ana', changes: [{ field: 'status', old: 'Doing', new: 'Done' }] },
                    ],
                }),
            ],
        })
        await mountInsights()
        expect(tile('Completed')).toBe('1')
        await openLens('delivery')
        expect(tile('Cycle time (median)')).toBe('4d')
        expect(tile('Lead time (median)')).toBe('6d')
    })

    it('shows the signed-in person their own work', async () => {
        api.listTasks.mockResolvedValue({
            total: 3,
            tasks: [
                task('A-1', { assignee: 'ana', due_date: '2026-01-02' }),
                task('A-2', { assignee: 'ana' }),
                task('A-3', { assignee: 'ben' }),
            ],
        })
        await mountInsights()
        await openLens('mine')
        expect(tile('Assigned to me')).toBe('2')
        expect(tile('Overdue')).toBe('1')
        await wrapper!.findAll('.task-table__link')[0]!.trigger('click')
        expect(openTaskPanel).toHaveBeenCalledWith({ taskId: 'A-1' })
    })

    it('records the chosen view and period in the URL while on the page', async () => {
        await mountInsights()
        routerReplace.mockClear()
        await openLens('quality')
        expect(routerReplace).toHaveBeenLastCalledWith({
            path: '/insights',
            query: { view: 'quality', period: '30d' },
        })
        expect(localStorage.getItem('lotar.insights.view')).toBe('quality')
    })

    it('never navigates back to Insights after the user has left (DEV-187)', async () => {
        const slow = deferred<{ total: number; tasks: TaskDTO[] }>()
        api.listTasks.mockReturnValue(slow.promise)
        const { default: ProjectInsights } = await import('../pages/ProjectInsights.vue')
        wrapper = mount(ProjectInsights, { global: { stubs: { FilterBar: true } } })
        await flushPromises()
        // The user navigates elsewhere while the first load is still running.
        routeState.path = '/sprints'
        slow.resolve({ total: 1, tasks: [task('A-1')] })
        await flushPromises()
        expect(routerReplace).not.toHaveBeenCalled()
        expect(routerPush).not.toHaveBeenCalled()

        // Same after unmounting mid-load.
        wrapper.unmount()
        wrapper = undefined
        routeState.path = '/insights'
        const late = deferred<{ total: number; tasks: TaskDTO[] }>()
        api.listTasks.mockReturnValue(late.promise)
        _resetTaskStore()
        const second = mount(ProjectInsights, { global: { stubs: { FilterBar: true } } })
        await flushPromises()
        second.unmount()
        routerReplace.mockClear()
        late.resolve({ total: 0, tasks: [] })
        await flushPromises()
        expect(routerReplace).not.toHaveBeenCalled()
    })

    it('offers a retry when the first load fails', async () => {
        api.listTasks.mockRejectedValueOnce(new Error('boom'))
        await mountInsights()
        const retry = wrapper!.findAll('button').find((button) => button.text() === 'Retry')
        expect(retry).toBeTruthy()
        api.listTasks.mockResolvedValue({ total: 1, tasks: [task('A-1')] })
        await retry!.trigger('click')
        await flushPromises()
        expect(tile('Open')).toBe('1')
    })
})
