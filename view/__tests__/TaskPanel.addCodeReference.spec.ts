import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'
import TaskPanel from '../components/TaskPanel.vue'

const apiFixtures = vi.hoisted(() => {
    const baseTask = {
        id: 'DEMO-123',
        title: 'Demo task',
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
        history: [],
        custom_fields: {},
    }

    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))

    const state = {
        task: clone(baseTask),
    }

    const getTaskMock = vi.fn(async () => clone(state.task))

    const referenceSnippetMock = vi.fn(async () => ({
        path: 'src/lib.rs',
        start_line: 8,
        end_line: 14,
        highlight_start: 10,
        highlight_end: 12,
        lines: [
            { number: 8, text: 'fn before() {}' },
            { number: 9, text: 'fn demo() {' },
            { number: 10, text: '  println!("hi");' },
            { number: 11, text: '  println!("bye");' },
            { number: 12, text: '}' },
            { number: 13, text: 'fn after() {}' },
            { number: 14, text: '' },
        ],
        has_more_before: false,
        has_more_after: false,
        total_lines: 14,
    }))

    const suggestReferenceFilesMock = vi.fn(async () => ['src/lib.rs', 'src/main.rs'])

    const addTaskCodeReferenceMock = vi.fn(async (_payload: any) => {
        const next = {
            ...clone(state.task),
            references: [...(state.task.references || []), { code: 'src/lib.rs#10-12' }],
        }
        state.task = clone(next)
        return { task: clone(state.task), added: true }
    })

    const removeTaskCodeReferenceMock = vi.fn(async (payload: any) => {
        const code = typeof payload?.code === 'string' ? payload.code.trim() : ''
        if (!code) return { task: clone(state.task), removed: false }

        const refs = Array.isArray(state.task.references) ? state.task.references : []
        const next = refs.filter((ref: any) => (typeof ref?.code === 'string' ? ref.code.trim() : '') !== code)
        const removed = next.length !== refs.length

        state.task = clone({
            ...state.task,
            references: next,
        })
        return { task: clone(state.task), removed }
    })

    const listTasksMock = vi.fn(async () => [])
    const listProjectsMock = vi.fn(async () => [{ prefix: 'DEMO', name: 'Demo Project' }])
    const showConfigMock = vi.fn(async () => ({
        issue_states: ['Open', 'Closed'],
        issue_priorities: ['Low', 'Medium', 'High'],
        issue_types: ['bug', 'feature'],
        tags: ['alpha', 'beta', 'gamma'],
        custom_fields: ['product'],
        default_project: 'DEMO',
        default_status: 'Open',
        default_priority: 'Medium',
        default_type: 'bug',
        default_reporter: '',
        default_assignee: '',
        default_tags: [],
    }))

    const reset = () => {
        state.task = clone(baseTask)
        getTaskMock.mockReset().mockImplementation(async () => clone(state.task))
        referenceSnippetMock.mockReset().mockImplementation(async () => ({
            path: 'src/lib.rs',
            start_line: 8,
            end_line: 14,
            highlight_start: 10,
            highlight_end: 12,
            lines: [
                { number: 8, text: 'fn before() {}' },
                { number: 9, text: 'fn demo() {' },
                { number: 10, text: '  println!("hi");' },
                { number: 11, text: '  println!("bye");' },
                { number: 12, text: '}' },
                { number: 13, text: 'fn after() {}' },
                { number: 14, text: '' },
            ],
            has_more_before: false,
            has_more_after: false,
            total_lines: 14,
        }))
        suggestReferenceFilesMock.mockReset().mockImplementation(async () => ['src/lib.rs', 'src/main.rs'])
        addTaskCodeReferenceMock.mockReset().mockImplementation(async () => {
            const next = {
                ...clone(state.task),
                references: [...(state.task.references || []), { code: 'src/lib.rs#10-12' }],
            }
            state.task = clone(next)
            return { task: clone(state.task), added: true }
        })
        removeTaskCodeReferenceMock.mockReset().mockImplementation(async (payload: any) => {
            const code = typeof payload?.code === 'string' ? payload.code.trim() : ''
            if (!code) return { task: clone(state.task), removed: false }

            const refs = Array.isArray(state.task.references) ? state.task.references : []
            const next = refs.filter((ref: any) => (typeof ref?.code === 'string' ? ref.code.trim() : '') !== code)
            const removed = next.length !== refs.length

            state.task = clone({
                ...state.task,
                references: next,
            })
            return { task: clone(state.task), removed }
        })
        listTasksMock.mockReset().mockImplementation(async () => [])
        listProjectsMock.mockReset().mockImplementation(async () => [{ prefix: 'DEMO', name: 'Demo Project' }])
        showConfigMock.mockReset().mockImplementation(async () => ({
            issue_states: ['Open', 'Closed'],
            issue_priorities: ['Low', 'Medium', 'High'],
            issue_types: ['bug', 'feature'],
            tags: ['alpha', 'beta', 'gamma'],
            custom_fields: ['product'],
            default_project: 'DEMO',
            default_status: 'Open',
            default_priority: 'Medium',
            default_type: 'bug',
            default_reporter: '',
            default_assignee: '',
            default_tags: [],
        }))
    }

    reset()

    return {
        state,
        reset,
        getTaskMock,
        referenceSnippetMock,
        suggestReferenceFilesMock,
        addTaskCodeReferenceMock,
        removeTaskCodeReferenceMock,
        listTasksMock,
        listProjectsMock,
        showConfigMock,
    }
})

