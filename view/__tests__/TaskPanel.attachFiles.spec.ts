import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'
import TaskPanel from '../components/TaskPanel.vue'

const last = <T>(items: T[]): T => items[items.length - 1]!
import { namePastedFiles } from '../composables/task-panel/useTaskPanelAttachmentDrop'

// DEV-216: files can be attached with a picker and by pasting (screenshots),
// not only by drag and drop, and the panel says so even before the first file.
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

const showToastMock = vi.hoisted(() => vi.fn(() => 1))

const extraMocks = vi.hoisted(() => ({
    removeTaskAttachment: vi.fn(async (payload: { id: string; stored_path: string }) => ({
        task: { id: payload.id, title: 'Task', references: [] },
        deleted: true,
        still_referenced: false,
    })),
    reorderTaskAttachments: vi.fn(async (payload: { id: string; attachments: string[] }) => ({
        id: payload.id,
        title: `Task ${payload.id}`,
        references: payload.attachments.map((attachment) => ({ attachment })),
    })),
    attachmentInfo: vi.fn(async () => ({
        items: [{ attachment: 'one.0123456789abcdef0123456789abcdef.png', size: 2048, width: 640, height: 480 }],
    })),
}))

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
        removeTaskAttachment: extraMocks.removeTaskAttachment,
        reorderTaskAttachments: extraMocks.reorderTaskAttachments,
        attachmentInfo: extraMocks.attachmentInfo,
        addTaskFileReference: vi.fn(),
        removeTaskFileReference: vi.fn(),
    },
}))

vi.mock('../components/toast', () => ({
    showToast: showToastMock,
    dismissToast: vi.fn(),
}))

