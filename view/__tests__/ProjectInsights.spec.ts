import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'
import { defineComponent, h, reactive, ref } from 'vue'
import type { ProjectStatsDTO, TaskDTO } from '../api/types'
import { _resetActivityStore } from '../composables/useActivity'
import { invalidateCompletionPolicies } from '../composables/useCompletionPolicy'
import { _resetTaskStore } from '../composables/useTaskStore'

const api = vi.hoisted(() => ({
    listTasks: vi.fn(),
    projectStats: vi.fn(),
    activityFeed: vi.fn(),
    showConfig: vi.fn(),
}))
vi.mock('../api/client', () => ({ api }))

const showToast = vi.hoisted(() => vi.fn())
vi.mock('../components/toast', () => ({ showToast }))

const projectsRef = ref<Array<{ prefix: string; name: string }>>([])
vi.mock('../composables/useProjects', () => ({
    useProjects: () => ({ projects: projectsRef, refresh: vi.fn(async () => {}) }),
}))

const routeState = reactive({ path: '/insights', query: {} as Record<string, string> })
const routerPush = vi.hoisted(() => vi.fn())
vi.mock('vue-router', () => ({
    useRoute: () => routeState,
    useRouter: () => ({ push: routerPush, replace: vi.fn(async () => {}) }),
}))

vi.mock('../composables/useSse', () => ({
    useSse: vi.fn(() => ({ on: vi.fn(), off: vi.fn(), close: vi.fn() })),
}))

const barProps: Array<Record<string, any>> = []
const BarChartStub = defineComponent({
    name: 'BarChart',
    props: ['series', 'width', 'height'] as any,
    setup(props: any) {
        barProps.push(props)
        return () => h('div', { class: 'bar-chart-stub' })
    },
})
const PieChartStub = defineComponent({
    name: 'PieChart',
    props: ['data', 'legend', 'size'] as any,
    setup: () => () => h('div', { class: 'pie-chart-stub' }),
})

function deferred<T>() {
    let resolve!: (value: T) => void
    let reject!: (error: Error) => void
    const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
    return { promise, resolve, reject }
}

function makeTask(id: string, overrides: Partial<TaskDTO> = {}): TaskDTO {
    return {
        id,
        title: `Task ${id}`,
        status: 'Todo' as any,
        priority: 'Medium' as any,
        task_type: 'Task' as any,
        created: '2026-01-01T00:00:00Z',
        modified: '2026-01-01T00:00:00Z',
        tags: [],
        relationships: {} as any,
        comments: [],
        references: [],
        sprints: [],
        history: [],
        custom_fields: {},
        ...overrides,
    } as TaskDTO
}

function stats(open_count: number, recent_modified?: string): ProjectStatsDTO {
    return {
        name: `project-${open_count}`,
        open_count,
        done_count: 0,
        recent_modified: recent_modified ?? null,
        tags_top: [],
    }
}

function feedChange(task: string, changes: number): any {
    return {
        commit: `sha-${task}-${changes}`,
        author: 'Ada Lovelace',
        email: 'ada@example.com',
        date: new Date().toISOString(),
        message: `commit for ${task}`,
        task_id: task,
        task_title: `Task ${task}`,
        history: [
            {
                at: new Date().toISOString(),
                actor: 'ada',
                changes: Array.from({ length: changes }, (_, i) => ({
                    kind: 'status',
                    field: `field-${i}`,
                    old: 'Todo',
                    new: 'Doing',
                })),
            },
        ],
    }
}

function seriesTotal(props: Record<string, any>): number {
    return (props.series as Array<{ total: number }>).reduce((sum, item) => sum + item.total, 0)
}

function currentSeriesTotal(): number {
    expect(barProps.length).toBeGreaterThan(0)
    return seriesTotal(barProps[barProps.length - 1]!)
}

