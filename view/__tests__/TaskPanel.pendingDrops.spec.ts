import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'
import TaskPanel from '../components/TaskPanel.vue'

// DEV-93 lifecycle coverage for drops that arrive while the task panel is
// still initializing an edit target: the buffered payload belongs to the
// task that was loading when it was dropped. It must upload exactly once
// for that task after its load completes, and must never leak into another
// task (target swap), fire after the panel closes, queue in create mode,
// or promote the link payload of a files-bearing mixed event. All drops are
// real DOM `drop` events on the panel root with deferred getTask/config
// gates controlling the initialization window — no internal-function calls.
const apiFixtures = vi.hoisted(() => {
    const makeTask = (id: string): any => ({
        id,
        title: `Task ${id}`,
        status: 'Open',
        priority: 'Medium',
        task_type: 'bug',
        reporter: '',
        assignee: '',
        due_date: '',
        effort: '',
        description: '',
        tags: [] as string[],
        relationships: {
            depends_on: [],
            blocks: [],
            related: [],
            children: [],
            fixes: [],
            parent: undefined,
            duplicate_of: undefined,
        } as any,
        comments: [] as any[],
        references: [] as any[],
        history: [] as any[],
        custom_fields: {},
    })

    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))

    const state = {
        taskGate: Promise.resolve() as Promise<void>,
        configGate: Promise.resolve() as Promise<void>,
        tasks: { 'TP-1': makeTask('TP-1'), 'TP-2': makeTask('TP-2') } as Record<string, any>,
    }

    const getTaskMock = vi.fn(async (id: string) => {
        await state.taskGate
        return clone(state.tasks[id] ?? makeTask(id))
    })
    const updateTaskMock = vi.fn(async (id: string, patch: any) => ({
        ...clone(state.tasks[id] ?? makeTask(id)),
        ...patch,
    }))
    const configRefreshMock = vi.fn(async () => {
        await state.configGate
    })
    const uploadTaskAttachmentMock = vi.fn(async (payload: { id: string; filename: string }) => {
        const task = state.tasks[payload.id] ?? makeTask(payload.id)
        task.references = [...(task.references as any[]), { attachment: 'early.0123456789abcdef0123456789abcdef.bin' }]
        return {
            stored_path: 'early.0123456789abcdef0123456789abcdef.bin',
            attached: true,
            task: clone(task),
        }
    })
    const addTaskLinkReferenceMock = vi.fn(async (payload: { id: string; url: string }) => {
        const task = state.tasks[payload.id] ?? makeTask(payload.id)
        task.references = [...(task.references as any[]), { link: payload.url }]
        return { task: clone(task), added: true }
    })

    const reset = () => {
        state.taskGate = Promise.resolve()
        state.configGate = Promise.resolve()
        state.tasks = { 'TP-1': makeTask('TP-1'), 'TP-2': makeTask('TP-2') }
        getTaskMock.mockClear()
        updateTaskMock.mockClear()
        configRefreshMock.mockClear()
        uploadTaskAttachmentMock.mockClear()
        addTaskLinkReferenceMock.mockClear()
    }

    reset()

    return {
        state,
        reset,
        getTaskMock,
        updateTaskMock,
        configRefreshMock,
        uploadTaskAttachmentMock,
        addTaskLinkReferenceMock,
    }
})

const showToastMock = vi.hoisted(() => vi.fn())

vi.mock('../api/client', () => ({
    api: {
        whoami: vi.fn(async () => 'tester'),
        getTask: apiFixtures.getTaskMock,
        updateTask: apiFixtures.updateTaskMock,
        setStatus: vi.fn(),
        addTask: vi.fn(),
        addComment: vi.fn(),
        updateComment: vi.fn(),
        taskHistory: vi.fn(async () => []),
        suggestTasks: vi.fn(async () => []),
        suggestReferenceFiles: vi.fn(async () => []),
        referenceSnippet: vi.fn(),
        addTaskLinkReference: apiFixtures.addTaskLinkReferenceMock,
        removeTaskLinkReference: vi.fn(),
        addTaskCodeReference: vi.fn(),
        removeTaskCodeReference: vi.fn(),
        addTaskReference: vi.fn(),
        removeTaskReference: vi.fn(),
        listTasks: vi.fn(async () => []),
        listProjects: vi.fn(async () => [{ prefix: 'TP', name: 'Typed Project' }]),
        showConfig: vi.fn(async () => ({})),
        inspectConfig: vi.fn(async () => ({
            effective: { remotes: {} },
            global_effective: { remotes: {} },
            global_raw: {},
            project_raw: null,
            has_global_file: false,
            project_exists: false,
            sources: {},
        })),
        uploadTaskAttachment: apiFixtures.uploadTaskAttachmentMock,
        removeTaskAttachment: vi.fn(),
        addTaskFileReference: vi.fn(),
        removeTaskFileReference: vi.fn(),
    },
}))

