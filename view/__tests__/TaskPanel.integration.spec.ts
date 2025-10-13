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
            depends_on: ['DEMO-101'],
            blocks: [],
            related: [],
            children: [],
            fixes: [],
            parent: undefined,
            duplicate_of: undefined,
        } as any,
        comments: [] as any[],
        references: [
            {
                code: 'src/lib.rs',
                link: 'https://example.com/lib.rs',
            },
        ],
        history: [
            {
                at: '2025-09-30T00:00:00.000Z',
                actor: 'alice',
                changes: [
                    { field: 'title', old: 'Old title', new: 'Demo task' },
                ],
            },
        ],
        custom_fields: {},
        category: null,
    }

    const baseCommits = [
        {
            commit: 'abc1234',
            author: 'Bob',
            email: 'bob@example.com',
            date: '2025-09-29T10:00:00.000Z',
            message: 'Initial commit',
        },
    ]

    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))

    const state = {
        task: clone(baseTask),
    }

    const getTaskMock = vi.fn(async () => {
        state.task = clone(baseTask)
        return clone(state.task)
    })

    const updateTaskMock = vi.fn(async (_id: string, patch: any) => {
        state.task = {
            ...state.task,
            ...patch,
            tags: patch.tags !== undefined ? [...patch.tags] : state.task.tags,
            relationships: patch.relationships !== undefined ? clone(patch.relationships) : state.task.relationships,
        }
        return clone(state.task)
    })

    const taskHistoryMock = vi.fn(async () => clone(baseCommits))
    const suggestTasksMock = vi.fn(async () => [
        { id: 'DEMO-777', title: 'Suggested work item' },
    ])
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
    const addCommentMock = vi.fn()
    const setStatusMock = vi.fn(async (_id: string, status: string) => ({
        ...clone(baseTask),
        status,
    }))
    const listTasksMock = vi.fn(async () => [])
    const listProjectsMock = vi.fn(async () => [{ prefix: 'DEMO', name: 'Demo Project' }])
    const showConfigMock = vi.fn(async () => ({
        issue_states: ['Open', 'Closed'],
        issue_priorities: ['Low', 'Medium', 'High'],
        issue_types: ['bug', 'feature'],
        tags: ['alpha', 'beta', 'gamma'],
        default_prefix: 'DEMO',
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
        taskHistoryMock.mockReset()
        suggestTasksMock.mockReset()
        referenceSnippetMock.mockReset()
        addCommentMock.mockReset()
        setStatusMock.mockReset()
        listTasksMock.mockReset()
        listProjectsMock.mockReset()
        showConfigMock.mockReset()

        getTaskMock.mockImplementation(async () => {
            state.task = clone(baseTask)
            return clone(state.task)
        })
        updateTaskMock.mockImplementation(async (_id: string, patch: any) => {
            state.task = {
                ...state.task,
                ...patch,
                tags: patch.tags !== undefined ? [...patch.tags] : state.task.tags,
                relationships: patch.relationships !== undefined ? clone(patch.relationships) : state.task.relationships,
            }
            return clone(state.task)
        })
        taskHistoryMock.mockImplementation(async () => clone(baseCommits))
        suggestTasksMock.mockImplementation(async () => [
            { id: 'DEMO-777', title: 'Suggested work item' },
        ])
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
        addCommentMock.mockImplementation(async () => clone(baseTask))
        setStatusMock.mockImplementation(async (_id: string, status: string) => ({
            ...clone(baseTask),
            status,
        }))
        listTasksMock.mockImplementation(async () => [])
        listProjectsMock.mockImplementation(async () => [{ prefix: 'DEMO', name: 'Demo Project' }])
        showConfigMock.mockImplementation(async () => ({
            issue_states: ['Open', 'Closed'],
            issue_priorities: ['Low', 'Medium', 'High'],
            issue_types: ['bug', 'feature'],
            tags: ['alpha', 'beta', 'gamma'],
            default_prefix: 'DEMO',
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
        baseTask,
        state,
        clone,
        getTaskMock,
        updateTaskMock,
        taskHistoryMock,
        suggestTasksMock,
        referenceSnippetMock,
        addCommentMock,
        setStatusMock,
        listTasksMock,
        listProjectsMock,
        showConfigMock,
        reset,
    }
})

vi.mock('../api/client', () => ({
    api: {
        whoami: vi.fn(async () => 'tester'),
        getTask: apiFixtures.getTaskMock,
        updateTask: apiFixtures.updateTaskMock,
        setStatus: apiFixtures.setStatusMock,
        addTask: vi.fn(),
        addComment: apiFixtures.addCommentMock,
        taskHistory: apiFixtures.taskHistoryMock,
        suggestTasks: apiFixtures.suggestTasksMock,
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
        category: '',
        tags: [] as string[],
    }
    return {
        useConfig: () => ({
            statuses,
            priorities,
            types,
            tags,
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
    vi.useRealTimers()
    document.body.innerHTML = ''
})

describe('TaskPanel integration safeguards', () => {
    it('updates tags via API when a suggestion is chosen', async () => {
        const wrapper = await mountTaskPanel()
        apiFixtures.updateTaskMock.mockClear()

        const tagInput = wrapper.find('#task-panel-tags-input')
        expect(tagInput.exists()).toBe(true)

        await tagInput.trigger('focus')
        await tagInput.setValue('al')
        await nextTick()

        const suggestions = wrapper.findAll('.task-panel__tag-suggestion')
        expect(suggestions.length).toBeGreaterThan(0)

        await suggestions[0].trigger('click')
        await flushPromises()

        expect((wrapper.vm as any).form.tags).toContain('alpha')
        expect(apiFixtures.updateTaskMock).toHaveBeenCalledTimes(1)
        expect(apiFixtures.updateTaskMock.mock.calls[0][1]).toMatchObject({ tags: ['alpha'] })

        wrapper.unmount()
    })

    it('only commits relationship changes when values differ from baseline', async () => {
        vi.useFakeTimers()
        const wrapper = await mountTaskPanel()
        apiFixtures.updateTaskMock.mockClear()

        const relationshipsTab = wrapper.findAll('.task-panel__tab').find((tab) => tab.text().includes('Relationships'))
        expect(relationshipsTab).toBeTruthy()
        await relationshipsTab!.trigger('click')
        await nextTick()

        const dependsInput = wrapper.find('input[placeholder="IDs comma separated"]')
        expect(dependsInput.exists()).toBe(true)

        await dependsInput.trigger('blur')
        await flushPromises()
        expect(apiFixtures.updateTaskMock).not.toHaveBeenCalled()

        await dependsInput.trigger('focus')
        await dependsInput.setValue('99')
        await nextTick()

        vi.runAllTimers()
        await flushPromises()

        const suggestionItems = wrapper.findAll('.task-panel__relation-suggest li')
        expect(suggestionItems.length).toBeGreaterThan(0)

        await suggestionItems[0].trigger('mousedown')
        await flushPromises()

        expect(apiFixtures.updateTaskMock).toHaveBeenCalledTimes(1)
        const payload = apiFixtures.updateTaskMock.mock.calls[0][1]
        expect(payload).toHaveProperty('relationships')
        expect(payload.relationships.depends_on).toContain('DEMO-777')

        wrapper.unmount()
    })

    it('shows commit entries in the commits tab and refreshes on demand', async () => {
        const wrapper = await mountTaskPanel()
        await flushPromises()

        const commitsTab = wrapper.findAll('.task-panel__tab').find((tab) => tab.text().includes('Commits'))
        expect(commitsTab).toBeTruthy()
        await commitsTab!.trigger('click')
        await nextTick()

        const commitEntries = wrapper.findAll('.task-panel__commits-list .task-panel__history-item')
        expect(commitEntries.length).toBeGreaterThan(0)

        apiFixtures.taskHistoryMock.mockClear()
        const refreshButton = wrapper.findAll('button').find((button) => button.text().includes('Refresh'))
        expect(refreshButton).toBeTruthy()
        await refreshButton!.trigger('click')
        await flushPromises()

        expect(apiFixtures.taskHistoryMock).toHaveBeenCalledTimes(1)

        wrapper.unmount()
    })
})