vi.mock('../composables/useProjects', () => {
    const refresh = vi.fn(async () => {})
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

const drains = async () => {
    // jsdom's FileReader fires its load event on a macrotask.
    for (let i = 0; i < 10; i += 1) {
        await new Promise((resolve) => setTimeout(resolve, 0))
    }
    await flushPromises()
    await nextTick()
}

const mountPanel = async (taskId: string | null) => {
    const wrapper = mount(TaskPanel, {
        props: { open: true, taskId },
        global: { stubs: { Teleport: true } },
        attachTo: document.body,
    })
    await drains()
    return wrapper
}

function paste(target: EventTarget, files: File[], types: string[] = ['Files']) {
    const event = new Event('paste', { bubbles: true, cancelable: true })
    Object.defineProperty(event, 'clipboardData', { value: { files, types } })
    target.dispatchEvent(event)
    return event
}

const png = () => new File([new Uint8Array([137, 80, 78, 71])], 'image.png', { type: 'image/png' })

let wrappers: Array<ReturnType<typeof mount>> = []

beforeEach(() => {
    apiFixtures.reset()
    showToastMock.mockClear()
    wrappers = []
})

afterEach(() => {
    wrappers.forEach((wrapper) => wrapper.unmount())
    document.body.innerHTML = ''
})

describe('attaching files from the task panel', () => {
    it('offers an Attach button and a drop/paste hint before the first file', async () => {
        const wrapper = await mountPanel('TP-1')
        wrappers.push(wrapper)
        expect(wrapper.find('.task-panel__attach-button').exists()).toBe(true)
        expect(wrapper.find('.task-panel__attachments-empty').text()).toContain('paste a screenshot')
    })

    it('uploads files chosen with the picker', async () => {
        const wrapper = await mountPanel('TP-1')
        wrappers.push(wrapper)
        const input = wrapper.find('input[data-testid="attachment-picker"]')
        const file = new File(['spec'], 'spec.pdf', { type: 'application/pdf' })
        Object.defineProperty(input.element, 'files', { value: [file], configurable: true })
        await input.trigger('change')
        await drains()
        expect(apiFixtures.uploadTaskAttachmentMock).toHaveBeenCalledTimes(1)
        expect(apiFixtures.uploadTaskAttachmentMock.mock.calls[0]![0]).toMatchObject({
            id: 'TP-1',
            filename: 'spec.pdf',
        })
        expect(wrapper.find('.task-panel__attachments-empty').exists()).toBe(false)
    })

    it('attaches a pasted screenshot under a findable name', async () => {
        const wrapper = await mountPanel('TP-1')
        wrappers.push(wrapper)
        const event = paste(wrapper.find('.task-panel').element, [png()])
        expect(event.defaultPrevented).toBe(true)
        await drains()
        expect(apiFixtures.uploadTaskAttachmentMock).toHaveBeenCalledTimes(1)
        expect(apiFixtures.uploadTaskAttachmentMock.mock.calls[0]![0].filename).toMatch(
            /^pasted-\d{4}-\d{2}-\d{2}-\d{6}\.png$/,
        )
    })

    it('leaves text pastes into fields alone', async () => {
        const wrapper = await mountPanel('TP-1')
        wrappers.push(wrapper)
        const field = wrapper.find('input').element
        const event = paste(field, [png()], ['text/plain', 'Files'])
        expect(event.defaultPrevented).toBe(false)
        await drains()
        expect(apiFixtures.uploadTaskAttachmentMock).not.toHaveBeenCalled()
    })

    it('asks to save a new task before attaching', async () => {
        const wrapper = await mountPanel(null)
        wrappers.push(wrapper)
        expect(wrapper.find('.task-panel__attach-button').exists()).toBe(false)
        paste(wrapper.find('.task-panel').element, [png()])
        await drains()
        expect(apiFixtures.uploadTaskAttachmentMock).not.toHaveBeenCalled()
        expect(showToastMock).toHaveBeenCalledWith('Save the task before attaching files')
    })
})

describe('namePastedFiles', () => {
    it('renames generic clipboard names and keeps real ones', () => {
        const now = new Date(2026, 9, 26, 21, 30, 5)
        const named = namePastedFiles([png(), new File(['x'], 'design.png'), new File(['y'], 'image.jpeg')], now)
        expect(named.map((file) => file.name)).toEqual([
            'pasted-2026-10-26-213005-1.png',
            'design.png',
            'pasted-2026-10-26-213005-2.jpg',
        ])
        expect(namePastedFiles([png()], now)[0]!.name).toBe('pasted-2026-10-26-213005.png')
    })
})

const HASH = '0123456789abcdef0123456789abcdef'
const ONE = `one.${HASH}.png`
const TWO = `two.${HASH.replace('0', '1')}.png`
const DOC = `spec.${HASH.replace('0', '2')}.pdf`

async function mountWithFiles() {
    apiFixtures.state.tasks['TP-1']!.references = [{ attachment: ONE }, { attachment: DOC }, { attachment: TWO }]
    const wrapper = await mountPanel('TP-1')
    wrappers.push(wrapper)
    return wrapper
}

describe('working with attached files', () => {
    beforeEach(() => {
        Object.values(extraMocks).forEach((mock) => mock.mockClear())
    })

    it('marks the cover, makes another image the cover, and shows sizes in tooltips', async () => {
        const wrapper = await mountWithFiles()
        const items = wrapper.findAll('.task-panel__gallery-item')
        expect(items).toHaveLength(2)
        expect(items[0]!.find('.task-panel__cover-badge').exists()).toBe(true)
        expect(items[0]!.find('a').attributes('title')).toBe('one.png · 640×480 · 2.0 KB')

        await items[1]!.find('button[aria-label="Use as cover"]').trigger('click')
        await drains()
        expect(extraMocks.reorderTaskAttachments).toHaveBeenCalledWith({ id: 'TP-1', attachments: [TWO, DOC, ONE] })
        expect(wrapper.findAll('.task-panel__gallery-name').map((name) => name.text())).toEqual(['two.png', 'one.png'])
    })

    it('opens the viewer on click but leaves modified clicks to the browser', async () => {
        const wrapper = await mountWithFiles()
        const link = wrapper.findAll('.task-panel__gallery-link')[1]!
        await link.trigger('click', { metaKey: true })
        expect(wrapper.find('.attachment-viewer').exists()).toBe(false)
        await link.trigger('click')
        expect(wrapper.find('.attachment-viewer').exists()).toBe(true)
        expect(wrapper.find('.attachment-viewer__count').text()).toBe('2 / 3')
    })

    it('hides a removed file at once, offers Undo, and sends the removal when the panel closes', async () => {
        const wrapper = await mountWithFiles()
        await wrapper
            .findAll('.task-panel__gallery-item')[0]!
            .find('button[aria-label="Remove attachment"]')
            .trigger('click')
        expect(wrapper.findAll('.task-panel__gallery-item')).toHaveLength(1)
        expect(extraMocks.removeTaskAttachment).not.toHaveBeenCalled()

        const toast = last(showToastMock.mock.calls) as unknown as [
            string,
            unknown,
            number,
            { label: string; run: () => void },
        ]
        expect(toast[0]).toBe('Removed one.png')
        expect(toast[3].label).toBe('Undo')
        toast[3].run()
        await nextTick()
        expect(wrapper.findAll('.task-panel__gallery-item')).toHaveLength(2)

        await wrapper
            .findAll('.task-panel__gallery-item')[0]!
            .find('button[aria-label="Remove attachment"]')
            .trigger('click')
        await wrapper.setProps({ open: false })
        await drains()
        expect(extraMocks.removeTaskAttachment).toHaveBeenCalledTimes(1)
        expect(extraMocks.removeTaskAttachment).toHaveBeenCalledWith({ id: 'TP-1', stored_path: ONE })
    })

    it('links a screenshot pasted into the description', async () => {
        const wrapper = await mountPanel('TP-1')
        wrappers.push(wrapper)
        await wrapper.find('.task-panel__description-display').trigger('click')
        await nextTick()
        const textarea = wrapper.find('.task-panel__description-body textarea')
        await textarea.setValue('Steps: ')
        ;(textarea.element as HTMLTextAreaElement).setSelectionRange(7, 7)
        const event = paste(textarea.element, [png()])
        expect(event.defaultPrevented).toBe(true)
        await drains()
        expect(apiFixtures.uploadTaskAttachmentMock).toHaveBeenCalledTimes(1)
        expect((textarea.element as HTMLTextAreaElement).value).toBe(`Steps: [early.bin](attachment:early.${HASH}.bin)`)
    })
})