describe('ProjectInsights query scoping (DEV-66)', () => {
    let wrapper: VueWrapper<any>
    let ProjectInsights: (typeof import('../pages/ProjectInsights.vue'))['default']
    let listTasksMock: Mock
    let projectStatsMock: Mock
    let activityFeedMock: Mock
    let showConfigMock: Mock

    function retryButton() {
        const button = wrapper.findAll('button').find(b => b.text().includes('Retry'))
        expect(button).toBeTruthy()
        return button!
    }

    function tile(label: string): string {
        const el = wrapper.findAll('.summary-tile').find(t => t.text().includes(label))
        return el ? el.find('.summary-value').text() : ''
    }

    function overviewRow(prefix: string): string {
        const rows = wrapper.findAll('table.distribution tbody tr')
        const row = rows.filter(r => r.text().includes(prefix))
        return row.length ? row[row.length - 1]!.text() : ''
    }

    async function mountInsights() {
        const mod = await import('../pages/ProjectInsights.vue')
        ProjectInsights = mod.default
        wrapper = mount(ProjectInsights, {
            global: { stubs: { BarChart: BarChartStub, PieChart: PieChartStub } },
        })
        await flushPromises()
        return wrapper
    }

    beforeEach(async () => {
        vi.clearAllMocks()
        _resetTaskStore()
        _resetActivityStore()
        routeState.path = '/insights'
        routeState.query = {}
        projectsRef.value = [
            { prefix: 'A', name: 'Alpha' },
            { prefix: 'B', name: 'Beta' },
        ]
        barProps.length = 0
        const client = await import('../api/client')
        listTasksMock = client.api.listTasks as unknown as Mock
        projectStatsMock = client.api.projectStats as unknown as Mock
        activityFeedMock = client.api.activityFeed as unknown as Mock
        showConfigMock = client.api.showConfig as unknown as Mock
        listTasksMock.mockResolvedValue({ total: 0, tasks: [] })
        projectStatsMock.mockResolvedValue(stats(0))
        activityFeedMock.mockResolvedValue([])
        showConfigMock.mockImplementation(async (project?: string) => ({
            issue_states: ['Todo', 'Doing', 'Done', 'Closed'],
            effective_done_states: project === 'B' ? ['Closed'] : ['Done'],
            done_states_mode: 'explicit',
            task_calendar_day: '2026-01-05',
        }))
        invalidateCompletionPolicies()
    })

    afterEach(() => {
        if (wrapper) {
            wrapper.unmount()
            wrapper = undefined as any
        }
    })

    it('mounts in global scope, filters nothing, and fills the overview from per-project stats', async () => {
        const tasks = [makeTask('A-1', { tags: ['x'] }), makeTask('A-2'), makeTask('B-1')]
        listTasksMock.mockResolvedValue({ total: tasks.length, tasks })
        projectStatsMock.mockImplementation((prefix: string) =>
            Promise.resolve(prefix === 'A' ? stats(3) : stats(9)))

        await mountInsights()

        expect(listTasksMock).toHaveBeenCalledWith(expect.objectContaining({ limit: 200, offset: 0 }))
        expect(tile('Total tasks')).toBe('3')
        expect(overviewRow('Alpha')).toContain('3')
        expect(overviewRow('Beta')).toContain('9')
        const feedParams = activityFeedMock.mock.calls[0]![0] as Record<string, unknown>
        expect(feedParams.limit).toBe(400)
        expect(feedParams).not.toHaveProperty('project')
    })

    it('counts no feed activity when the tag filter matches zero tasks', async () => {
        const tasks = [makeTask('A-1', { tags: ['x'] }), makeTask('A-2', { tags: ['y'] })]
        listTasksMock.mockResolvedValue({ total: 2, tasks })
        activityFeedMock.mockResolvedValue([feedChange('A-1', 2), feedChange('A-2', 3)])

        await mountInsights()
        // 5 feed changes over 30 days -> 0.2 updates/day, always rendered.
        expect(tile('Activity (avg/day)')).toBe('0.2')

        await wrapper.find('input[placeholder="Filter tags (comma separated)"]').setValue('nomatch')
        await nextTickP()

        expect(wrapper.text()).toContain('No tasks match the current filters.')
        expect(tile('Total tasks')).toBe('0')
        expect(tile('Activity (avg/day)')).toBe('0.0')
    })

    it('untagged-only filter excludes tagged tasks', async () => {
        const tasks = [makeTask('A-1', { tags: ['x'] }), makeTask('A-2'), makeTask('B-1')]
        listTasksMock.mockResolvedValue({ total: tasks.length, tasks })

        await mountInsights()
        const input = wrapper.find('input[placeholder="Filter tags (comma separated)"]')

        await input.setValue('untagged')
        await nextTickP()
        expect(tile('Total tasks')).toBe('2')
        expect(tile('Tagged')).toBe('0')

        await input.setValue('x')
        await nextTickP()
        expect(tile('Total tasks')).toBe('1')

        await input.setValue('')
        await nextTickP()
        expect(tile('Total tasks')).toBe('3')
    })

    it('rapid project switches settle on the selected scope with out-of-order responses', async () => {
        const base = [makeTask('A-1'), makeTask('A-2'), makeTask('B-1')]
        listTasksMock.mockResolvedValue({ total: base.length, tasks: base })
        await mountInsights()

        const projectA = deferred<{ total: number; tasks: TaskDTO[] }>()
        const projectB = deferred<{ total: number; tasks: TaskDTO[] }>()
        listTasksMock.mockImplementation((filter: any) => {
            if (filter?.project === 'A') return projectA.promise
            if (filter?.project === 'B') return projectB.promise
            return Promise.resolve({ total: base.length, tasks: base })
        })

        const select = wrapper.find('select.ui-select')
        await select.setValue('A')
        await flushPromises()
        expect(wrapper.text()).toContain('Loading insights')

        await select.setValue('B')
        await flushPromises()

        projectB.resolve({ total: 2, tasks: [makeTask('B-1'), makeTask('B-2')] })
        await flushPromises()
        expect(tile('Total tasks')).toBe('2')

        projectA.resolve({ total: 5, tasks: Array.from({ length: 5 }, (_, i) => makeTask(`A-${i}`)) })
        await flushPromises()
        expect(tile('Total tasks')).toBe('2')

        expect(listTasksMock).toHaveBeenCalledWith(expect.objectContaining({ project: 'B' }))
        const lastFeed = activityFeedMock.mock.calls[activityFeedMock.mock.calls.length - 1]![0] as Record<string, unknown>
        expect(lastFeed.project).toBe('B')
    })

    it('renders a retryable error state for a failed project switch, not a false empty', async () => {
        listTasksMock.mockResolvedValue({ total: 2, tasks: [makeTask('A-1'), makeTask('A-2')] })
        await mountInsights()
        expect(tile('Total tasks')).toBe('2')

        listTasksMock.mockImplementation((filter: any) =>
            filter?.project === 'A'
                ? Promise.reject(new Error('A unavailable'))
                : Promise.resolve({ total: 0, tasks: [] }))

        await wrapper.find('select.ui-select').setValue('A')
        await flushPromises()

        expect(wrapper.text()).toContain("We couldn't load tasks")
        expect(wrapper.text()).toContain('A unavailable')
        expect(wrapper.text()).not.toContain('No tasks match the current filters.')
        expect(showToast).not.toHaveBeenCalled()

        listTasksMock.mockImplementation((filter: any) =>
            filter?.project === 'A'
                ? Promise.resolve({ total: 1, tasks: [makeTask('A-9')] })
                : Promise.resolve({ total: 0, tasks: [] }))

        await retryButton().trigger('click')
        await flushPromises()

        expect(tile('Total tasks')).toBe('1')
        expect(wrapper.text()).not.toContain("We couldn't load tasks")
        expect(wrapper.text()).not.toContain('No tasks match the current filters.')
    })

    it('preserves valid rows with an error banner when a same-scope refresh fails', async () => {
        listTasksMock.mockImplementation((filter: any) =>
            filter?.project === 'A'
                ? Promise.resolve({ total: 2, tasks: [makeTask('A-1'), makeTask('A-2')] })
                : Promise.resolve({ total: 0, tasks: [] }))

        await mountInsights()
        await wrapper.find('select.ui-select').setValue('A')
        await flushPromises()
        expect(tile('Total tasks')).toBe('2')

        listTasksMock.mockImplementation((filter: any) =>
            filter?.project === 'A'
                ? Promise.reject(new Error('flaky reload'))
                : Promise.resolve({ total: 0, tasks: [] }))

        await wrapper.find('button[title="Refresh insights"]').trigger('click')
        await flushPromises()

        const banner = wrapper.find('.insights-refresh-error')
        expect(banner.exists()).toBe(true)
        expect(banner.text()).toContain('flaky reload')
        expect(banner.text()).toContain('Retry')
        expect(tile('Total tasks')).toBe('2')
        expect(wrapper.text()).not.toContain("We couldn't load tasks")
        expect(wrapper.text()).not.toContain('No tasks match the current filters.')
    })

    it('shows the queued loader during startup and a retryable error after an initial load failure', async () => {
        const initial = deferred<{ total: number; tasks: TaskDTO[] }>()
        listTasksMock.mockReturnValue(initial.promise)
        await mountInsights()

        expect(wrapper.text()).toContain('Loading insights')
        expect(wrapper.text()).not.toContain('No tasks match the current filters.')

        initial.reject(new Error('startup down'))
        await flushPromises()

        expect(wrapper.text()).not.toContain('Loading insights')
        expect(wrapper.text()).toContain("We couldn't load tasks")
        expect(wrapper.text()).toContain('startup down')
        expect(wrapper.text()).not.toContain('No tasks match the current filters.')
        expect(retryButton()).toBeTruthy()
    })

    it('Reload refetches visible project stats and a stale fill cannot overwrite the fresh value', async () => {
        const projectC1 = deferred<ProjectStatsDTO>()
        const projectC2 = deferred<ProjectStatsDTO>()
        const queue: Record<string, Array<Promise<ProjectStatsDTO> | ProjectStatsDTO>> = {
            A: [stats(3), stats(5)],
            B: [stats(9), stats(9)],
            C: [projectC1.promise, projectC2.promise],
        }
        projectStatsMock.mockImplementation((prefix: string) => {
            const next = queue[prefix]!.shift()
            return next instanceof Promise ? next : Promise.resolve(next)
        })

        listTasksMock.mockResolvedValue({ total: 1, tasks: [makeTask('A-1')] })
        await mountInsights()
        expect(overviewRow('Alpha')).toContain('3')

        // A new project appears: the watcher starts an older fill-missing fetch.
        projectsRef.value = [...projectsRef.value, { prefix: 'C', name: 'Gamma' }]
        await flushPromises()
        expect(projectStatsMock).toHaveBeenCalledWith('C')

        // Reload forces a refetch of every visible project (including C).
        await wrapper.find('button[title="Refresh insights"]').trigger('click')
        await flushPromises()
        projectC2.resolve(stats(12))
        await flushPromises()
        expect(overviewRow('Alpha')).toContain('5')
        expect(overviewRow('Gamma')).toContain('12')

        // The older fill response arrives late: the fresh value must survive.
        projectC1.resolve(stats(2))
        await flushPromises()
        expect(overviewRow('Gamma')).toContain('12')
        expect(overviewRow('Alpha')).toContain('5')
    })

    it('window switches fetch the new scope; an older window completing late cannot publish', async () => {
        listTasksMock.mockResolvedValue({ total: 1, tasks: [makeTask('A-1')] })
        activityFeedMock.mockResolvedValue([feedChange('A-1', 2)])
        await mountInsights()
        await nextTickP()
        expect(currentSeriesTotal()).toBe(2)

        const window60 = deferred<any[]>()
        const window90 = deferred<any[]>()
        activityFeedMock.mockImplementation(() => {
            const callIndex = activityFeedMock.mock.calls.length
            if (callIndex === 2) return window60.promise
            if (callIndex === 3) return window90.promise
            return Promise.resolve([feedChange('A-1', 2)])
        })

        const firstSince = new Date((activityFeedMock.mock.calls[0]![0] as any).since as string).getTime()

        await wrapper.findAll('.chart-controls button').find(b => b.text() === '60d')!.trigger('click')
        await flushPromises()
        await wrapper.findAll('.chart-controls button').find(b => b.text() === '90d')!.trigger('click')
        await flushPromises()

        const secondSince = new Date((activityFeedMock.mock.calls[1]![0] as any).since as string).getTime()
        expect(secondSince).toBeLessThan(firstSince)

        window90.resolve([feedChange('A-1', 1)])
        await flushPromises()
        await nextTickP()
        expect(currentSeriesTotal()).toBe(1)

        window60.resolve([feedChange('A-1', 5)])
        await flushPromises()
        await nextTickP()
        expect(currentSeriesTotal()).toBe(1)
    })

    it('surfaces feed failures in the chart area without dropping task data', async () => {
        listTasksMock.mockResolvedValue({ total: 2, tasks: [makeTask('A-1'), makeTask('A-2')] })
        await mountInsights()

        activityFeedMock.mockRejectedValueOnce(new Error('feed unavailable'))
        await wrapper.findAll('.chart-controls button').find(b => b.text() === '14d')!.trigger('click')
        await flushPromises()

        expect(wrapper.text()).toContain('feed unavailable')
        expect(tile('Total tasks')).toBe('2')
    })

    it('stops all orchestration on unmount; late responses issue nothing further', async () => {
        listTasksMock.mockResolvedValue({ total: 1, tasks: [makeTask('A-1')] })
        await mountInsights()

        const projectA = deferred<{ total: number; tasks: TaskDTO[] }>()
        listTasksMock.mockImplementation((filter: any) =>
            filter?.project === 'A' ? projectA.promise : Promise.resolve({ total: 0, tasks: [] }))

        await wrapper.find('select.ui-select').setValue('A')
        await flushPromises()

        const statsCalls = projectStatsMock.mock.calls.length
        const feedCalls = activityFeedMock.mock.calls.length

        wrapper.unmount()
        wrapper = undefined as any

        projectA.resolve({ total: 7, tasks: Array.from({ length: 7 }, (_, i) => makeTask(`A-${i}`)) })
        await flushPromises()

        expect(projectStatsMock.mock.calls.length).toBe(statsCalls)
        expect(activityFeedMock.mock.calls.length).toBe(feedCalls)
        expect(showToast).not.toHaveBeenCalled()
        const { useTaskStore } = await import('../composables/useTaskStore')
        expect(useTaskStore().getQuery({ project: 'A' }).ids.value).toEqual([])
    })

    it('overdue tile excludes terminal tasks and due-today; tile and breakdown agree', async () => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        try {
            const tasks = [
                makeTask('A-1', { status: 'Done', due_date: '2025-12-01' }), // terminal past due: not overdue
                makeTask('A-2', { status: 'Todo', due_date: '2025-12-01' }), // overdue
                makeTask('A-3', { status: 'Todo', due_date: '2026-01-05' }), // due today, not overdue
                makeTask('A-4', { status: 'Done', due_date: '2026-01-05' }), // terminal due today: still due today
                makeTask('A-5', { status: 'Todo' }),                          // no due date
            ]
            listTasksMock.mockResolvedValue({ total: tasks.length, tasks })

            await mountInsights()

            expect(tile('Overdue')).toBe('1')
            const rows = wrapper.findAll('table.distribution tbody tr')
            const dueRow = (label: string) => rows.find(r => r.text().includes(label))
            expect(dueRow('Overdue')?.text()).toContain('1')
            expect(dueRow('Due today')?.text()).toContain('2')

            // F4: the terminal past-due class is VISIBLE (no silent percent
            // leak) and explains the whole denominator...
            const finished = dueRow('Finished (past due)')
            expect(finished).toBeTruthy()
            expect(finished!.text()).toContain('1')
            expect(finished!.text()).toContain('20%')
            // ...percentages now account for every task (1+1+2+1 of 5 = 100%).
            const percentOf = (label: string) => {
                const cells = dueRow(label)!.findAll('td')
                return Number(cells[cells.length - 1]!.text().replace('%', ''))
            }
            const percents = ['Overdue', 'Finished (past due)', 'Due today', 'No due date'].map(percentOf)
            expect(percents).toEqual([20, 20, 40, 20])
            expect(percents.reduce((a, b) => a + b, 0)).toBe(100)
            // ...but the row is NOT clickable: no invented due-filter value.
            expect(finished!.classes()).not.toContain('clickable')
            routerPush.mockClear()
            await finished!.trigger('click')
            expect(routerPush).not.toHaveBeenCalled()
            // The Overdue row itself remains the authoritative drill-down.
            expect(dueRow('Overdue')!.classes()).toContain('clickable')
        } finally {
            vi.useRealTimers()
        }
    })

    it('drills the overdue row into the TasksList due=overdue filter', async () => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        try {
            const tasks = [makeTask('A-2', { status: 'Todo', due_date: '2025-12-01' })]
            listTasksMock.mockResolvedValue({ total: tasks.length, tasks })
            await mountInsights()

            const rows = wrapper.findAll('table.distribution tbody tr')
            const overdueRow = rows.find(r => r.text().includes('Overdue'))
            expect(overdueRow).toBeTruthy()
            await overdueRow!.trigger('click')

            expect(routerPush).toHaveBeenCalledWith(expect.objectContaining({
                path: '/',
                query: expect.objectContaining({ due: 'overdue' }),
            }))
        } finally {
            vi.useRealTimers()
        }
    })

    it('uses each project policy in all-projects scope (custom terminal, not a Done guess)', async () => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        try {
            // B's policy names only Closed; a Done task stays overdue there.
            const tasks = [
                makeTask('A-1', { status: 'Done', due_date: '2025-12-01' }),
                makeTask('B-1', { status: 'Done', due_date: '2025-12-01' }),
                makeTask('B-2', { status: 'Closed', due_date: '2025-12-01' }),
            ]
            listTasksMock.mockResolvedValue({ total: tasks.length, tasks })

            await mountInsights()

            expect(showConfigMock).toHaveBeenCalledWith('A')
            expect(showConfigMock).toHaveBeenCalledWith('B')
            expect(tile('Overdue')).toBe('1')
        } finally {
            vi.useRealTimers()
        }
    })
})

function nextTickP(): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, 0))
}
