import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'
import TaskPanel from '../components/TaskPanel.vue'
import { _resetTaskStore, useTaskStore } from '../composables/useTaskStore'

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
        sprints: [] as number[],
        relationships: {} as any,
        comments: [] as any[],
        references: [] as any[],
        history: [],
        custom_fields: {},
        deleted_at: undefined as string | null | undefined,
    }

    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))

    const state = {
        task: clone(baseTask),
    }

    const getTaskMock = vi.fn(async () => clone(state.task))
    const updateTaskMock = vi.fn(async (_id: string, patch: any) => ({ ...clone(state.task), ...patch }))
    const restoreTaskMock = vi.fn(async () => {
        state.task = { ...clone(baseTask) }
        return clone(state.task)
    })
    const deleteTaskMock = vi.fn(async () => ({ deleted: true, hard: true, warnings: ['kept blob: @attachments/shared.png'] }))
    const addCommentMock = vi.fn(async () => clone(state.task))

    const reset = () => {
        state.task = { ...clone(baseTask), deleted_at: '2026-10-05T10:00:00Z' }
        getTaskMock.mockReset().mockImplementation(async () => clone(state.task))
        updateTaskMock.mockClear()
        restoreTaskMock.mockReset().mockImplementation(async () => {
            state.task = { ...clone(baseTask) }
            return clone(state.task)
        })
        deleteTaskMock.mockReset().mockImplementation(async () => ({
            deleted: true,
            hard: true,
            warnings: ['kept blob: @attachments/shared.png'],
        }))
        addCommentMock.mockClear()
    }

    reset()

    return { baseTask, state, clone, getTaskMock, updateTaskMock, restoreTaskMock, deleteTaskMock, addCommentMock, reset }
})

