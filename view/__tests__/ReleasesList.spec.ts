import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ReleaseListItem, TaskDTO } from '../api/types'

enableAutoUnmount(afterEach)

const events = vi.hoisted(() => ({ handlers: new Map<string, () => void>() }))
vi.mock('../composables/useSse', () => ({
    useSse: () => ({
        on: (kind: string, handler: () => void) => events.handlers.set(kind, handler),
        off: vi.fn(),
        close: vi.fn(),
    }),
}))

vi.mock('../api/client', () => ({
    api: {
        releaseList: vi.fn(),
        releaseGet: vi.fn(),
        releaseBacklog: vi.fn(),
        releaseCreate: vi.fn(),
        releaseUpdate: vi.fn(),
        releaseMarkReleased: vi.fn(),
        releaseReopen: vi.fn(),
        releaseDelete: vi.fn(),
        releaseAdd: vi.fn(),
        releaseMove: vi.fn(),
        releaseRemove: vi.fn(),
    },
}))

vi.mock('../components/toast', () => ({ showToast: vi.fn() }))
vi.mock('../components/IconGlyph.vue', () => ({ default: { template: '<span class="icon" />' } }))
vi.mock('../components/ReloadButton.vue', () => ({
    default: { emits: ['click'], template: '<button type="button" class="reload" @click="$emit(\'click\')" />' },
}))

const router = vi.hoisted(() => ({ query: {} as Record<string, string>, replace: vi.fn() }))
vi.mock('vue-router', async () => {
    const { reactive } = await import('vue')
    const route = reactive({ query: router.query })
    router.replace.mockImplementation(async ({ query }: { query: Record<string, string> }) => {
        route.query = query
    })
    return { useRoute: () => route, useRouter: () => ({ replace: router.replace }) }
})

vi.mock('../composables/useProjects', async () => {
    const { ref } = await import('vue')
    const projects = ref([
        { name: 'Alpha', prefix: 'A' },
        { name: 'Beta', prefix: 'B' },
    ])
    return { useProjects: () => ({ projects, status: ref('ready'), refresh: vi.fn() }) }
})

const openTaskPanel = vi.fn()
vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({ state: { open: false }, openTaskPanel }),
}))

import { api } from '../api/client'
import ReleasesList from '../pages/ReleasesList.vue'

function release(id: number, version: string, overrides: Partial<ReleaseListItem> = {}): ReleaseListItem {
    return {
        id,
        version,
        display_name: version,
        state: 'planned',
        overdue: false,
        task_count: 0,
        done_count: 0,
        open_count: 0,
        ...overrides,
    }
}

function task(id: string, releases: number[] = [], done = false): TaskDTO {
    return {
        task_state: { is_done: done, due_bucket: null, calendar_day: '2026-01-01' },
        id,
        title: `Task ${id}`,
        status: 'Todo',
        priority: 'Medium',
        task_type: 'Feature',
        created: '2026-01-01T00:00:00Z',
        modified: '2026-01-01T00:00:00Z',
        tags: [],
        relationships: {},
        comments: [],
        references: [],
        sprints: [],
        releases,
        history: [],
        custom_fields: {},
    }
}

// Server order is version order: 0.9.0 before 0.10.0 although its id is higher.
const RELEASES = [
    release(3, '0.9.0', { state: 'released', released_at: '2026-10-01T12:00:00Z', task_count: 1, done_count: 1 }),
    release(1, '0.10.0', {
        label: 'Autumn',
        display_name: '0.10.0 (Autumn)',
        planned_date: '2026-11-01',
        overdue: true,
        task_count: 4,
        done_count: 1,
        open_count: 3,
    }),
    release(2, '0.11.0'),
]

const TASKS: Record<number, TaskDTO[]> = {
    3: [task('A-1', [3])],
    1: [task('A-2', [1]), task('A-3', [1]), task('B-1', [1], true), task('B-2', [1])],
    2: [],
}

function assignment(action: 'add' | 'move' | 'remove', id: number, modified: string[]) {
    return { status: 'ok', action, release_id: id, release_version: '0.11.0', modified, unchanged: [] }
}

async function mountPage() {
    const wrapper = mount(ReleasesList, { attachTo: document.body })
    await flushPromises()
    return wrapper
}

function groupOf(wrapper: Awaited<ReturnType<typeof mountPage>>, id: number | 'backlog') {
    return wrapper.get(`[data-release-id="${id}"]`)
}

function dataTransfer(): DataTransfer {
    const data = new Map<string, string>()
    return {
        setData: (type: string, value: string) => data.set(type, value),
        getData: (type: string) => data.get(type) ?? '',
        dropEffect: 'none',
        effectAllowed: 'all',
    } as unknown as DataTransfer
}

