import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import TaskPanel from '../components/TaskPanel.vue'
import { invalidateCompletionPolicies } from '../composables/useCompletionPolicy'

const api = vi.hoisted(() => ({
    getTask: vi.fn(), showConfig: vi.fn(), updateTask: vi.fn(), setStatus: vi.fn(), addTask: vi.fn(),
    listProjects: vi.fn(), listTasks: vi.fn(), sprintList: vi.fn(), taskHistory: vi.fn(), inspectConfig: vi.fn(),
    suggestTasks: vi.fn(), whoami: vi.fn(),
}))
const toast = vi.hoisted(() => vi.fn())
vi.mock('../api/client', () => ({ api }))
vi.mock('../components/toast', () => ({ showToast: toast }))

function task(id: string, status: string, taskState?: Record<string, unknown>) {
    return {
        id, title: id, status, priority: 'Medium', task_type: 'Task',
        reporter: '', assignee: '', description: '', tags: [] as string[], sprints: [],
        relationships: {}, comments: [], references: [], history: [], custom_fields: {},
        ...(taskState ? { task_state: taskState } : {}),
    }
}

function config(project: string) {
    // Explicit policy: only Shipped is terminal — Done is deliberately
    // excluded so the badge must NOT fall back to a 'done' substring guess.
    return {
        issue_states: ['Todo', 'Doing', 'Done', 'Shipped'],
        issue_priorities: ['Medium', 'High'],
        issue_types: ['Task'],
        effective_done_states: ['Shipped'],
        done_states_mode: 'explicit',
        task_calendar_day: '2026-10-05',
        default_project: project, default_status: 'Todo', default_priority: 'Medium',
        default_reporter: '', default_assignee: '', default_tags: [] as string[],
        tags: ['*'], custom_fields: [],
    }
}

let wrappers: VueWrapper[] = []

async function panel(taskId: string, status: string, taskState?: Record<string, unknown>) {
    api.getTask.mockImplementation(async () => task(taskId, status, taskState))
    const wrapper = mount(TaskPanel, {
        props: { open: true, taskId, initialProject: 'A' },
        global: { stubs: { Teleport: true } }, attachTo: document.body,
    })
    wrappers.push(wrapper)
    await flushPromises()
    await flushPromises()
    return wrapper
}

describe('TaskPanel status badge vs explicit done policy (DEV-21 F2)', () => {
    beforeEach(() => {
        vi.resetAllMocks()
        localStorage.clear()
        invalidateCompletionPolicies()
        api.listProjects.mockResolvedValue({ projects: [{ prefix: 'A' }], total: 1 })
        api.listTasks.mockResolvedValue({ tasks: [], total: 0 })
        api.sprintList.mockResolvedValue({ sprints: [], missing_sprints: [] })
        api.taskHistory.mockResolvedValue([])
        api.inspectConfig.mockResolvedValue({ effective: { remotes: {} } })
        api.suggestTasks.mockResolvedValue([])
        api.whoami.mockResolvedValue('')
        api.showConfig.mockImplementation(async (project?: string) => config(project ?? ''))
        api.updateTask.mockImplementation(async (id: string, patch: any) => ({ ...task(id, 'Todo'), ...patch }))
        api.setStatus.mockImplementation(async (id: string, status: string) => ({ ...task(id, status), status }))
    })

    afterEach(() => {
        wrappers.forEach(wrapper => wrapper.unmount())
        wrappers = []
        document.body.innerHTML = ''
    })

    it('does not badge Done as success when the loaded policy excludes it', async () => {
        const wrapper = await panel('A-1', 'Done')
        const vm = wrapper.vm as any

        // Edit mode resolved the config scope for A; the explicit policy
        // names only Shipped, so Done must NOT render success.
        expect(vm.form.project).toBe('A')
        expect(vm.loadedConfigScope ?? 'A').toBeTruthy()
        const badge = wrapper.find('[data-testid="task-panel-status-badge"]')
        expect(badge.exists()).toBe(true)
        expect(badge.text()).toBe('Done')
        expect(badge.classes()).not.toContain('badge--success')
        expect(badge.classes()).toContain('badge--muted')
    })

    it('badges the custom terminal status Shipped as success', async () => {
        const wrapper = await panel('A-2', 'Shipped')

        const badge = wrapper.find('[data-testid="task-panel-status-badge"]')
        expect(badge.exists()).toBe(true)
        expect(badge.text()).toBe('Shipped')
        expect(badge.classes()).toContain('badge--success')
    })

    it('keeps the badge neutral while no completion policy resolved for the scope', async () => {
        api.showConfig.mockResolvedValue({ issue_states: ['Todo', 'Done'] })
        invalidateCompletionPolicies()
        const wrapper = await panel('A-3', 'Done')

        const badge = wrapper.find('[data-testid="task-panel-status-badge"]')
        expect(badge.exists()).toBe(true)
        expect(badge.classes()).toContain('badge--muted')
    })

    it('drives the badge from the task\'s embedded actual-root done states', async () => {
        // The task carries its OWN policy ([Shipped]); the scoped showConfig
        // cache says [Shipped] too, but the embedded set is what decides —
        // and no green 'Done' guess once embedded states are provided.
        const embedded = { done_states: ['Shipped'], is_done: false, due_bucket: null, calendar_day: '2026-10-05' }

        const doneWrapper = await panel('A-4', 'Done', embedded)
        const doneBadge = doneWrapper.find('[data-testid="task-panel-status-badge"]')
        expect(doneBadge.exists()).toBe(true)
        expect(doneBadge.text()).toBe('Done')
        expect(doneBadge.classes()).not.toContain('badge--success')
        expect(doneBadge.classes()).toContain('badge--muted')

        const shippedWrapper = await panel('A-5', 'Shipped', embedded)
        const shippedBadge = shippedWrapper.find('[data-testid="task-panel-status-badge"]')
        expect(shippedBadge.classes()).toContain('badge--success')
    })

    it('embedded set beats a conflicting cached scope policy in the panel', async () => {
        // Cache resolves [Done]; the embedded actual-root policy says only
        // Shipped — the embedded set wins even when the status name is Done.
        const embedded = { done_states: ['Shipped'], is_done: false, due_bucket: null, calendar_day: '2026-10-05' }
        api.showConfig.mockImplementation(async (project?: string) => ({
            ...config(project ?? ''), effective_done_states: ['Done'],
        }))
        const wrapper = await panel('A-6', 'Done', embedded)
        const vm = wrapper.vm as any
        expect(vm.form.project).toBe('A')

        const badge = wrapper.find('[data-testid="task-panel-status-badge"]')
        expect(badge.classes()).not.toContain('badge--success')
    })

    it('does not borrow cached Done when the task embeds an empty fail-closed policy', async () => {
        api.showConfig.mockImplementation(async (project?: string) => ({
            ...config(project ?? ''), effective_done_states: ['Done'],
        }))
        const wrapper = await panel('A-7', 'Done', {
            done_states: [], is_done: false, due_bucket: null, calendar_day: '2026-10-05',
        })
        expect(wrapper.find('[data-testid="task-panel-status-badge"]').classes()).toContain('badge--muted')
    })
})
