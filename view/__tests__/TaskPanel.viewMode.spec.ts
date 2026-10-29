import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import TaskPanel from '../components/TaskPanel.vue'
import { invalidateCompletionPolicies } from '../composables/useCompletionPolicy'
import { readTaskViewPreference, storeTaskViewPreference, TASK_VIEW_EXISTING_KEY } from '../utils/preferences'

const api = vi.hoisted(() => ({
    getTask: vi.fn(),
    showConfig: vi.fn(),
    updateTask: vi.fn(),
    setStatus: vi.fn(),
    addTask: vi.fn(),
    listProjects: vi.fn(),
    listTasks: vi.fn(),
    sprintList: vi.fn(),
    taskHistory: vi.fn(),
    inspectConfig: vi.fn(),
    suggestTasks: vi.fn(),
    whoami: vi.fn(),
}))
const toast = vi.hoisted(() => vi.fn())
vi.mock('../api/client', () => ({ api }))
vi.mock('../components/toast', () => ({ showToast: toast }))

function task(id: string, status: string, taskState?: Record<string, unknown>) {
    return {
        id,
        title: id,
        status,
        priority: 'Medium',
        task_type: 'Task',
        reporter: '',
        assignee: '',
        description: '',
        tags: [] as string[],
        sprints: [],
        relationships: {},
        comments: [],
        references: [],
        history: [],
        custom_fields: {},
        ...(taskState ? { task_state: taskState } : {}),
    }
}

function config(project: string) {
    return {
        issue_states: ['Todo', 'Doing', 'Done', 'Shipped'],
        issue_priorities: ['Medium', 'High'],
        issue_types: ['Task'],
        effective_done_states: ['Shipped'],
        done_states_mode: 'explicit',
        task_calendar_day: '2026-10-05',
        default_project: project,
        default_status: 'Todo',
        default_priority: 'Medium',
        default_reporter: '',
        default_assignee: '',
        default_tags: [] as string[],
        tags: ['*'],
        custom_fields: [],
    }
}

let wrappers: VueWrapper[] = []

async function mountPanel(props: Record<string, unknown>) {
    const wrapper = mount(TaskPanel, {
        props: { open: true, initialProject: 'A', ...props },
        global: { stubs: { Teleport: true } },
        attachTo: document.body,
    })
    wrappers.push(wrapper)
    await flushPromises()
    await flushPromises()
    return wrapper
}

const isLarge = (wrapper: VueWrapper) => wrapper.find('aside.task-panel').classes().includes('task-panel--large')

describe('TaskPanel side panel and large view (DEV-239)', () => {
    beforeEach(() => {
        vi.resetAllMocks()
        localStorage.clear()
        invalidateCompletionPolicies()
        api.getTask.mockImplementation(async (id: string) => task(id, 'Todo'))
        api.listProjects.mockResolvedValue({ projects: [{ prefix: 'A' }], total: 1 })
        api.listTasks.mockResolvedValue({ tasks: [], total: 0 })
        api.sprintList.mockResolvedValue({ sprints: [], missing_sprints: [] })
        api.taskHistory.mockResolvedValue([])
        api.inspectConfig.mockResolvedValue({ effective: { remotes: {} } })
        api.suggestTasks.mockResolvedValue([])
        api.whoami.mockResolvedValue('')
        api.showConfig.mockImplementation(async (project?: string) => config(project ?? ''))
    })

    afterEach(() => {
        wrappers.forEach((wrapper) => wrapper.unmount())
        wrappers = []
        document.body.innerHTML = ''
    })

    it('defaults to the side panel for both kinds of task', () => {
        expect(readTaskViewPreference('existing')).toBe('panel')
        expect(readTaskViewPreference('new')).toBe('panel')
        localStorage.setItem(TASK_VIEW_EXISTING_KEY, 'huge')
        expect(readTaskViewPreference('existing')).toBe('panel')
    })

    it('switches views with the header button and keeps the choice while moving between tasks', async () => {
        const wrapper = await mountPanel({ taskId: 'A-1' })
        const toggle = wrapper.get('[data-testid="task-view-toggle"]')
        expect(isLarge(wrapper)).toBe(false)
        expect(toggle.attributes('aria-pressed')).toBe('false')
        expect(toggle.attributes('aria-label')).toBe('Switch to large view')

        await toggle.trigger('click')
        expect(isLarge(wrapper)).toBe(true)
        expect(wrapper.get('[data-testid="task-view-toggle"]').attributes('aria-pressed')).toBe('true')
        expect(wrapper.get('[data-testid="task-view-toggle"]').attributes('aria-label')).toBe('Switch to side panel')
        expect(readTaskViewPreference('existing')).toBe('panel')

        await wrapper.setProps({ taskId: 'A-2' })
        await flushPromises()
        expect(isLarge(wrapper)).toBe(true)
    })

    it('opens existing and new tasks in their preferred views each time', async () => {
        storeTaskViewPreference('existing', 'large')
        const wrapper = await mountPanel({ taskId: 'A-1' })
        expect(isLarge(wrapper)).toBe(true)

        await wrapper.get('[data-testid="task-view-toggle"]').trigger('click')
        expect(isLarge(wrapper)).toBe(false)
        await wrapper.setProps({ open: false })
        await wrapper.setProps({ open: true })
        await flushPromises()
        expect(isLarge(wrapper)).toBe(true)

        const created = await mountPanel({ taskId: 'new' })
        expect(isLarge(created)).toBe(false)
        storeTaskViewPreference('new', 'large')
        const createdLarge = await mountPanel({ taskId: 'new' })
        expect(isLarge(createdLarge)).toBe(true)
    })

    it('keeps people, sprints and custom fields together for the large view column', async () => {
        const wrapper = await mountPanel({ taskId: 'A-1' })
        const side = wrapper.get('.task-panel__side')
        expect(side.text()).toContain('Ownership')
        expect(wrapper.findAll('.task-panel__form-fieldset > .task-panel__main').length).toBeGreaterThanOrEqual(3)
    })
})
