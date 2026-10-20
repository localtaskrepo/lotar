import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'

const api = vi.hoisted(() => ({
    addTask: vi.fn(),
    updateTask: vi.fn(),
    getTask: vi.fn(),
    sprintAdd: vi.fn(),
    sprintRemove: vi.fn(),
    whoami: vi.fn(async () => 'tester'),
    listTasks: vi.fn(async () => ({ status: 'ok', count: 0, total: 0, limit: 200, offset: 0, tasks: [] })),
    showConfig: vi.fn(async () => ({})),
}))

const refreshConfig = vi.hoisted(() => vi.fn(async () => { }))

vi.mock('../api/client', () => ({ api }))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))

vi.mock('../composables/useProjects', () => ({
    useProjects: () => ({
        projects: ref([{ prefix: 'DEMO', name: 'Demo Project' }]),
        refresh: vi.fn(async () => { }),
    }),
}))

vi.mock('../composables/useConfig', () => ({
    useConfig: () => ({
        statuses: ref(['Open', 'Closed']),
        priorities: ref(['Low', 'Medium', 'High']),
        types: ref(['bug', 'feature']),
        tags: ref([]),
        customFields: ref([]),
        members: ref([]),
        defaults: {
            value: {
                project: 'DEMO',
                status: 'Open',
                priority: 'Medium',
                type: 'bug',
                reporter: '',
                assignee: '',
                tags: [],
            },
        },
        refresh: refreshConfig,
    }),
}))

const sprintsStore = {
    sprints: ref<any[]>([]),
    loading: ref(false),
    missingSprints: ref<number[]>([]),
    active: ref<any[]>([]),
    refresh: vi.fn(async () => { }),
}

vi.mock('../composables/useSprints', () => ({
    useSprints: () => sprintsStore,
}))

import TaskPanel from '../components/TaskPanel.vue'
import { showToast } from '../components/toast'

const createdTask = {
    id: 'DEMO-200',
    title: 'Sprint task',
    status: 'Open',
    priority: 'Medium',
    task_type: 'bug',
    reporter: '',
    assignee: '',
    due_date: '',
    effort: '',
    description: '',
    tags: [],
    sprints: [101],
    relationships: {},
    comments: [],
    references: [],
    history: [],
    custom_fields: {},
}

function setSprintFixtures(sprints: any[], missing: number[] = []) {
    sprintsStore.sprints.value = sprints
    sprintsStore.missingSprints.value = missing
    sprintsStore.active.value = sprints.filter((s) => s.state === 'active' || s.state === 'overdue')
}

const defaultSprints = [
    { id: 101, label: 'Sprint 101', display_name: 'Sprint 101', state: 'active' },
    { id: 102, label: 'Sprint 102', display_name: 'Sprint 102', state: 'complete' },
    { id: 103, label: 'Sprint 103', display_name: 'Sprint 103', state: 'active' },
]

const mountedPanels: VueWrapper[] = []
const mountHosts: HTMLElement[] = []

async function mountCreatePanel(props: Record<string, any> = {}, options: { realTeleport?: boolean } = {}) {
    const host = document.createElement('div')
    host.dataset.testTaskPanelMount = ''
    document.body.append(host)
    mountHosts.push(host)
    const wrapper = mount(TaskPanel, {
        props: {
            open: true,
            taskId: 'new',
            ...props,
        },
        global: {
            stubs: options.realTeleport ? {} : { Teleport: true },
        },
        attachTo: host,
    })
    mountedPanels.push(wrapper)

    await flushPromises()
    await nextTick()
    await flushPromises()
    return wrapper
}

type PanelWrapper = Awaited<ReturnType<typeof mountCreatePanel>>

async function cleanupPanels() {
    for (const wrapper of mountedPanels.splice(0).reverse()) wrapper.unmount()
    await flushPromises()
    for (const host of mountHosts.splice(0)) host.remove()
    sprintsStore.sprints.value = []
    sprintsStore.missingSprints.value = []
    sprintsStore.active.value = []
}

async function submitCreate(wrapper: PanelWrapper) {
    ;(wrapper.vm as any).form.title = 'Sprint task'
    await nextTick()
    await (wrapper.vm as any).handleSubmit()
    await flushPromises()
}

function dialogKey(target: Element, name: string, shiftKey = false) {
    const event = new KeyboardEvent('keydown', { key: name, shiftKey, bubbles: true, cancelable: true })
    target.dispatchEvent(event)
    return event
}

