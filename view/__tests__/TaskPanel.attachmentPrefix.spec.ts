import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'
import TaskPanel from '../components/TaskPanel.vue'

const apiFixtures = vi.hoisted(() => {
    const baseTask = {
        id: 'ABC-OPS-12',
        title: 'Hyphenated task',
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
            { file: 'reports/notes.txt' },
            { file: 'photo.0123456789abcdef0123456789abcdef.png' },
        ],
        history: [],
        custom_fields: {},
    }

    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))

    const state = { task: clone(baseTask) }

    const getTaskMock = vi.fn(async () => clone(state.task))
    const inspectConfigMock = vi.fn(async (_project: string) => ({
        effective: { remotes: {} },
        global_effective: { remotes: {} },
        global_raw: {},
        project_raw: null,
        has_global_file: false,
        project_exists: false,
        sources: {},
    }))
    const updateTaskMock = vi.fn(async (_id: string, patch: any) => ({ ...clone(state.task), ...patch }))

    const reset = () => {
        state.task = clone(baseTask)
        getTaskMock.mockReset()
        inspectConfigMock.mockReset()
        updateTaskMock.mockReset()
        getTaskMock.mockImplementation(async () => clone(state.task))
        inspectConfigMock.mockImplementation(async (_project: string) => ({
            effective: { remotes: {} },
            global_effective: { remotes: {} },
            global_raw: {},
            project_raw: null,
            has_global_file: false,
            project_exists: false,
            sources: {},
        }))
        updateTaskMock.mockImplementation(async (_id: string, patch: any) => ({ ...clone(state.task), ...patch }))
    }

    reset()

    return { state, reset, getTaskMock, inspectConfigMock, updateTaskMock }
})

vi.mock('../api/client', () => ({
    api: {
        whoami: vi.fn(async () => 'tester'),
        getTask: apiFixtures.getTaskMock,
        updateTask: apiFixtures.updateTaskMock,
        setStatus: vi.fn(),
        addTask: vi.fn(),
        addComment: vi.fn(),
        taskHistory: vi.fn(async () => []),
        taskCommitDiff: vi.fn(async () => ''),
        suggestTasks: vi.fn(async () => []),
        referenceSnippet: vi.fn(async () => null),
        listTasks: vi.fn(async () => []),
        listProjects: vi.fn(async () => [{ prefix: 'ABC-OPS', name: 'Hyphen Project' }]),
        showConfig: vi.fn(async () => ({
            issue_states: ['Open', 'Closed'],
            issue_priorities: ['Low', 'Medium', 'High'],
            issue_types: ['bug', 'feature'],
            tags: [],
            custom_fields: [],
            default_project: 'ABC-OPS',
            default_status: 'Open',
            default_priority: 'Medium',
            default_type: 'bug',
            default_reporter: '',
            default_assignee: '',
            default_tags: [],
        })),
        inspectConfig: apiFixtures.inspectConfigMock,
    },
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))

vi.mock('../composables/useProjects', () => {
    const refresh = vi.fn(async () => { })
    return {
        useProjects: () => ({
            projects: ref([{ prefix: 'ABC-OPS', name: 'Hyphen Project' }]),
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
                    project: 'ABC-OPS',
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

const mountTaskPanel = async () => {
    const wrapper = mount(TaskPanel, {
        props: {
            open: true,
            taskId: 'ABC-OPS-12',
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

describe('TaskPanel hyphenated-prefix attachments and config', () => {
    it('builds attachment URLs with the full exact-case project prefix', async () => {
        const wrapper = await mountTaskPanel()

        const attachmentLinks = wrapper.findAll('.task-panel__attachment-link')
        expect(attachmentLinks.length).toBe(2)

        const plainHref = attachmentLinks[0]!.attributes('href') ?? ''
        expect(plainHref).toContain('/api/attachments/get?')
        expect(plainHref).toContain('project=ABC-OPS')

        const hashedHref = attachmentLinks[1]!.attributes('href') ?? ''
        expect(hashedHref).toContain('/api/attachments/h/0123456789abcdef0123456789abcdef/')
        expect(hashedHref).toContain('project=ABC-OPS')

        wrapper.unmount()
    })

    it('uses the full prefix on the references tab attachment links', async () => {
        const wrapper = await mountTaskPanel()

        const referencesTab = wrapper.findAll('.task-panel__tab').find((tab) => tab.text().includes('References'))
        expect(referencesTab).toBeTruthy()
        await referencesTab!.trigger('click')
        await nextTick()

        const referenceLinks = wrapper.findAll('.task-panel__reference-link')
        expect(referenceLinks.length).toBeGreaterThanOrEqual(2)
        for (const link of referenceLinks) {
            const href = link.attributes('href') ?? ''
            expect(href).toContain('project=ABC-OPS')
        }

        wrapper.unmount()
    })

    it('inspects config for the full prefix, never the first dash segment', async () => {
        const wrapper = await mountTaskPanel()

        const inspected = apiFixtures.inspectConfigMock.mock.calls.map((call) => call[0])
        expect(inspected).toContain('ABC-OPS')
        expect(inspected).not.toContain('ABC')

        wrapper.unmount()
    })
})
