import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'
import TaskPanel from '../components/TaskPanel.vue'

// DEV-61 regression coverage: managed attachments (`attachment` key, managed
// blob names) and repository files (`file` key, repo-relative paths) are
// distinct reference kinds and must never share hrefs, removal endpoints, or
// previews — including when both share the same basename.
const apiFixtures = vi.hoisted(() => {
    const ATTACHMENT_STORED = 'notes.0123456789abcdef0123456789abcdef.txt'
    const REPO_FILE = 'docs/notes.txt'

    const baseTask = {
        id: 'TP-1',
        title: 'Typed references task',
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
        references: [
            { attachment: ATTACHMENT_STORED },
            { file: REPO_FILE },
            { code: 'src/lib.rs#10-12' },
            { link: 'https://example.com/spec' },
        ],
        history: [],
        custom_fields: {},
    }

    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))

    const state = { task: clone(baseTask) }

    const getTaskMock = vi.fn(async () => clone(state.task))
    const updateTaskMock = vi.fn(async (_id: string, patch: any) => ({ ...clone(state.task), ...patch }))
    const referenceSnippetMock = vi.fn(async (code: string) => ({
        path: code.split('#')[0],
        highlight_start: 10,
        highlight_end: 12,
        lines: [
            { number: 10, text: 'fn demo() {}' },
            { number: 11, text: 'println!("hi");' },
        ],
        has_more_before: false,
        has_more_after: false,
        total_lines: 2,
    }))

    const removeTaskAttachmentMock = vi.fn(async (payload: { id: string; stored_path: string }) => {
        state.task.references = (state.task.references as any[]).filter(
            (entry) => entry.attachment !== payload.stored_path,
        )
        return { task: clone(state.task), deleted: true, still_referenced: false }
    })

    const removeTaskFileReferenceMock = vi.fn(async (payload: { id: string; path: string }) => {
        state.task.references = (state.task.references as any[]).filter(
            (entry) => entry.file !== payload.path,
        )
        return { task: clone(state.task), removed: true }
    })

    const addTaskFileReferenceMock = vi.fn(async (payload: { id: string; path: string }) => {
        state.task.references = [...(state.task.references as any[]), { file: payload.path }]
        return { task: clone(state.task), added: true }
    })

    const uploadTaskAttachmentMock = vi.fn(async (_payload: { id: string; filename: string }) => {
        const stored = 'upload.0123456789abcdef0123456789abcdef.bin'
        state.task.references = [...(state.task.references as any[]), { attachment: stored }]
        return { stored_path: stored, attached: true, task: clone(state.task) }
    })

    const reset = () => {
        state.task = clone(baseTask)
        getTaskMock.mockReset().mockImplementation(async () => clone(state.task))
        updateTaskMock.mockReset().mockImplementation(async (_id: string, patch: any) => ({ ...clone(state.task), ...patch }))
        referenceSnippetMock.mockClear()
        removeTaskAttachmentMock.mockClear().mockImplementation(async (payload: { id: string; stored_path: string }) => {
            state.task.references = (state.task.references as any[]).filter(
                (entry) => entry.attachment !== payload.stored_path,
            )
            return { task: clone(state.task), deleted: true, still_referenced: false }
        })
        removeTaskFileReferenceMock.mockClear().mockImplementation(async (payload: { id: string; path: string }) => {
            state.task.references = (state.task.references as any[]).filter(
                (entry) => entry.file !== payload.path,
            )
            return { task: clone(state.task), removed: true }
        })
        addTaskFileReferenceMock.mockClear().mockImplementation(async (payload: { id: string; path: string }) => {
            state.task.references = [...(state.task.references as any[]), { file: payload.path }]
            return { task: clone(state.task), added: true }
        })
        uploadTaskAttachmentMock.mockClear().mockImplementation(async (_payload: { id: string; filename: string }) => {
            const stored = 'upload.0123456789abcdef0123456789abcdef.bin'
            state.task.references = [...(state.task.references as any[]), { attachment: stored }]
            return { stored_path: stored, attached: true, task: clone(state.task) }
        })
    }

    reset()

    return {
        state,
        reset,
        getTaskMock,
        updateTaskMock,
        referenceSnippetMock,
        removeTaskAttachmentMock,
        removeTaskFileReferenceMock,
        addTaskFileReferenceMock,
        uploadTaskAttachmentMock,
    }
})

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
        referenceSnippet: apiFixtures.referenceSnippetMock,
        addTaskLinkReference: vi.fn(),
        removeTaskLinkReference: vi.fn(),
        addTaskCodeReference: vi.fn(),
        removeTaskCodeReference: vi.fn(),
        addTaskReference: vi.fn(),
        removeTaskReference: vi.fn(),
        listTasks: vi.fn(async () => []),
        listProjects: vi.fn(async () => [{ prefix: 'TP', name: 'Typed Project' }]),
        showConfig: vi.fn(async () => ({
            issue_states: ['Open', 'Closed'],
            issue_priorities: ['Low', 'Medium', 'High'],
            issue_types: ['bug', 'feature'],
            tags: [],
            custom_fields: [],
            default_project: 'TP',
            default_status: 'Open',
            default_priority: 'Medium',
            default_type: 'bug',
            default_reporter: '',
            default_assignee: '',
            default_tags: [],
        })),
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
        removeTaskAttachment: apiFixtures.removeTaskAttachmentMock,
        addTaskFileReference: apiFixtures.addTaskFileReferenceMock,
        removeTaskFileReference: apiFixtures.removeTaskFileReferenceMock,
    },
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
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
    const refresh = vi.fn(async () => { })
    return {
        useConfig: () => ({
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
            refresh,
        }),
    }
})