function findSprintModalDialog() {
    return document.querySelector<HTMLElement>('.ui-modal__overlay [role="dialog"]')
}

describe('TaskPanel create-mode sprint destination (DEV-104)', () => {
    beforeEach(() => {
        refreshConfig.mockReset().mockResolvedValue(undefined)
        setSprintFixtures(defaultSprints)
        api.addTask.mockReset().mockResolvedValue({ ...createdTask })
        api.updateTask.mockReset()
        api.getTask.mockReset()
        api.sprintAdd.mockReset()
        api.sprintRemove.mockReset()
        vi.mocked(showToast).mockClear()
    })

    afterEach(cleanupPanels)

    it('cleans mounted resources after an async test body throws', async () => {
        const beforeOverflow = document.body.style.getPropertyValue('overflow')
        const beforePriority = document.body.style.getPropertyPriority('overflow')
        const outside = document.createElement('button')
        const inertBefore = document.createElement('div')
        inertBefore.setAttribute('inert', '')
        document.body.append(outside, inertBefore)
        document.body.style.setProperty('overflow', 'clip', 'important')
        const added = vi.spyOn(window, 'addEventListener')
        const removed = vi.spyOn(window, 'removeEventListener')
        let wrapper: PanelWrapper | undefined
        const failure = new Error('simulated async test failure')

        try {
            await expect((async () => {
                try {
                    wrapper = await mountCreatePanel({ initialSprint: 101 }, { realTeleport: true })
                    document.querySelector<HTMLButtonElement>('button[aria-label="Assign sprint"]')!.click()
                    await nextTick()
                    await flushPromises()
                    expect(findSprintModalDialog()).not.toBeNull()
                    expect(document.body.style.overflow).toBe('hidden')
                    expect(outside.hasAttribute('inert')).toBe(true)
                    throw failure
                } finally {
                    // Exercise the same cleanup registered with afterEach.
                    await cleanupPanels()
                }
            })()).rejects.toBe(failure)

            expect(wrapper!.exists()).toBe(false)
            expect(findSprintModalDialog()).toBeNull()
            expect(document.querySelector('.ui-modal__overlay')).toBeNull()
            expect(document.querySelector('.task-panel')).toBeNull()
            expect(document.querySelector('[data-test-task-panel-mount]')).toBeNull()
            expect(document.body.style.getPropertyValue('overflow')).toBe('clip')
            expect(document.body.style.getPropertyPriority('overflow')).toBe('important')
            expect(outside.hasAttribute('inert')).toBe(false)
            expect(inertBefore.hasAttribute('inert')).toBe(true)
            const preferencesListeners = added.mock.calls.filter(([name]) => name === 'lotar:preferences-changed')
            expect(preferencesListeners.length).toBeGreaterThan(0)
            for (const [, listener] of preferencesListeners) {
                expect(removed).toHaveBeenCalledWith('lotar:preferences-changed', listener)
            }
            outside.focus()
            expect(document.activeElement).toBe(outside)
            expect(dialogKey(document.body, 'Escape').defaultPrevented).toBe(false)
        } finally {
            // Keep the regression isolated if lifecycle cleanup regresses.
            if (wrapper?.exists()) wrapper.unmount()
            outside.remove()
            inertBefore.remove()
            document.body.style.setProperty('overflow', beforeOverflow, beforePriority)
            added.mockRestore()
            removed.mockRestore()
        }
    })

    it('prefills initialSprint into the draft and submits it in the atomic create payload', async () => {
        const wrapper = await mountCreatePanel({ initialSprint: 101 })

        expect((wrapper.vm as any).form.sprints).toEqual([101])
        expect(wrapper.findAll('.task-panel__sprint-chip').length).toBe(1)

        await submitCreate(wrapper)

        expect(api.addTask).toHaveBeenCalledTimes(1)
        expect(api.addTask.mock.calls[0]![0]).toMatchObject({ sprints: [101] })
        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(wrapper.emitted('created')).toBeTruthy()
    })

    it('keeps the initial sprint destination when sprint metadata is missing or stale', async () => {
        setSprintFixtures([], [555])
        const wrapper = await mountCreatePanel({ initialSprint: 555 })

        expect((wrapper.vm as any).form.sprints).toEqual([555])

        await submitCreate(wrapper)

        expect(api.addTask).toHaveBeenCalledTimes(1)
        expect(api.addTask.mock.calls[0]![0]).toMatchObject({ sprints: [555] })
    })

    it('edits draft sprint membership locally without any API calls', async () => {
        const wrapper = await mountCreatePanel({ initialSprint: 101 })

        await wrapper.find('button[aria-label="Assign sprint"]').trigger('click')
        await nextTick()

        const dialog = wrapper.find('.task-panel-dialog__form')
        expect(dialog.exists()).toBe(true)

        const select = dialog.find('select')
        const optionValues = dialog.findAll('option').map((option) => option.element.value)
        expect(optionValues).toEqual(['101', '102', '103'])

        await select.setValue('103')
        await dialog.trigger('submit')
        await flushPromises()

        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.sprintRemove).not.toHaveBeenCalled()
        expect(api.updateTask).not.toHaveBeenCalled()
        expect((wrapper.vm as any).form.sprints).toEqual([101, 103])

        const removeChip = wrapper.find('button[aria-label="Remove #101 Sprint 101"]')
        expect(removeChip.exists()).toBe(true)
        await removeChip.trigger('click')
        await nextTick()

        expect(api.sprintRemove).not.toHaveBeenCalled()
        expect((wrapper.vm as any).form.sprints).toEqual([103])

        await submitCreate(wrapper)
        expect(api.addTask.mock.calls[0]![0]).toMatchObject({ sprints: [103] })
    })

    it('enforces the closed-sprint opt-in for draft additions', async () => {
        const wrapper = await mountCreatePanel()

        await wrapper.find('button[aria-label="Assign sprint"]').trigger('click')
        await nextTick()

        const dialog = wrapper.find('.task-panel-dialog__form')
        await dialog.find('select').setValue('102')
        await dialog.trigger('submit')
        await flushPromises()

        expect((wrapper.vm as any).form.sprints).toEqual([])
        expect(vi.mocked(showToast).mock.calls.some((call) => String(call[0]).includes('closed sprints'))).toBe(true)

        const allowClosed = dialog.find('.task-panel-dialog__checkbox input')
        expect(allowClosed.exists()).toBe(true)
        await allowClosed.setValue(true)
        await dialog.trigger('submit')
        await flushPromises()

        expect((wrapper.vm as any).form.sprints).toEqual([102])
        expect(api.sprintAdd).not.toHaveBeenCalled()
    })

    it('clears the prior sprint prefill across reopen sessions', async () => {
        const wrapper = await mountCreatePanel({ initialSprint: 101 })
        expect((wrapper.vm as any).form.sprints).toEqual([101])

        await wrapper.setProps({ open: false })
        await flushPromises()
        expect((wrapper.vm as any).form.sprints).toEqual([])

        await wrapper.setProps({ open: true })
        await flushPromises()
        expect((wrapper.vm as any).form.sprints).toEqual([101])

        await wrapper.setProps({ open: false })
        await flushPromises()
        await wrapper.setProps({ open: true, initialSprint: null })
        await flushPromises()
        expect((wrapper.vm as any).form.sprints).toEqual([])

        await submitCreate(wrapper)
        expect(api.addTask.mock.calls[0]![0].sprints).toBeUndefined()
    })

    it('keeps the global sprint destination when the draft project changes', async () => {
        const wrapper = await mountCreatePanel({ initialSprint: 101 })

        ;(wrapper.vm as any).form.project = 'OTHER'
        await (wrapper.vm as any).onProjectChange()
        await flushPromises()
        await nextTick()
        await flushPromises()

        expect((wrapper.vm as any).form.project).toBe('OTHER')
        expect((wrapper.vm as any).form.sprints).toEqual([101])

        await submitCreate(wrapper)
        expect(api.addTask.mock.calls[0]![0]).toMatchObject({ project: 'OTHER', sprints: [101] })
    })

    it('keeps draft sprint edits made while project configuration is loading', async () => {
        const wrapper = await mountCreatePanel({ initialSprint: 101 })
        let finishRefresh!: () => void
        refreshConfig.mockReturnValueOnce(new Promise<void>(resolve => { finishRefresh = resolve }))
        ;(wrapper.vm as any).form.project = 'OTHER'
        ;(wrapper.vm as any).onProjectChange()
        await nextTick()
        await wrapper.find('button[aria-label="Remove #101 Sprint 101"]').trigger('click')
        expect((wrapper.vm as any).form.sprints).toEqual([])
        finishRefresh()
        await flushPromises()
        expect((wrapper.vm as any).form.sprints).toEqual([])
        await submitCreate(wrapper)
        expect(api.addTask.mock.calls[0]![0].sprints).toBeUndefined()
    })

    it('closes the nested sprint dialog on Escape, restores the trigger focus and keeps the draft untouched', async () => {
        const wrapper = await mountCreatePanel({ initialSprint: 101 }, { realTeleport: true })
        ;(wrapper.vm as any).form.title = 'Draft title'
        ;(wrapper.vm as any).form.description = 'Draft description'

        const trigger = document.querySelector('button[aria-label="Assign sprint"]') as HTMLButtonElement
        trigger.focus()
        trigger.click()
        await nextTick()
        await flushPromises()

        const dialog = findSprintModalDialog()
        expect(dialog).not.toBeNull()
        expect(dialog!.getAttribute('aria-modal')).toBe('true')
        expect(dialog!.getAttribute('aria-label')).toBe('Assign to sprint')
        expect(document.querySelector('.task-panel-dialog__form--sprint')).not.toBeNull()
        expect(document.body.style.overflow).toBe('hidden')
        expect((wrapper.vm as any).form.sprints).toEqual([101])

        expect(dialogKey(dialog!, 'Escape').defaultPrevented).toBe(true)
        await nextTick()
        await flushPromises()

        expect(document.querySelector('.task-panel-dialog__form--sprint')).toBeNull()
        expect(document.activeElement).toBe(trigger)
        expect(document.body.style.overflow).toBe('')
        expect((wrapper.vm as any).form.sprints).toEqual([101])
        expect((wrapper.vm as any).form.title).toBe('Draft title')
        expect((wrapper.vm as any).form.description).toBe('Draft description')
        expect(api.sprintAdd).not.toHaveBeenCalled()
        expect(api.sprintRemove).not.toHaveBeenCalled()
        expect(api.updateTask).not.toHaveBeenCalled()
        expect(api.addTask).not.toHaveBeenCalled()
    })

    it('contains Tab focus within the nested sprint dialog', async () => {
        const wrapper = await mountCreatePanel({ initialSprint: 101 }, { realTeleport: true })

        const trigger = document.querySelector('button[aria-label="Assign sprint"]') as HTMLButtonElement
        trigger.click()
        await nextTick()
        await flushPromises()

        const dialog = findSprintModalDialog() as HTMLElement
        const controls = Array.from(
            dialog.querySelectorAll<HTMLElement>('button, input:not([type="hidden"]), select'),
        ).filter(element => element.tabIndex >= 0 && !(element as HTMLButtonElement).disabled)
        const first = controls[0]!
        const last = controls[controls.length - 1]!
        expect(controls.length).toBeGreaterThanOrEqual(4)
        expect(document.activeElement).toBe(first)

        last.focus()
        expect(dialogKey(last, 'Tab').defaultPrevented).toBe(true)
        expect(document.activeElement).toBe(first)
        expect(dialogKey(first, 'Tab', true).defaultPrevented).toBe(true)
        expect(document.activeElement).toBe(last)
    })

    it('keeps the nested sprint dialog open on Escape while a sprint request is pending', async () => {
        api.getTask.mockResolvedValue({ ...createdTask })
        let resolveAdd!: (value: any) => void
        api.sprintAdd.mockReturnValue(new Promise<any>(resolve => { resolveAdd = resolve }))
        const wrapper = await mountCreatePanel({ taskId: 'DEMO-200' }, { realTeleport: true })
        await flushPromises()

        const trigger = document.querySelector('button[aria-label="Assign sprint"]') as HTMLButtonElement
        trigger.click()
        await nextTick()
        expect(document.querySelector('.task-panel-dialog__form--sprint')).not.toBeNull()

        const form = document.querySelector('.task-panel-dialog__form--sprint') as HTMLFormElement
        form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
        await nextTick()
        await flushPromises()
        expect(api.sprintAdd).toHaveBeenCalledTimes(1)

        dialogKey(findSprintModalDialog()!, 'Escape')
        await nextTick()
        await flushPromises()
        expect(document.querySelector('.task-panel-dialog__form--sprint')).not.toBeNull()

        resolveAdd({ modified: [], replaced: [], integrity: {} })
        await flushPromises()
        await nextTick()
        expect(document.querySelector('.task-panel-dialog__form--sprint')).toBeNull()
    })
})
