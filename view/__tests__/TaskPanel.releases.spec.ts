import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import TaskPanel from '../components/TaskPanel.vue'
import TaskPanelReleasesSection from '../components/task-panel/TaskPanelReleasesSection.vue'
import type { ReleaseListItem } from '../api/types'

// DEV-393: the task panel shows a ticket's releases as chips and sets them
// through the task update `releases` field (create: the create payload).
const api = vi.hoisted(() => ({
    getTask: vi.fn(),
    showConfig: vi.fn(),
    updateTask: vi.fn(),
    addTask: vi.fn(),
    listProjects: vi.fn(),
    listTasks: vi.fn(),
    sprintList: vi.fn(),
    releaseList: vi.fn(),
    taskHistory: vi.fn(),
    inspectConfig: vi.fn(),
    suggestTasks: vi.fn(),
    whoami: vi.fn(),
}))
vi.mock('../api/client', () => ({ api }))
vi.mock('../components/toast', () => ({ showToast: vi.fn() }))

function release(id: number, version: string, state: 'planned' | 'released' = 'planned'): ReleaseListItem {
    return {
        id,
        version,
        display_name: version,
        state,
        overdue: false,
        task_count: 0,
        done_count: 0,
        open_count: 0,
    }
}

const RELEASES = [release(3, '0.9.0', 'released'), release(1, '0.10.0'), release(2, '0.11.0')]

function task(extra: Record<string, unknown> = {}) {
    return {
        id: 'A-1',
        title: 'Ship it',
        status: 'Open',
        priority: 'Medium',
        task_type: 'Task',
        reporter: '',
        assignee: '',
        description: '',
        tags: [],
        sprints: [],
        releases: [1],
        relationships: {},
        comments: [],
        references: [],
        history: [],
        custom_fields: {},
        modified: '2026-11-01T00:00:00Z',
        ...extra,
    }
}

let current: Record<string, unknown> = {}
let wrappers: VueWrapper[] = []

function panel(taskId: string) {
    const wrapper = mount(TaskPanel, {
        props: { open: true, taskId, initialProject: 'A' },
        global: { stubs: { Teleport: true } },
        attachTo: document.body,
    })
    wrappers.push(wrapper)
    return wrapper
}

beforeEach(() => {
    vi.resetAllMocks()
    localStorage.clear()
    current = task()
    api.listProjects.mockResolvedValue({ projects: [{ prefix: 'A' }], total: 1 })
    api.listTasks.mockResolvedValue({ tasks: [], total: 0 })
    api.sprintList.mockResolvedValue({ sprints: [], missing_sprints: [] })
    api.releaseList.mockResolvedValue({ status: 'ok', total: RELEASES.length, releases: RELEASES })
    api.taskHistory.mockResolvedValue([])
    api.inspectConfig.mockResolvedValue({ effective: { remotes: {} } })
    api.suggestTasks.mockResolvedValue([])
    api.whoami.mockResolvedValue('')
    api.showConfig.mockResolvedValue({
        issue_states: ['Open', 'Done'],
        issue_priorities: ['Medium'],
        issue_types: ['Task'],
        default_project: 'A',
        default_status: 'Open',
        default_priority: 'Medium',
        tags: ['*'],
        custom_fields: [],
    })
    api.getTask.mockImplementation(async () => structuredClone(current))
    api.updateTask.mockImplementation(async (_id: string, patch: Record<string, unknown>) => {
        current = { ...structuredClone(current), ...structuredClone(patch) }
        return structuredClone(current)
    })
    api.addTask.mockImplementation(async (payload: Record<string, unknown>) => ({ ...task(), ...payload, id: 'A-2' }))
})

afterEach(() => {
    wrappers.forEach((wrapper) => wrapper.unmount())
    wrappers = []
    document.body.innerHTML = ''
})

describe('TaskPanelReleasesSection', () => {
    it('lists assigned releases in version order and offers the others', async () => {
        const wrapper = mount(TaskPanelReleasesSection, { props: { releases: [2, 3, 9], releaseList: RELEASES } })
        const chips = wrapper.findAll('.task-panel-releases__chip').map((chip) => chip.text())
        expect(chips).toEqual(['0.9.0', '0.11.0', '#9 (missing)'])
        const options = wrapper.findAll('option').map((option) => option.text())
        expect(options).toEqual(['Add to release…', '0.10.0'])

        await wrapper.get('select').setValue('1')
        await wrapper.get('button:not(.task-panel-releases__remove)').trigger('click')
        expect(wrapper.emitted('update:releases')?.[0]).toEqual([[2, 3, 9, 1]])

        await wrapper.get('button[aria-label="Remove from 0.9.0"]').trigger('click')
        expect(wrapper.emitted('update:releases')?.[1]).toEqual([[2, 9]])
    })
})

describe('TaskPanel releases field', () => {
    it('shows a saved ticket’s releases and saves changes through the task update', async () => {
        const wrapper = panel('A-1')
        await flushPromises()
        const section = wrapper.get('[data-testid="task-panel-releases"]')
        expect(section.findAll('.task-panel-releases__chip').map((chip) => chip.text())).toEqual(['0.10.0'])

        await section.get('select').setValue('2')
        await section.get('button:not(.task-panel-releases__remove)').trigger('click')
        await flushPromises()
        expect(api.updateTask).toHaveBeenCalledTimes(1)
        expect(api.updateTask.mock.calls[0]![1]).toMatchObject({ releases: [1, 2] })

        await wrapper.get('button[aria-label="Remove from 0.10.0"]').trigger('click')
        await flushPromises()
        expect(api.updateTask.mock.calls[1]![1]).toMatchObject({ releases: [2] })
        expect(
            wrapper.findAll('[data-testid="task-panel-releases"] .task-panel-releases__chip').map((c) => c.text()),
        ).toEqual(['0.11.0'])
    })

    it('sends a draft’s releases in the create request', async () => {
        const wrapper = panel('new')
        await flushPromises()
        const section = wrapper.get('[data-testid="task-panel-releases"]')
        await section.get('select').setValue('1')
        await section.get('button:not(.task-panel-releases__remove)').trigger('click')
        await flushPromises()
        expect(api.updateTask).not.toHaveBeenCalled()

        const vm = wrapper.vm as unknown as { form: { title: string }; handleSubmit: () => Promise<void> }
        vm.form.title = 'New ticket'
        await vm.handleSubmit()
        await flushPromises()
        expect(api.addTask.mock.calls[0]![0]).toMatchObject({ title: 'New ticket', releases: [1] })
    })
})