vi.mock('../components/toast', () => ({
    showToast: showToastMock,
}))

vi.mock('../composables/useProjects', () => {
    const refresh = vi.fn(async () => { })
    return {
        useProjects: () => ({
            projects: ref([{ prefix: 'TP', name: 'Typed Project' }]),
            refresh,
        }),
    }
})

vi.mock('../composables/useConfig', () => {
    return {
        useConfig: () => ({
            cfg: ref(null),
            loading: ref(false),
            error: ref(null),
            statuses: ref(['Open', 'Closed']),
            priorities: ref(['Low', 'Medium', 'High']),
            types: ref(['bug', 'feature']),
            tags: ref([]),
            customFields: ref([]),
            members: ref([] as string[]),
            defaults: {
                value: {
                    project: 'TP',
                    status: 'Open',
                    priority: 'Medium',
                    type: 'bug',
                    reporter: '',
                    assignee: '',
                    tags: [] as string[],
                    customFields: {},
                },
            },
            sprintDefaults: { value: {} },
            attachmentsDir: ref('@attachments'),
            refresh: apiFixtures.configRefreshMock,
        }),
    }
})

// Deferred gates keeping the panel inside its initialization window. Every
// held gate is force-released in afterEach so a failing assertion can never
// leave a pending load behind.
const gateReleases: Array<() => void> = []

const holdTaskGate = (): (() => void) => {
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
        release = resolve
    })
    apiFixtures.state.taskGate = gate
    gateReleases.push(release)
    return () => release()
}

const holdConfigGate = (): (() => void) => {
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
        release = resolve
    })
    apiFixtures.state.configGate = gate
    gateReleases.push(release)
    return () => release()
}

const drains = async () => {
    // jsdom's FileReader fires its load event on a macrotask.
    for (let i = 0; i < 10; i += 1) {
        await new Promise((resolve) => setTimeout(resolve, 0))
    }
    await flushPromises()
    await nextTick()
    await flushPromises()
}

const mountPanel = async (taskId: string) => {
    const wrapper = mount(TaskPanel, {
        props: {
            open: true,
            taskId,
        },
        global: {
            stubs: {
                Teleport: true,
            },
        },
        attachTo: document.body,
    })
    await flushPromises()
    await nextTick()
    return wrapper
}

// Real DOM drop on the panel root, mirroring the smoke fixture's dispatch.
const dispatchFileDrop = async (
    wrapper: ReturnType<typeof mount>,
    files: File[],
    uriList = '',
) => {
    const dataTransfer = {
        files,
        items: files.map((file) => ({ kind: 'file' })),
        types: uriList ? ['Files', 'text/uri-list'] : ['Files'],
        getData: (type: string) => (type === 'text/uri-list' ? uriList : ''),
    }
    const drop = new Event('drop', { bubbles: true, cancelable: true })
    Object.defineProperty(drop, 'dataTransfer', { value: dataTransfer })
    wrapper.find('aside.task-panel').element.dispatchEvent(drop)
    await drains()
}

const toastTexts = () => showToastMock.mock.calls.map((call) => String(call[0]))

beforeEach(() => {
    apiFixtures.reset()
    showToastMock.mockClear()
    localStorage.setItem('lotar.preferences.taskPanel.showAttachments', 'true')
    localStorage.setItem('lotar.preferences.taskPanel.showLinksInAttachments', 'true')
    localStorage.setItem('lotar.preferences.taskPanel.autoDetectLinks', 'false')
})

afterEach(() => {
    gateReleases.splice(0).forEach((release) => release())
    document.body.innerHTML = ''
    localStorage.removeItem('lotar.preferences.taskPanel.showAttachments')
    localStorage.removeItem('lotar.preferences.taskPanel.showLinksInAttachments')
    localStorage.removeItem('lotar.preferences.taskPanel.autoDetectLinks')
})