const openReferencesTab = async (wrapper: ReturnType<typeof mount>) => {
    const referencesTab = wrapper.findAll('.task-panel__tab').find((tab) => tab.text().includes('References'))
    expect(referencesTab).toBeTruthy()
    await referencesTab!.trigger('click')
    await nextTick()
}

// VTU trigger() does not reach Vue listeners inside the dialog's stubbed
// Teleport subtree, so dialog interactions click the live DOM node directly.
const docClick = async (selector: string) => {
    const element = document.querySelector(selector) as HTMLElement | null
    expect(element, `missing ${selector} in document`).toBeTruthy()
    element!.click()
    await nextTick()
}

const setInputValue = async (selector: string, value: string) => {
    const input = document.querySelector(selector) as HTMLInputElement | null
    expect(input, `missing ${selector} in document`).toBeTruthy()
    input!.value = value
    input!.dispatchEvent(new Event('input', { bubbles: true }))
    await nextTick()
}

const mountTaskPanel = async () => {
    const wrapper = mount(TaskPanel, {
        props: {
            open: true,
            taskId: 'TP-1',
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
    await flushPromises()

    return wrapper
}

beforeEach(() => {
    apiFixtures.reset()
    localStorage.setItem('lotar.preferences.taskPanel.showAttachments', 'true')
    localStorage.setItem('lotar.preferences.taskPanel.showLinksInAttachments', 'true')
    localStorage.setItem('lotar.preferences.taskPanel.autoDetectLinks', 'false')
})

afterEach(() => {
    document.body.innerHTML = ''
    localStorage.removeItem('lotar.preferences.taskPanel.showAttachments')
    localStorage.removeItem('lotar.preferences.taskPanel.showLinksInAttachments')
    localStorage.removeItem('lotar.preferences.taskPanel.autoDetectLinks')
})

describe('TaskPanel typed reference kinds (DEV-61)', () => {
    it('renders same-basename attachment and repository file with distinct hrefs and labels', async () => {
        const wrapper = await mountTaskPanel()
        await openReferencesTab(wrapper)

        const items = wrapper.findAll('.task-panel__reference-item')
        expect(items.length).toBe(4)

        // Managed attachment: link into the attachments store; the display
        // name strips the content hash so it shares the repo file's basename.
        const attachmentRow = items.find((item) => item.find('button[aria-label="Remove attachment"]').exists())
        expect(attachmentRow).toBeTruthy()
        const attachmentHref = attachmentRow!.find('a.task-panel__reference-link').attributes('href') ?? ''
        expect(attachmentHref).toContain('/api/attachments/h/0123456789abcdef0123456789abcdef/')
        expect(attachmentRow!.text()).toContain('notes.txt')
        expect(attachmentRow!.find('.task-panel__reference-kind').attributes('title')).toBe('Attachment reference')

        // Repository file: never an attachment href; rendered as previewable text.
        const fileRow = items.find((item) => item.find('button[aria-label="Remove repository file"]').exists())
        expect(fileRow).toBeTruthy()
        expect(fileRow!.find('a.task-panel__reference-link').exists()).toBe(false)
        expect(fileRow!.text()).toContain('docs/notes.txt')
        expect(fileRow!.find('.task-panel__reference-kind').attributes('title')).toBe('Repository file reference')

        // Links and code refs keep their existing rendering.
        const linkRow = items.find((item) => item.text().includes('https://example.com/spec'))
        expect(linkRow).toBeTruthy()
        expect(linkRow!.find('a.task-panel__reference-link').attributes('href')).toBe('https://example.com/spec')
        const codeRow = items.find((item) => item.text().includes('src/lib.rs#10-12'))
        expect(codeRow).toBeTruthy()

        wrapper.unmount()
    })

    it('lists only managed attachments in the panel attachments section', async () => {
        const wrapper = await mountTaskPanel()

        const chipLinks = wrapper.findAll('.task-panel__attachment-link')
        const hrefs = chipLinks.map((link) => link.attributes('href') ?? '')
        const attachmentHrefs = hrefs.filter((href) => href.includes('/api/attachments/'))
        expect(attachmentHrefs.length).toBe(1)
        expect(attachmentHrefs[0]).toContain('/api/attachments/h/0123456789abcdef0123456789abcdef/')
        const names = wrapper.findAll('.task-panel__attachment-name').map((chip) => chip.text())
        expect(names).toContain('notes.txt')
        expect(names).not.toContain('docs/notes.txt')

        wrapper.unmount()
    })

    it('removes attachments and repository files through their own endpoints', async () => {
        const wrapper = await mountTaskPanel()
        await openReferencesTab(wrapper)

        const attachmentRow = wrapper
            .findAll('.task-panel__reference-item')
            .find((item) => item.find('button[aria-label="Remove attachment"]').exists())
        await attachmentRow!.find('button[aria-label="Remove attachment"]').trigger('click')
        await flushPromises()

        expect(apiFixtures.removeTaskAttachmentMock).toHaveBeenCalledWith({
            id: 'TP-1',
            stored_path: 'notes.0123456789abcdef0123456789abcdef.txt',
        })
        expect(apiFixtures.removeTaskFileReferenceMock).not.toHaveBeenCalled()

        const fileRow = wrapper
            .findAll('.task-panel__reference-item')
            .find((item) => item.find('button[aria-label="Remove repository file"]').exists())
        expect(fileRow).toBeTruthy()
        await fileRow!.find('button[aria-label="Remove repository file"]').trigger('click')
        await flushPromises()

        expect(apiFixtures.removeTaskFileReferenceMock).toHaveBeenCalledWith({
            id: 'TP-1',
            path: 'docs/notes.txt',
        })
        expect(apiFixtures.removeTaskAttachmentMock).toHaveBeenCalledTimes(1)

        wrapper.unmount()
    })

    it('previews repository files anchorless and code refs with their anchor', async () => {
        const wrapper = await mountTaskPanel()
        await openReferencesTab(wrapper)

        const fileRow = wrapper
            .findAll('.task-panel__reference-item')
            .find((item) => item.find('button[aria-label="Remove repository file"]').exists())
        await fileRow!.trigger('mouseenter')
        await flushPromises()

        expect(apiFixtures.referenceSnippetMock).toHaveBeenCalledWith('docs/notes.txt', { before: 6, after: 6 })

        const codeRow = wrapper
            .findAll('.task-panel__reference-item')
            .find((item) => item.text().includes('src/lib.rs#10-12'))
        await codeRow!.trigger('mouseenter')
        await flushPromises()

        expect(apiFixtures.referenceSnippetMock).toHaveBeenCalledWith('src/lib.rs#10-12', { before: 6, after: 6 })

        // Attachment rows never request a repository snippet.
        const attachmentRow = wrapper
            .findAll('.task-panel__reference-item')
            .find((item) => item.find('button[aria-label="Remove attachment"]').exists())
        await attachmentRow!.trigger('mouseenter')
        await flushPromises()
        expect(apiFixtures.referenceSnippetMock).toHaveBeenCalledTimes(2)

        wrapper.unmount()
    })

    it('adds a repository file reference through the dialog file tab', async () => {
        const wrapper = await mountTaskPanel()
        await openReferencesTab(wrapper)

        await wrapper.find('[data-testid="references-add"]').trigger('click')
        await nextTick()

        expect(document.querySelector('[data-testid="references-add-dialog"]')).toBeTruthy()

        await docClick('[data-testid="references-add-tab-file"]')
        await setInputValue('#task-panel-add-file-input', 'docs/extra.md')

        const form = document.querySelector('.task-panel__references-dialog form') as HTMLFormElement | null
        expect(form).toBeTruthy()
        form!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
        await flushPromises()
        await nextTick()

        expect(apiFixtures.addTaskFileReferenceMock).toHaveBeenCalledWith({ id: 'TP-1', path: 'docs/extra.md' })
        expect(wrapper.text()).toContain('docs/extra.md')

        wrapper.unmount()
    })

    it('keys same-value attachment and repository file rows independently', async () => {
        // The exact same string is valid as both a managed blob name and a
        // repository-root file path; rows must key by typed kind, not raw value.
        const sameName = 'notes.0123456789abcdef0123456789abcdef.txt'
        apiFixtures.state.task.references = [{ attachment: sameName }, { file: sameName }] as any

        const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => { })
        let wrapper: ReturnType<typeof mount> | null = null
        try {
            wrapper = await mountTaskPanel()
            await openReferencesTab(wrapper)

            let rows = wrapper.findAll('.task-panel__reference-item')
            expect(rows.length).toBe(2)

            const fileRow = rows.find((row) => row.find('button[aria-label="Remove repository file"]').exists())
            const attachmentRow = rows.find((row) => row.find('button[aria-label="Remove attachment"]').exists())
            expect(fileRow).toBeTruthy()
            expect(attachmentRow).toBeTruthy()
            expect(fileRow!.find('a.task-panel__reference-link').exists()).toBe(false)
            expect(attachmentRow!.find('a.task-panel__reference-link').attributes('href')).toContain('/api/attachments/')
            const fileRowElement = fileRow!.element

            // Re-render the list while both same-key rows exist (hover changes
            // parent preview state and re-renders the tab): duplicated raw
            // value keys would surface as duplicate-key warnings here.
            await fileRow!.trigger('mouseenter')
            await flushPromises()
            await nextTick()
            rows = wrapper.findAll('.task-panel__reference-item')
            expect(rows.length).toBe(2)
            expect(rows.filter((row) => row.find('button[aria-label="Remove attachment"]').exists()).length).toBe(1)
            expect(rows.filter((row) => row.find('button[aria-label="Remove repository file"]').exists()).length).toBe(1)

            // Remove the managed attachment first: the repository-file row must
            // survive while the attachment row leaves the DOM.
            await attachmentRow!.find('button[aria-label="Remove attachment"]').trigger('click')
            await flushPromises()
            expect(apiFixtures.removeTaskAttachmentMock).toHaveBeenCalledWith({ id: 'TP-1', stored_path: sameName })
            rows = wrapper.findAll('.task-panel__reference-item')
            expect(rows.length).toBe(1)
            expect(rows[0]!.find('button[aria-label="Remove repository file"]').exists()).toBe(true)
            expect(rows[0]!.find('button[aria-label="Remove attachment"]').exists()).toBe(false)
            expect(rows[0]!.text()).toContain(sameName)
            // The surviving row must be the original repository-file DOM node,
            // not the attachment row's node patched in place: raw-value keys
            // make Vue reuse the first same-keyed node across kinds.
            expect(rows[0]!.element).toBe(fileRowElement)

            // Then remove the repository file reference; the list empties.
            await rows[0]!.find('button[aria-label="Remove repository file"]').trigger('click')
            await flushPromises()
            expect(apiFixtures.removeTaskFileReferenceMock).toHaveBeenCalledWith({ id: 'TP-1', path: sameName })
            expect(wrapper.findAll('.task-panel__reference-item').length).toBe(0)

            expect(warnSpy.mock.calls.some((call) => String(call[0]).includes('Duplicate keys'))).toBe(false)
        } finally {
            warnSpy.mockRestore()
            wrapper?.unmount()
        }
    })

    it('renders uploaded attachments from the typed attachment key', async () => {
        const wrapper = await mountTaskPanel()

        const file = new File(['hello'], 'upload.bin', { type: 'application/octet-stream' })
        const dataTransfer = {
            files: [file],
            items: [{ kind: 'file' }],
            types: ['Files'],
            getData: () => '',
        }
        const drop = new Event('drop', { bubbles: true, cancelable: true })
        Object.defineProperty(drop, 'dataTransfer', { value: dataTransfer })
        wrapper.find('aside.task-panel').element.dispatchEvent(drop)
        // jsdom's FileReader fires its load event on a macrotask.
        for (let i = 0; i < 10; i += 1) {
            await new Promise((resolve) => setTimeout(resolve, 0))
        }
        await flushPromises()
        await nextTick()
        await flushPromises()

        expect(apiFixtures.uploadTaskAttachmentMock).toHaveBeenCalledTimes(1)
        const names = wrapper.findAll('.task-panel__attachment-name').map((chip) => chip.text())
        expect(names).toContain('upload.bin')
        const hrefs = wrapper.findAll('.task-panel__attachment-link').map((link) => link.attributes('href') ?? '')
        expect(hrefs.some((href) => href.includes('/api/attachments/h/0123456789abcdef0123456789abcdef/'))).toBe(true)

        wrapper.unmount()
    })
})
