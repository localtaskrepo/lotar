import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

// DEV-263: an open panel follows outside edits that reach the shared store,
// and a field the user is editing that changed elsewhere is never saved over
// the newer value without asking.

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
}))
vi.mock('../api/client', () => ({ api }))
vi.mock('../components/toast', () => ({ showToast: vi.fn() }))

import TaskPanel from '../components/TaskPanel.vue'
import { _resetTaskStore, useTaskStore } from '../composables/useTaskStore'

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

/** An outside edit (CLI, agent, other tab) arriving through the store. */
async function outsideEdit(patch: Record<string, unknown>, modified = LATER) {
    server = { ...server, ...patch, modified }
    useTaskStore().upsert(JSON.parse(JSON.stringify(server)))
    await nextTick()
    await flushPromises()
}

beforeEach(() => {
    vi.resetAllMocks()
    _resetTaskStore()
    localStorage.clear()
    server = task()
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
    api.updateTask.mockImplementation(async (_id: string, patch: any) => {
        server = { ...server, ...JSON.parse(JSON.stringify(patch)), modified: '2026-10-01T10:10:00Z' }
        return JSON.parse(JSON.stringify(server))
    })
})

afterEach(() => {
    wrappers.forEach((wrapper) => wrapper.unmount())
    wrappers = []
    _resetTaskStore()
    document.body.innerHTML = ''
})

describe('TaskPanel outside edits (DEV-263)', () => {
    it('shows a newer outside description, so the next edit builds on it', async () => {
        const wrapper = await openPanel()
        const vm = wrapper.vm as any

        await outsideEdit({ description: 'Original text. Acceptance criteria added by agent.' })
        expect(vm.form.description).toBe('Original text. Acceptance criteria added by agent.')
        expect(wrapper.find('[data-testid="task-panel-conflict"]').exists()).toBe(false)

        vm.form.description = `${vm.form.description} User note.`
        await vm.onFieldBlur('description')
        await flushPromises()
        expect(api.updateTask).toHaveBeenCalledTimes(1)
        expect(api.updateTask.mock.calls[0]![1]).toEqual({
            description: 'Original text. Acceptance criteria added by agent. User note.',
        })
    })

    it('refreshes untouched fields and keeps the field being edited', async () => {
        const wrapper = await openPanel()
        const vm = wrapper.vm as any
        vm.form.description = 'Typing my own text'

        await outsideEdit({ title: 'Renamed by CLI', status: 'Done' })
        expect(vm.form.title).toBe('Renamed by CLI')
        expect(vm.form.status).toBe('Done')
        expect(vm.form.description).toBe('Typing my own text')
        expect(wrapper.find('[data-testid="task-panel-conflict"]').exists()).toBe(false)
    })

    it('ignores store entities that are not newer than the panel', async () => {
        const wrapper = await openPanel()
        const vm = wrapper.vm as any
        await outsideEdit({ title: 'Older snapshot' }, '2026-10-01T09:00:00Z')
        expect(vm.form.title).toBe('Original title')
    })

    it('holds a conflicting edit back and keeps it when asked', async () => {
        const wrapper = await openPanel()
        const vm = wrapper.vm as any
        vm.form.description = 'Mine'

        await outsideEdit({ description: 'Theirs' })
        const banner = wrapper.find('[data-testid="task-panel-conflict"]')
        expect(banner.exists()).toBe(true)
        expect(banner.text()).toContain('Description')
        expect(vm.form.description).toBe('Mine')

        // Leaving the field does not overwrite the outside change.
        await vm.onFieldBlur('description')
        await flushPromises()
        expect(api.updateTask).not.toHaveBeenCalled()

        await wrapper.find('[data-testid="task-panel-conflict-keep"]').trigger('click')
        await flushPromises()
        expect(api.updateTask).toHaveBeenCalledTimes(1)
        expect(api.updateTask.mock.calls[0]![1]).toEqual({ description: 'Mine' })
        expect(wrapper.find('[data-testid="task-panel-conflict"]').exists()).toBe(false)
    })

    it('reloads the outside value when asked, without saving', async () => {
        const wrapper = await openPanel()
        const vm = wrapper.vm as any
        vm.form.title = 'My title'

        await outsideEdit({ title: 'Their title' })
        expect(wrapper.find('[data-testid="task-panel-conflict"]').text()).toContain('Title')

        await wrapper.find('[data-testid="task-panel-conflict-reload"]').trigger('click')
        await flushPromises()
        expect(vm.form.title).toBe('Their title')
        expect(wrapper.find('[data-testid="task-panel-conflict"]').exists()).toBe(false)
        await vm.handleFieldBlur('title')
        await flushPromises()
        expect(api.updateTask).not.toHaveBeenCalled()
    })

    it('does not flag a conflict when the user typed what arrived', async () => {
        const wrapper = await openPanel()
        const vm = wrapper.vm as any
        vm.form.title = 'Same title'
        await outsideEdit({ title: 'Same title' })
        expect(wrapper.find('[data-testid="task-panel-conflict"]').exists()).toBe(false)
        await vm.handleFieldBlur('title')
        await flushPromises()
        expect(api.updateTask).not.toHaveBeenCalled()
    })

    it('treats a changed custom field as a conflict and keeps other custom values in step', async () => {
        server = task({ custom_fields: { product: 'web', points: 3 } })
        const wrapper = await openPanel()
        const vm = wrapper.vm as any

        await outsideEdit({ custom_fields: { product: 'api', points: 3 } })
        expect(vm.customFields.product).toBe('api')

        vm.customFields.points = '5'
        await outsideEdit({ custom_fields: { product: 'api', points: 8 } }, '2026-10-01T10:06:00Z')
        expect(wrapper.find('[data-testid="task-panel-conflict"]').text()).toContain('Custom fields')
        expect(vm.customFields.points).toBe('5')
        await vm.commitCustomFields()
        await flushPromises()
        expect(api.updateTask).not.toHaveBeenCalled()

        await wrapper.find('[data-testid="task-panel-conflict-reload"]').trigger('click')
        await flushPromises()
        expect(vm.customFields.points).toBe('8')
        expect(api.updateTask).not.toHaveBeenCalled()
    })
})
