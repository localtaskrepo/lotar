import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

// DEV-263: Escape closes the panel like the close button does: the field
// being edited is saved first, an Escape an inner control used stays there,
// and a create draft is not thrown away without asking.

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
import { _resetTaskStore } from '../composables/useTaskStore'

function task(extra: Record<string, unknown> = {}) {
    return {
        id: 'A-3',
        title: 'Third task',
        status: 'Open',
        priority: 'Medium',
        task_type: 'Task',
        reporter: '',
        assignee: '',
        description: 'Old description',
        tags: [],
        sprints: [],
        relationships: {},
        comments: [],
        references: [],
        history: [],
        custom_fields: {},
        modified: '2026-10-01T10:00:00Z',
        ...extra,
    }
}

let wrappers: VueWrapper[] = []

async function openPanel(taskId: string, options: { realTeleport?: boolean } = {}) {
    const wrapper = mount(TaskPanel, {
        props: { open: true, taskId, initialProject: 'A' },
        global: { stubs: options.realTeleport ? {} : { Teleport: true } },
        attachTo: document.body,
    })
    wrappers.push(wrapper)
    await flushPromises()
    api.updateTask.mockClear()
    return wrapper
}

function pressEscape(target: Element | null) {
    const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    ;(target ?? document.body).dispatchEvent(event)
    return event
}

beforeEach(() => {
    vi.resetAllMocks()
    _resetTaskStore()
    localStorage.clear()
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
        default_status: 'Open',
        default_priority: 'Medium',
        tags: ['alpha', 'beta'],
        custom_fields: [],
    })
    api.getTask.mockImplementation(async () => task())
    api.updateTask.mockImplementation(async (_id: string, patch: any) => ({ ...task(), ...patch }))
})

afterEach(() => {
    wrappers.forEach((wrapper) => wrapper.unmount())
    wrappers = []
    _resetTaskStore()
    document.body.innerHTML = ''
})

describe('TaskPanel Escape (DEV-263)', () => {
    it('saves the focused title before closing', async () => {
        const wrapper = await openPanel('A-3')
        const input = wrapper.find('input[placeholder="Title"]')
        ;(input.element as HTMLInputElement).focus()
        await input.setValue('Third task (edited title)')

        pressEscape(input.element)
        await flushPromises()

        expect(api.updateTask).toHaveBeenCalledWith('A-3', { title: 'Third task (edited title)' })
        expect(wrapper.emitted('close')).toHaveLength(1)
    })

    it('saves the description being edited before closing', async () => {
        const wrapper = await openPanel('A-3')
        await wrapper.find('.task-panel__description-display').trigger('click')
        await nextTick()
        const textarea = wrapper.find('.task-panel__description textarea')
        ;(textarea.element as HTMLTextAreaElement).focus()
        await textarea.setValue('Typed then Escape')

        pressEscape(textarea.element)
        await flushPromises()

        expect(api.updateTask).toHaveBeenCalledTimes(1)
        expect(api.updateTask).toHaveBeenCalledWith('A-3', { description: 'Typed then Escape' })
        expect(wrapper.emitted('close')).toHaveLength(1)
    })

    it('saves the description being edited when the close button is used', async () => {
        const wrapper = await openPanel('A-3')
        await wrapper.find('.task-panel__description-display').trigger('click')
        await nextTick()
        const textarea = wrapper.find('.task-panel__description textarea')
        ;(textarea.element as HTMLTextAreaElement).focus()
        await textarea.setValue('Typed then closed')

        await wrapper.find('button[aria-label="Close panel"]').trigger('click')
        await flushPromises()

        expect(api.updateTask).toHaveBeenCalledWith('A-3', { description: 'Typed then closed' })
        expect(wrapper.emitted('close')).toHaveLength(1)
    })

    it('leaves the panel open when an inner control already used the Escape', async () => {
        const wrapper = await openPanel('A-3')
        const input = wrapper.find('input[placeholder="Title"]')
        input.element.addEventListener('keydown', (event) => event.preventDefault())
        pressEscape(input.element)
        await flushPromises()
        expect(wrapper.emitted('close')).toBeUndefined()
    })

    it('closes only the tag dialog on Escape in its input', async () => {
        const wrapper = await openPanel('A-3')
        await wrapper.find('[data-testid="tags-add"]').trigger('click')
        await flushPromises()
        expect(wrapper.find('[data-testid="tag-dialog"]').exists()).toBe(true)

        const event = pressEscape(wrapper.find('#task-panel-tags-input').element)
        await flushPromises()

        expect(event.defaultPrevented).toBe(true)
        expect(wrapper.find('[data-testid="tag-dialog"]').exists()).toBe(false)
        expect(wrapper.emitted('close')).toBeUndefined()
    })

    it('asks before Escape discards a create draft', async () => {
        const wrapper = await openPanel('new', { realTeleport: true })
        const vm = wrapper.vm as any
        const title = () => document.querySelector('input[placeholder="Title"]') as HTMLInputElement
        const find = (id: string) => document.querySelector(`[data-testid="${id}"]`) as HTMLElement | null
        title().focus()
        title().value = 'Unsaved draft'
        title().dispatchEvent(new Event('input', { bubbles: true }))
        await nextTick()

        pressEscape(title())
        await flushPromises()
        expect(wrapper.emitted('close')).toBeUndefined()
        expect(find('task-panel-discard-cancel')).not.toBeNull()

        find('task-panel-discard-cancel')!.click()
        await flushPromises()
        expect(find('task-panel-discard-cancel')).toBeNull()
        expect(vm.form.title).toBe('Unsaved draft')
        expect(wrapper.emitted('close')).toBeUndefined()

        pressEscape(title())
        await flushPromises()
        find('task-panel-discard-confirm')!.click()
        await flushPromises()
        expect(wrapper.emitted('close')).toHaveLength(1)
        expect(api.addTask).not.toHaveBeenCalled()
    })

    it('closes an empty create draft on Escape without asking', async () => {
        const wrapper = await openPanel('new')
        pressEscape(document.body)
        await flushPromises()
        expect(wrapper.emitted('close')).toHaveLength(1)
        expect(wrapper.find('[data-testid="task-panel-discard-cancel"]').exists()).toBe(false)
    })
})
