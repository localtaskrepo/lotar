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
        references: [
            { code: 'src/lib.rs', link: 'https://example.com/lib.rs' },
            { code: 'src/lib.rs#10', link: 'https://example.com/lib.rs#10' },
        ],
        history: [],
        custom_fields: {},
    }

    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))

    const state = {
        task: clone(baseTask),
    }

    const getTaskMock = vi.fn(async () => clone(state.task))
    const updateTaskMock = vi.fn(async (_id: string, patch: any) => {
        state.task = {
            ...state.task,
            ...patch,
            tags: patch.tags !== undefined ? [...patch.tags] : state.task.tags,
            relationships: patch.relationships !== undefined ? clone(patch.relationships) : state.task.relationships,
        }
        return clone(state.task)
    })

    const referenceSnippetMock = vi.fn(async () => ({
        path: 'src/lib.rs',
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
        getTaskMock.mockReset()
        updateTaskMock.mockReset()
        referenceSnippetMock.mockReset()
        listTasksMock.mockReset()
        listProjectsMock.mockReset()
        showConfigMock.mockReset()

        getTaskMock.mockImplementation(async () => clone(state.task))
        updateTaskMock.mockImplementation(async (_id: string, patch: any) => {
            state.task = {
                ...state.task,
                ...patch,
                tags: patch.tags !== undefined ? [...patch.tags] : state.task.tags,
                relationships: patch.relationships !== undefined ? clone(patch.relationships) : state.task.relationships,
            }
            return clone(state.task)
        })
        referenceSnippetMock.mockImplementation(async () => ({
            path: 'src/lib.rs',
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
        listTasksMock.mockImplementation(async () => [])
        listProjectsMock.mockImplementation(async () => [{ prefix: 'DEMO', name: 'Demo Project' }])
        showConfigMock.mockImplementation(async () => ({
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
        updateTaskMock,
        referenceSnippetMock,
        listTasksMock,
        listProjectsMock,
        showConfigMock,
    }
})

vi.mock('../api/client', () => ({
    api: {
        whoami: vi.fn(async () => 'tester'),
        getTask: apiFixtures.getTaskMock,
        updateTask: apiFixtures.updateTaskMock,
        setStatus: vi.fn(async (_id: string, status: string) => ({
            ...apiFixtures.state.task,
            status,
        })),
        addTask: vi.fn(),
        addComment: vi.fn(),
        taskHistory: vi.fn(async () => []),
        suggestTasks: vi.fn(async () => []),
        referenceSnippet: apiFixtures.referenceSnippetMock,
        listTasks: apiFixtures.listTasksMock,
        listProjects: apiFixtures.listProjectsMock,
        showConfig: apiFixtures.showConfigMock,
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
    apiFixtures.reset()
})

afterEach(() => {
    document.body.innerHTML = ''
})

describe('TaskPanel reference preview regression', () => {
    it('hovering references should not throw', async () => {
        const wrapper = await mountTaskPanel()

        const referencesTab = wrapper.findAll('.task-panel__tab').find((tab) => tab.text().includes('References'))
        expect(referencesTab).toBeTruthy()
        await referencesTab!.trigger('click')
        await nextTick()

        const referenceItems = wrapper.findAll('.task-panel__reference-item')
        expect(referenceItems.length).toBeGreaterThan(0)

        const hover = async (index: number) => {
            await referenceItems[index].trigger('mouseenter')
            await flushPromises()
            await nextTick()
        }

        await expect(hover(0)).resolves.toBeUndefined()
        await expect(hover(1)).resolves.toBeUndefined()

        wrapper.unmount()
    })
})
