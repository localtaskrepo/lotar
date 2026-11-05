import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// DEV-301: saves carry the snapshot's `modified`; a save refused because the
// task changed elsewhere is merged and resent without overwriting the outside
// change, save responses keep text typed in other fields, and responses from
// other endpoints (archive) bring outside changes into the form.

const api = vi.hoisted(() => ({
    getTask: vi.fn(),
    showConfig: vi.fn(),
    updateTask: vi.fn(),
    addTask: vi.fn(),
    listProjects: vi.fn(),
    listTasks: vi.fn(),
    sprintList: vi.fn(),
    taskHistory: vi.fn(),
    inspectConfig: vi.fn(),
    suggestTasks: vi.fn(),
    whoami: vi.fn(),
    archiveTask: vi.fn(),
}))
vi.mock('../api/client', () => ({ api }))
vi.mock('../components/toast', () => ({ showToast: vi.fn() }))

import TaskPanel from '../components/TaskPanel.vue'
import { _resetTaskStore } from '../composables/useTaskStore'

const LOADED = '2026-10-01T10:00:00Z'
const LATER = '2026-10-01T10:05:00Z'

function task(extra: Record<string, unknown> = {}) {
    return {
        id: 'A-1',
        title: 'Original title',
        status: 'Open',
        priority: 'Medium',
        task_type: 'Task',
        reporter: '',
        assignee: '',
        description: 'Original text',
        tags: [],
        sprints: [],
        relationships: {},
        comments: [],
        references: [],
        history: [],
        custom_fields: {},
        modified: LOADED,
        ...extra,
    }
}

let server: Record<string, any> = {}
let saves = 0
let wrappers: VueWrapper[] = []

async function openPanel() {
    const wrapper = mount(TaskPanel, {
        props: { open: true, taskId: 'A-1', initialProject: 'A' },
        global: { stubs: { Teleport: true } },
        attachTo: document.body,
    })
    wrappers.push(wrapper)
    await flushPromises()
    api.updateTask.mockClear()
    return wrapper
}

beforeEach(() => {
    vi.resetAllMocks()
    _resetTaskStore()
    localStorage.clear()
    server = task()
    saves = 0
    api.listProjects.mockResolvedValue({ projects: [{ prefix: 'A' }], total: 1 })
    api.listTasks.mockResolvedValue({ tasks: [], total: 0 })
    api.sprintList.mockResolvedValue({ sprints: [], missing_sprints: [] })
    api.taskHistory.mockResolvedValue([])
    api.inspectConfig.mockResolvedValue({ effective: { remotes: {} } })
    api.suggestTasks.mockResolvedValue([])
    api.whoami.mockResolvedValue('')
    api.showConfig.mockResolvedValue({
        issue_states: ['Open', 'Done'],
        issue_priorities: ['Medium', 'High'],
        issue_types: ['Task'],
        default_project: 'A',
        tags: ['*'],
        custom_fields: ['*'],
    })
    api.getTask.mockImplementation(async () => JSON.parse(JSON.stringify(server)))
    // The server's precondition: a stale expected_modified is a 409.
    api.updateTask.mockImplementation(async (_id: string, patch: any) => {
        const { expected_modified: expected, ...fields } = JSON.parse(JSON.stringify(patch))
        if (expected && expected !== server.modified) {
            const error = new Error('POST /api/tasks/update failed: changed since it was loaded') as Error & {
                status?: number
            }
            error.status = 409
            throw error
        }
        saves += 1
        server = { ...server, ...fields, modified: `2026-10-01T11:0${saves}:00Z` }
        return JSON.parse(JSON.stringify(server))
    })
})

afterEach(() => {
    wrappers.forEach((wrapper) => wrapper.unmount())
    wrappers = []
    _resetTaskStore()
    document.body.innerHTML = ''
})

/** A change written elsewhere that the panel has not heard about yet. */
function silentOutsideEdit(patch: Record<string, unknown>) {
    server = { ...server, ...patch, modified: LATER }
}

describe('TaskPanel stale saves (DEV-301)', () => {
    it('resends a field that did not clash and takes the outside change', async () => {
        const wrapper = await openPanel()
        const vm = wrapper.vm as any
        silentOutsideEdit({ description: 'Agent text' })

        vm.form.title = 'My title'
        await vm.onFieldBlur('title')
        await flushPromises()

        expect(api.updateTask).toHaveBeenCalledTimes(2)
        expect(api.updateTask.mock.calls[0]![1]).toEqual({ title: 'My title', expected_modified: LOADED })
        expect(api.updateTask.mock.calls[1]![1]).toEqual({ title: 'My title', expected_modified: LATER })
        expect(server.title).toBe('My title')
        expect(server.description).toBe('Agent text')
        expect(vm.form.description).toBe('Agent text')
        expect(wrapper.find('[data-testid="task-panel-conflict"]').exists()).toBe(false)
    })

    it('holds an edit to a field that also changed elsewhere', async () => {
        const wrapper = await openPanel()
        const vm = wrapper.vm as any
        silentOutsideEdit({ description: 'Agent text' })

        vm.form.description = 'My text'
        await vm.onFieldBlur('description')
        await flushPromises()

        expect(api.updateTask).toHaveBeenCalledTimes(1)
        expect(server.description).toBe('Agent text')
        expect(vm.form.description).toBe('My text')
        expect(wrapper.find('[data-testid="task-panel-conflict"]').exists()).toBe(true)
    })

    it('keeps text typed in another field when a save response arrives', async () => {
        const wrapper = await openPanel()
        const vm = wrapper.vm as any

        vm.form.description = 'Saved text'
        vm.form.title = 'Still typing'
        await vm.onFieldBlur('description')
        await flushPromises()

        expect(server.description).toBe('Saved text')
        expect(server.title).toBe('Original title')
        expect(vm.form.description).toBe('Saved text')
        expect(vm.form.title).toBe('Still typing')
    })

    it('brings outside changes in an archive response into the form', async () => {
        const wrapper = await openPanel()
        const vm = wrapper.vm as any
        api.archiveTask.mockImplementation(async () => {
            server = { ...server, description: 'Agent text', archived: true, modified: LATER }
            return JSON.parse(JSON.stringify(server))
        })

        await vm.setTaskArchived(true)
        await flushPromises()

        expect(vm.form.description).toBe('Agent text')
    })
})