describe('TaskPanel pending drop lifecycle (DEV-93)', () => {
    it('buffers an eager drop through the config phase of task load and uploads it exactly once for that task', async () => {
        const releaseConfig = holdConfigGate()
        const wrapper = await mountPanel('TP-1')
        try {
            await dispatchFileDrop(wrapper, [
                new File(['early payload'], 'early.bin', { type: 'application/octet-stream' }),
            ])

            // Buffered, not uploaded: task fields are not applied yet.
            expect(apiFixtures.uploadTaskAttachmentMock).not.toHaveBeenCalled()
            expect(toastTexts().some((text) => text.includes('still loading'))).toBe(true)

            releaseConfig()
            await drains()

            expect(apiFixtures.uploadTaskAttachmentMock).toHaveBeenCalledTimes(1)
            expect(apiFixtures.uploadTaskAttachmentMock.mock.calls[0]![0]).toMatchObject({
                id: 'TP-1',
                filename: 'early.bin',
            })
            const names = wrapper.findAll('.task-panel__attachment-name').map((chip) => chip.text())
            expect(names).toContain('early.bin')
        } finally {
            wrapper.unmount()
        }
    })

    it('discards the buffer without any API call when the panel closes before the task loads', async () => {
        const releaseTasks = holdTaskGate()
        const wrapper = await mountPanel('TP-1')
        try {
            await dispatchFileDrop(wrapper, [
                new File(['close payload'], 'close.bin', { type: 'application/octet-stream' }),
            ])
            expect(apiFixtures.uploadTaskAttachmentMock).not.toHaveBeenCalled()

            await wrapper.setProps({ open: false })
            releaseTasks()
            await drains()

            expect(apiFixtures.uploadTaskAttachmentMock).not.toHaveBeenCalled()
        } finally {
            wrapper.unmount()
        }
    })

    it('never uploads a drop buffered for task A into task B after a target swap before A loads', async () => {
        const releaseTasks = holdTaskGate()
        const wrapper = await mountPanel('TP-1')
        try {
            await dispatchFileDrop(wrapper, [
                new File(['a payload'], 'a-file.bin', { type: 'application/octet-stream' }),
            ])
            expect(apiFixtures.uploadTaskAttachmentMock).not.toHaveBeenCalled()

            // Controller swaps the open panel to another task while the
            // first is still loading. B must load normally, but A's buffered
            // drop belongs to A and must not attach to B.
            await wrapper.setProps({ taskId: 'TP-2' })
            releaseTasks()
            await drains()

            expect(apiFixtures.getTaskMock).toHaveBeenCalledWith('TP-2')
            expect(wrapper.text()).toContain('Task TP-2')
            expect(apiFixtures.uploadTaskAttachmentMock).not.toHaveBeenCalled()
        } finally {
            wrapper.unmount()
        }
    })

    it('rejects queuing in create mode and never flushes the drop after switching to edit', async () => {
        const wrapper = await mountPanel('new')
        try {
            await dispatchFileDrop(wrapper, [
                new File(['create payload'], 'create.bin', { type: 'application/octet-stream' }),
            ])

            expect(
                toastTexts().some((text) => text.includes('Save the task before attaching files')),
            ).toBe(true)
            expect(toastTexts().some((text) => text.includes('still loading'))).toBe(false)
            expect(apiFixtures.uploadTaskAttachmentMock).not.toHaveBeenCalled()

            await wrapper.setProps({ taskId: 'TP-1' })
            await drains()

            expect(apiFixtures.getTaskMock).toHaveBeenCalledWith('TP-1')
            expect(apiFixtures.uploadTaskAttachmentMock).not.toHaveBeenCalled()
        } finally {
            wrapper.unmount()
        }
    })

    it('gives files priority in a mixed drop event and never adds the event link payload later', async () => {
        const releaseConfig = holdConfigGate()
        const wrapper = await mountPanel('TP-1')
        try {
            await dispatchFileDrop(
                wrapper,
                [new File(['mixed payload'], 'mixed.bin', { type: 'application/octet-stream' })],
                'https://example.com/mixed-drop',
            )
            expect(apiFixtures.uploadTaskAttachmentMock).not.toHaveBeenCalled()

            releaseConfig()
            await drains()

            expect(apiFixtures.uploadTaskAttachmentMock).toHaveBeenCalledTimes(1)
            expect(apiFixtures.uploadTaskAttachmentMock.mock.calls[0]![0]).toMatchObject({
                id: 'TP-1',
                filename: 'mixed.bin',
            })
            expect(apiFixtures.addTaskLinkReferenceMock).not.toHaveBeenCalled()
        } finally {
            wrapper.unmount()
        }
    })
})