async function drag(
    wrapper: Awaited<ReturnType<typeof mountPage>>,
    taskId: string,
    from: number | 'backlog',
    to: number | 'backlog',
) {
    const transfer = dataTransfer()
    await groupOf(wrapper, from).get(`tr[data-task-id="${taskId}"]`).trigger('dragstart', { dataTransfer: transfer })
    await groupOf(wrapper, to).trigger('dragover', { dataTransfer: transfer })
    await groupOf(wrapper, to).trigger('drop', { dataTransfer: transfer })
    await flushPromises()
}

describe('ReleasesList', () => {
    beforeEach(() => {
        events.handlers.clear()
        openTaskPanel.mockReset()
        router.replace.mockClear()
        vi.mocked(api.releaseList).mockReset().mockResolvedValue({ status: 'ok', total: 3, releases: RELEASES })
        vi.mocked(api.releaseGet)
            .mockReset()
            .mockImplementation(async (id) => ({
                status: 'ok',
                release: RELEASES.find((entry) => entry.id === id)!,
                tasks: TASKS[Number(id)] ?? [],
                history: [],
            }))
        vi.mocked(api.releaseBacklog)
            .mockReset()
            .mockResolvedValue({ status: 'ok', total: 1, count: 1, limit: 50, offset: 0, tasks: [task('A-9')] })
        for (const method of ['releaseAdd', 'releaseMove', 'releaseRemove'] as const) vi.mocked(api[method]).mockReset()
        vi.mocked(api.releaseMarkReleased).mockReset()
        vi.mocked(api.releaseCreate).mockReset()
    })

    it('lists releases in the server version order with states, dates, counts and tickets', async () => {
        const wrapper = await mountPage()
        const titles = wrapper.findAll('[data-testid="release-group"] h2').map((node) => node.text())
        expect(titles).toEqual(['0.9.0', '0.10.0 (Autumn)', '0.11.0'])

        const autumn = groupOf(wrapper, 1)
        expect(autumn.get('[data-testid="release-state"]').text()).toBe('planned')
        expect(autumn.find('[data-testid="release-overdue"]').exists()).toBe(true)
        expect(autumn.text()).toContain('Planned')
        expect(autumn.get('[data-testid="release-counts"]').text()).toBe('3 open · 1 done')
        expect(autumn.get('[role="progressbar"]').attributes('aria-valuenow')).toBe('1')
        expect(autumn.findAll('tbody tr').map((row) => row.attributes('data-task-id'))).toEqual([
            'A-2',
            'A-3',
            'B-1',
            'B-2',
        ])

        const shipped = groupOf(wrapper, 3)
        expect(shipped.get('[data-testid="release-state"]').text()).toBe('released')
        expect(shipped.text()).toContain('Released')
        expect(shipped.find('[data-testid="release-reopen"]').exists()).toBe(true)
        // Released releases start collapsed.
        expect(shipped.get('button.collapse-btn').attributes('aria-expanded')).toBe('false')

        expect(groupOf(wrapper, 'backlog').findAll('tbody tr')).toHaveLength(1)
        expect(api.releaseBacklog).toHaveBeenCalledWith(expect.objectContaining({ offset: 0 }))
    })

    it('shows an empty state without releases', async () => {
        vi.mocked(api.releaseList).mockResolvedValue({ status: 'ok', total: 0, releases: [] })
        vi.mocked(api.releaseBacklog).mockResolvedValue({
            status: 'ok',
            total: 0,
            count: 0,
            limit: 50,
            offset: 0,
            tasks: [],
        })
        const wrapper = await mountPage()
        expect(wrapper.text()).toContain('No releases yet')
    })

    it('adds from the backlog, moves between releases and removes to the backlog by drag and drop', async () => {
        vi.mocked(api.releaseAdd).mockResolvedValue(assignment('add', 2, ['A-9']))
        vi.mocked(api.releaseMove).mockResolvedValue(assignment('move', 2, ['A-2']))
        vi.mocked(api.releaseRemove).mockResolvedValue(assignment('remove', 1, ['A-3']))
        const wrapper = await mountPage()

        await drag(wrapper, 'A-9', 'backlog', 2)
        expect(api.releaseAdd).toHaveBeenCalledWith({ release: 2, tasks: ['A-9'] })

        await drag(wrapper, 'A-2', 1, 2)
        expect(api.releaseMove).toHaveBeenCalledWith({ release: 2, tasks: ['A-2'] })

        await drag(wrapper, 'A-3', 1, 'backlog')
        expect(api.releaseRemove).toHaveBeenCalledWith({ release: 1, tasks: ['A-3'] })

        // A drop on the release a ticket came from does nothing.
        await drag(wrapper, 'A-2', 1, 1)
        expect(api.releaseMove).toHaveBeenCalledTimes(1)
        // Each change reloads the page data.
        expect(vi.mocked(api.releaseList).mock.calls.length).toBeGreaterThanOrEqual(4)
    })

    it('adds, moves and removes selected tickets in bulk', async () => {
        vi.mocked(api.releaseAdd).mockResolvedValue(assignment('add', 2, ['A-2', 'A-9']))
        vi.mocked(api.releaseRemove).mockResolvedValue(assignment('remove', 1, ['A-2']))
        vi.mocked(api.releaseMove).mockResolvedValue(assignment('move', 2, ['A-3']))
        const wrapper = await mountPage()

        await groupOf(wrapper, 1).get('input[aria-label="Select A-2"]').setValue(true)
        await groupOf(wrapper, 'backlog').get('input[aria-label="Select A-9"]').setValue(true)
        expect(wrapper.get('.releases-bulk').text()).toContain('2 selected')
        // The first planned release is the default target.
        expect((wrapper.get('[data-testid="release-bulk-target"]').element as HTMLSelectElement).value).toBe('1')
        await wrapper.get('[data-testid="release-bulk-target"]').setValue('2')
        await wrapper.get('[data-testid="release-bulk-add"]').trigger('click')
        await flushPromises()
        expect(api.releaseAdd).toHaveBeenCalledWith({ release: 2, tasks: ['A-2', 'A-9'] })
        expect(wrapper.find('.releases-bulk').exists()).toBe(false)

        await groupOf(wrapper, 1).get('input[aria-label="Select A-2"]').setValue(true)
        await wrapper.get('[data-testid="release-bulk-remove"]').trigger('click')
        await flushPromises()
        expect(api.releaseRemove).toHaveBeenCalledWith({ release: 1, tasks: ['A-2'] })

        await groupOf(wrapper, 1).get('input[aria-label="Select A-3"]').setValue(true)
        await wrapper.get('[data-testid="release-bulk-target"]').setValue('2')
        await wrapper.get('[data-testid="release-bulk-move"]').trigger('click')
        await flushPromises()
        expect(api.releaseMove).toHaveBeenCalledWith({ release: 2, tasks: ['A-3'] })
    })

    it('marks a release released and creates a release', async () => {
        vi.mocked(api.releaseMarkReleased).mockResolvedValue({ status: 'ok', release: RELEASES[1]!, changed: true })
        vi.mocked(api.releaseCreate).mockResolvedValue({ status: 'ok', release: release(4, '0.12.0'), changed: true })
        const wrapper = await mountPage()

        await groupOf(wrapper, 1).get('[data-testid="release-mark-released"]').trigger('click')
        await flushPromises()
        expect(api.releaseMarkReleased).toHaveBeenCalledWith({ release: 1 })

        await wrapper.get('[data-testid="release-create"]').trigger('click')
        await flushPromises()
        const form = document.querySelector<HTMLFormElement>('[data-testid="release-form"]')!
        const version = form.querySelector<HTMLInputElement>('[data-testid="release-form-version"]')!
        version.value = 'v0.12.0'
        version.dispatchEvent(new Event('input'))
        const planned = form.querySelector<HTMLInputElement>('[data-testid="release-form-planned"]')!
        planned.value = '2026-12-01'
        planned.dispatchEvent(new Event('input'))
        form.dispatchEvent(new Event('submit'))
        await flushPromises()
        expect(api.releaseCreate).toHaveBeenCalledWith({ version: 'v0.12.0', planned_date: '2026-12-01' })
    })

    it('narrows the tickets and counts to the project picked (DEV-464)', async () => {
        const wrapper = await mountPage()
        await wrapper.get('[data-testid="release-project"]').setValue('B')
        await flushPromises()
        expect(router.replace).toHaveBeenCalledWith({ query: { project: 'B' } })
        expect(wrapper.get('h1, .app-page-header').text()).toContain('Beta')

        const autumn = groupOf(wrapper, 1)
        expect(autumn.findAll('tbody tr').map((row) => row.attributes('data-task-id'))).toEqual(['B-1', 'B-2'])
        expect(autumn.get('[data-testid="release-counts"]').text()).toBe('1 open · 1 done')
        expect(autumn.get('[role="progressbar"]').attributes('aria-valuemax')).toBe('2')
        expect(groupOf(wrapper, 3).text()).toContain('No Beta tickets in this release.')
        expect(api.releaseBacklog).toHaveBeenLastCalledWith(expect.objectContaining({ project: 'B', offset: 0 }))

        await wrapper.get('[data-testid="release-project"]').setValue('')
        await flushPromises()
        expect(groupOf(wrapper, 1).findAll('tbody tr')).toHaveLength(4)
        expect(vi.mocked(api.releaseBacklog).mock.lastCall?.[0]).not.toHaveProperty('project')
    })

    it('reloads on a release_changed event', async () => {
        vi.useFakeTimers()
        try {
            await mountPage()
            const before = vi.mocked(api.releaseList).mock.calls.length
            events.handlers.get('release_changed')?.()
            await vi.advanceTimersByTimeAsync(200)
            expect(vi.mocked(api.releaseList).mock.calls.length).toBe(before + 1)
        } finally {
            vi.useRealTimers()
        }
    })
})