vi.mock('../api/client', () => ({
    api: {
        whoami: vi.fn(async () => 'tester'),
        getTask: apiFixtures.getTaskMock,
        updateTask: vi.fn(),
        setStatus: vi.fn(async (_id: string, status: string) => ({
            ...apiFixtures.state.task,
            status,
        })),
        addTask: vi.fn(),
        addComment: vi.fn(),
        updateComment: vi.fn(),
        taskHistory: vi.fn(async () => []),
        suggestTasks: vi.fn(async () => []),
        referenceSnippet: apiFixtures.referenceSnippetMock,
        suggestReferenceFiles: apiFixtures.suggestReferenceFilesMock,
        addTaskCodeReference: apiFixtures.addTaskCodeReferenceMock,
        removeTaskCodeReference: apiFixtures.removeTaskCodeReferenceMock,
        addTaskLinkReference: vi.fn(),
        removeTaskLinkReference: vi.fn(),
        uploadTaskAttachment: vi.fn(),
        removeTaskAttachment: vi.fn(),
        listTasks: apiFixtures.listTasksMock,
        listProjects: apiFixtures.listProjectsMock,
        showConfig: apiFixtures.showConfigMock,
        inspectConfig: vi.fn(async () => ({
            effective: { remotes: {} },
            global_effective: { remotes: {} },
            global_raw: {},
            project_raw: null,
            has_global_file: false,
            project_exists: false,
            sources: {},
        })),
    },
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))

vi.mock('../composables/useProjects', () => {
    const refresh = vi.fn(async () => { })
    return {
        useProjects: () => ({
            projects: ref([{ prefix: 'DEMO', name: 'Demo Project' }]),
            refresh,
        }),
    }
})

vi.mock('../composables/useConfig', () => {
    const refresh = vi.fn(async () => { })
    const statuses = ref(['Open', 'Closed'])
    const priorities = ref(['Low', 'Medium', 'High'])
    const types = ref(['bug', 'feature'])
    const tags = ref(['alpha', 'beta', 'gamma'])
    const defaults = {
        project: 'DEMO',
        status: 'Open',
        priority: 'Medium',
        type: 'bug',
        reporter: '',
        assignee: '',
        tags: [] as string[],
        customFields: { product: '' },
    }
    return {
        useConfig: () => ({
            statuses,
            priorities,
            types,
            tags,
            customFields: ref(['product']),
            members: ref([] as string[]),
            defaults: { value: defaults },
            refresh,
        }),
    }
})

const mountTaskPanel = async () => {
    const wrapper = mount(TaskPanel, {
        props: {
            open: true,
            taskId: 'DEMO-123',
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
    vi.useFakeTimers()
    apiFixtures.reset()
})

afterEach(() => {
    vi.useRealTimers()
    document.body.innerHTML = ''
})

describe('TaskPanel add code reference dialog', () => {
    it('adds a code reference and shows preview', async () => {
        const wrapper = await mountTaskPanel()

        const referencesTab = wrapper.findAll('.task-panel__tab').find((tab) => tab.text().includes('References'))
        expect(referencesTab).toBeTruthy()
        await referencesTab!.trigger('click')
        await nextTick()

        await wrapper.find('[data-testid="references-add"]').trigger('click')
        await nextTick()

        let dialog = wrapper.find('[data-testid="references-add-dialog"]')
        expect(dialog.exists()).toBe(true)

        await dialog.find('[data-testid="references-add-tab-code"]').trigger('click')
        await nextTick()

        dialog = wrapper.find('[data-testid="references-add-dialog"]')
        expect(dialog.exists()).toBe(true)

        await dialog.find('#task-panel-add-code-file-input').setValue('src/lib.rs')
        await dialog.find('#task-panel-add-code-start').setValue('10')
        await dialog.find('#task-panel-add-code-end').setValue('12')

        vi.runAllTimers()
        await flushPromises()

        expect(apiFixtures.suggestReferenceFilesMock).toHaveBeenCalled()
        expect(apiFixtures.referenceSnippetMock).toHaveBeenCalledWith('src/lib.rs#10-12', { before: 6, after: 6 })

        await dialog.find('form').trigger('submit')
        await flushPromises()

        expect(apiFixtures.addTaskCodeReferenceMock).toHaveBeenCalledWith({ id: 'DEMO-123', code: 'src/lib.rs#10-12' })

        const referenceItems = wrapper.findAll('.task-panel__reference-item')
        expect(referenceItems.length).toBeGreaterThan(0)
        expect(wrapper.text()).toContain('src/lib.rs#10-12')
    })

    it('removes a code reference via the references list', async () => {
        const wrapper = await mountTaskPanel()

        const referencesTab = wrapper.findAll('.task-panel__tab').find((tab) => tab.text().includes('References'))
        expect(referencesTab).toBeTruthy()
        await referencesTab!.trigger('click')
        await nextTick()

        await wrapper.find('[data-testid="references-add"]').trigger('click')
        await nextTick()

        let dialog = wrapper.find('[data-testid="references-add-dialog"]')
        expect(dialog.exists()).toBe(true)

        await dialog.find('[data-testid="references-add-tab-code"]').trigger('click')
        await nextTick()

        dialog = wrapper.find('[data-testid="references-add-dialog"]')
        expect(dialog.exists()).toBe(true)

        await dialog.find('#task-panel-add-code-file-input').setValue('src/lib.rs')
        await dialog.find('#task-panel-add-code-start').setValue('10')
        await dialog.find('#task-panel-add-code-end').setValue('12')

        vi.runAllTimers()
        await flushPromises()

        await dialog.find('form').trigger('submit')
        await flushPromises()

        expect(wrapper.text()).toContain('src/lib.rs#10-12')

        const referenceItem = wrapper
            .findAll('.task-panel__reference-item')
            .find((item) => item.text().includes('src/lib.rs#10-12'))
        expect(referenceItem).toBeTruthy()

        const removeButton = referenceItem!.find('button[aria-label="Remove code reference"]')
        expect(removeButton.exists()).toBe(true)
        await removeButton.trigger('click')
        await flushPromises()

        expect(apiFixtures.removeTaskCodeReferenceMock).toHaveBeenCalledWith({ id: 'DEMO-123', code: 'src/lib.rs#10-12' })
        expect(wrapper.text()).not.toContain('src/lib.rs#10-12')
    })
})