vi.mock('../api/client', () => ({
    api: {
        whoami: vi.fn(async () => 'tester'),
        getTask: apiFixtures.getTaskMock,
        updateTask: apiFixtures.updateTaskMock,
        restoreTask: apiFixtures.restoreTaskMock,
        deleteTask: apiFixtures.deleteTaskMock,
        addComment: apiFixtures.addCommentMock,
        setStatus: vi.fn(),
        addTask: vi.fn(),
        taskHistory: vi.fn(async () => []),
        suggestTasks: vi.fn(async () => []),
        referenceSnippet: vi.fn(async () => ({
            path: 'src/lib.rs',
            highlight_start: 1,
            highlight_end: 1,
            lines: [{ number: 1, text: 'fn demo() {}' }],
            has_more_before: false,
            has_more_after: false,
            total_lines: 1,
        })),
        listTasks: vi.fn(async () => []),
        listProjects: vi.fn(async () => [{ prefix: 'DEMO', name: 'Demo Project' }]),
        showConfig: vi.fn(async () => ({
            issue_states: ['Open', 'Closed'],
            issue_priorities: ['Low', 'Medium', 'High'],
            issue_types: ['bug', 'feature'],
            tags: [],
            custom_fields: [],
            default_project: 'DEMO',
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
        sprintList: vi.fn(async () => ({ status: 'ok', count: 0, sprints: [], missing_sprints: [] })),
    },
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))

vi.mock('../composables/useProjects', () => ({
    useProjects: () => ({
        projects: ref([{ prefix: 'DEMO', name: 'Demo Project' }]),
        refresh: vi.fn(async () => { }),
    }),
}))

vi.mock('../composables/useConfig', () => {
    const refresh = vi.fn(async () => { })
    const defaults = {
        project: 'DEMO',
        status: 'Open',
        priority: 'Medium',
        type: 'bug',
        reporter: '',
        assignee: '',
        tags: [] as string[],
        customFields: {},
    }
    return {
        useConfig: () => ({
            statuses: ref(['Open', 'Closed']),
            priorities: ref(['Low', 'Medium', 'High']),
            types: ref(['bug', 'feature']),
            tags: ref([]),
            customFields: ref([]),
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
    _resetTaskStore()
    apiFixtures.reset()
    try {
        localStorage.removeItem('lotar.preferences.taskPanel.showAttachments')
        localStorage.removeItem('lotar.preferences.taskPanel.showLinksInAttachments')
        localStorage.removeItem('lotar.preferences.taskPanel.autoDetectLinks')
    } catch {
        // ignore
    }
})

afterEach(() => {
    document.body.innerHTML = ''
})

describe('TaskPanel deleted (trash) mode — DEV-92', () => {
    it('does not apply a late restore response over a newer deletion', async () => {
        const store = useTaskStore()
        store.upsert(apiFixtures.clone(apiFixtures.state.task) as any)
        const wrapper = await mountTaskPanel()
        let release!: (task: any) => void
        apiFixtures.restoreTaskMock.mockImplementationOnce(() => new Promise(resolve => { release = resolve }))
        await wrapper.find('[data-testid="task-restore"]').trigger('click')
        const newer = { ...apiFixtures.clone(apiFixtures.state.task), deleted_at: '2026-10-06T00:00:00Z' }
        apiFixtures.state.task = newer
        store.upsert(newer as any)
        release(apiFixtures.clone(apiFixtures.baseTask))
        await flushPromises()
        expect(wrapper.find('fieldset[disabled]').exists()).toBe(true)
        expect(wrapper.emitted('restored')).toBeUndefined()
        expect(store._map.value.get('DEMO-123')?.deleted_at).toBe(newer.deleted_at)
        wrapper.unmount()
    })

    it('loads a deleted task through the trash endpoint and renders read-only', async () => {
        const wrapper = await mountTaskPanel()

        expect(apiFixtures.getTaskMock).toHaveBeenCalledWith('DEMO-123', undefined, { includeDeleted: true })
        expect(wrapper.find('[data-testid="task-deleted-banner"]').exists()).toBe(true)
        expect(wrapper.find('[data-testid="task-panel-deleted-badge"]').exists()).toBe(true)
        // The read-only fieldset disables every nested form control.
        expect(wrapper.find('fieldset[disabled]').exists()).toBe(true)
    })

    it('blocks field edits while the task is deleted', async () => {
        const wrapper = await mountTaskPanel()

        const title = wrapper
            .findAll('input')
            .find((i: any) => (i.element as HTMLInputElement).value === 'Demo task')
        expect(title).toBeTruthy()

        await title!.setValue('Renamed')
        await title!.trigger('blur')
        await flushPromises()

        expect(apiFixtures.updateTaskMock).not.toHaveBeenCalled()
    })

    it('blocks comments while the task is deleted', async () => {
        const wrapper = await mountTaskPanel()

        const commentBox = wrapper.find('textarea')
        expect(commentBox.exists()).toBe(true)
        await commentBox.setValue('should not post')

        const submit = wrapper.find('.task-panel__comment-submit')
        expect(submit.exists()).toBe(true)
        await submit.trigger('click')
        await flushPromises()

        expect(apiFixtures.addCommentMock).not.toHaveBeenCalled()
    })

    it('restores via the API and re-enables editing', async () => {
        const wrapper = await mountTaskPanel()

        const restoreButton = wrapper.find('[data-testid="task-restore"]')
        expect(restoreButton.exists()).toBe(true)
        await restoreButton.trigger('click')
        await flushPromises()
        await nextTick()

        expect(apiFixtures.restoreTaskMock).toHaveBeenCalledWith('DEMO-123', 'DEMO')
        const updated = wrapper.emitted('updated') ?? []
        expect(updated.length).toBeGreaterThan(0)
        expect((updated[updated.length - 1]?.[0] as any).deleted_at).toBeUndefined()
        expect(wrapper.find('[data-testid="task-deleted-banner"]').exists()).toBe(false)
        expect(wrapper.find('fieldset[disabled]').exists()).toBe(false)
    })

    it('reloads the open task when the host signals an external lifecycle change', async () => {
        apiFixtures.reset()
        const active = apiFixtures.clone(apiFixtures.baseTask)
        apiFixtures.getTaskMock.mockImplementation(async () => active)
        const wrapper = await mountTaskPanel()
        expect(wrapper.find('[data-testid="task-deleted-banner"]').exists()).toBe(false)
        const callsAfterMount = apiFixtures.getTaskMock.mock.calls.length

        // External soft deletion: the host bumps the prop; the panel
        // reloads authoritatively and turns read-only.
        const deleted = { ...apiFixtures.clone(apiFixtures.baseTask), deleted_at: '2026-10-05T10:00:00Z' }
        apiFixtures.getTaskMock.mockImplementation(async () => deleted)
        await wrapper.setProps({ lifecycleReload: 1 })
        await flushPromises()
        await nextTick()

        expect(apiFixtures.getTaskMock.mock.calls.length).toBe(callsAfterMount + 1)
        expect(wrapper.find('[data-testid="task-deleted-banner"]').exists()).toBe(true)
        expect(wrapper.find('fieldset[disabled]').exists()).toBe(true)
    })

    it('permanently deletes after an explicit confirmation and surfaces retention warnings', async () => {
        const { showToast } = await import('../components/toast')
        const wrapper = await mountTaskPanel()

        const dangerEntries = wrapper
            .findAll('.task-panel__deleted-banner-actions button')
            .filter((b: any) => (b.text() as string).includes('Delete permanently'))
        expect(dangerEntries.length).toBe(1)
        await dangerEntries[0]!.trigger('click')
        await nextTick()
        await flushPromises()

        // The Teleport stub renders the dialog inline within the wrapper.
        const dialog = wrapper.find('[aria-label="Delete permanently"]')
        expect(dialog.exists()).toBe(true)

        const dangerButton = dialog.find('button[type="submit"]')
        expect((dangerButton.element as HTMLButtonElement).disabled).toBe(true)

        await dialog.find('input[type="checkbox"]').setValue(true)
        await nextTick()
        expect((dangerButton.element as HTMLButtonElement).disabled).toBe(false)

        await dangerButton.trigger('click')
        await flushPromises()
        await flushPromises()
        await nextTick()

        expect(apiFixtures.deleteTaskMock).toHaveBeenCalledWith('DEMO-123', { project: 'DEMO', hard: true })
        expect(vi.mocked(showToast)).toHaveBeenCalledWith('kept blob: @attachments/shared.png')
        expect(wrapper.emitted('close')).toBeTruthy()
    })
})
