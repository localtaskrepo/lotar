import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import TaskPanel from '../components/TaskPanel.vue'
import TaskPanelSource from '../components/TaskPanel.vue?raw'

vi.mock('../api/client', () => {
    const baseTask = {
        id: 'TMP-1',
        title: 'Temp task',
        status: 'Open',
        priority: 'Medium',
        task_type: 'bug',
        reporter: '',
        assignee: '',
        due_date: '',
        effort: '',
        description: '',
        tags: [] as string[],
        relationships: {},
        comments: [] as any[],
        references: [] as any[],
        history: [] as any[],
        custom_fields: {},
    }
    const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))
    return {
        api: {
            whoami: vi.fn().mockResolvedValue('tester'),
            addTask: vi.fn().mockResolvedValue(clone(baseTask)),
            setStatus: vi.fn().mockResolvedValue(clone(baseTask)),
            updateTask: vi.fn().mockResolvedValue(clone(baseTask)),
            getTask: vi.fn().mockResolvedValue(clone(baseTask)),
            taskHistory: vi.fn().mockResolvedValue([]),
            suggestTasks: vi.fn().mockResolvedValue([]),
            listTasks: vi.fn().mockResolvedValue([]),
            listProjects: vi.fn().mockResolvedValue([{ prefix: 'DEMO', name: 'Demo Project' }]),
            projectStats: vi.fn(),
        },
    }
})

vi.mock('../components/toast', () => ({
    showToast: vi.fn(),
}))

vi.mock('../composables/useProjects', () => {
    return {
        useProjects: () => ({
            projects: ref([{ prefix: 'DEMO', name: 'Demo Project' }]),
            refresh: vi.fn(async () => { }),
        }),
    }
})

vi.mock('../composables/useConfig', () => {
    return {
        useConfig: () => ({
            statuses: ref(['Open', 'Closed']),
            priorities: ref(['Low', 'Medium', 'High']),
            types: ref(['bug', 'feature']),
            tags: ref(['alpha', 'beta']),
            customFields: ref(['product']),
            members: ref([] as string[]),
            defaults: ref({
                project: 'DEMO',
                status: 'Open',
                priority: 'Medium',
                type: 'bug',
                reporter: '',
                assignee: '',
                tags: [] as string[],
                customFields: { product: '' },
            }),
            refresh: vi.fn(async () => { }),
        }),
    }
})

vi.mock('../composables/useReferencePreview', () => {
    return {
        useReferencePreview: () => ({
            hoveredReferenceCode: ref(null),
            hoveredReferenceStyle: ref(null),
            hoveredReferenceLoading: ref(false),
            hoveredReferenceError: ref(null),
            hoveredReferenceSnippet: ref(null),
            hoveredReferenceCanExpand: ref(false),
            hoveredReferenceCanExpandBefore: ref(false),
            hoveredReferenceCanExpandAfter: ref(false),
            onReferenceEnter: vi.fn(),
            onReferenceLeave: vi.fn(),
            onReferencePreviewEnter: vi.fn(),
            onReferencePreviewLeave: vi.fn(),
            expandReferenceSnippet: vi.fn(),
            isReferenceLineHighlighted: vi.fn(),
            setReferencePreviewElement: vi.fn(),
            resetReferencePreviews: vi.fn(),
        }),
    }
})

vi.mock('../composables/useTaskPanelOwnership', () => {
    return {
        useTaskPanelOwnership: () => ({
            whoami: ref('tester'),
            reporterMode: ref('select'),
            assigneeMode: ref('select'),
            reporterCustom: ref(''),
            assigneeCustom: ref(''),
            orderedKnownUsers: ref(['tester']),
            reporterSelection: ref(''),
            assigneeSelection: ref(''),
            setReporterSelection: vi.fn(),
            setAssigneeSelection: vi.fn(),
            setReporterCustom: vi.fn(),
            setAssigneeCustom: vi.fn(),
            commitReporterCustom: vi.fn(),
            commitAssigneeCustom: vi.fn(),
            resetReporterSelection: vi.fn(),
            resetAssigneeSelection: vi.fn(),
            preloadPeople: vi.fn(async () => { }),
            resetOwnership: vi.fn(),
            syncOwnershipControls: vi.fn(),
        }),
    }
})

vi.mock('../composables/useTaskRelationships', () => {
    return {
        useTaskRelationships: () => ({
            relationDefs: ref([] as any[]),
            relationships: ref({ depends_on: [] as string[] }),
            relationSuggestions: ref([] as any[]),
            relationActiveIndex: ref(-1),
            resetRelationships: vi.fn(),
            buildRelationships: vi.fn(() => ({})),
            snapshotRelationshipsBaselineFromTask: vi.fn(),
            snapshotRelationshipsBaselineFromInputs: vi.fn(),
            applyRelationshipsFromTask: vi.fn(),
            updateRelationshipField: vi.fn(),
            handleRelationshipBlur: vi.fn(),
            onRelationInput: vi.fn(),
            onRelationKey: vi.fn(),
            pickRelation: vi.fn(),
            commitRelationships: vi.fn(),
        }),
    }
})

vi.mock('../composables/useTaskComments', () => {
    return {
        useTaskComments: () => ({
            newComment: ref(''),
            editingCommentIndex: ref<number | null>(null),
            editingCommentText: ref(''),
            editingCommentSubmitting: ref(false),
            setEditingCommentTextarea: vi.fn(),
            updateNewComment: vi.fn(),
            updateEditingCommentText: vi.fn(),
            addComment: vi.fn(),
            startEditComment: vi.fn(),
            cancelEditComment: vi.fn(),
            saveCommentEdit: vi.fn(),
            resetComments: vi.fn(),
        }),
    }
})

const componentStubs = {
    Teleport: true,
    TaskPanelSummarySection: { template: '<div class="stub-summary"></div>' },
    TaskPanelOwnershipSection: { template: '<div class="stub-ownership"></div>' },
    TaskPanelCustomFieldsSection: { template: '<div class="stub-custom-fields"></div>' },
    TaskPanelCommentsTab: { template: '<div class="stub-comments"></div>' },
    TaskPanelRelationshipsTab: { template: '<div class="stub-relationships"></div>' },
    TaskPanelHistoryTab: { template: '<div class="stub-history"></div>' },
    TaskPanelCommitsTab: { template: '<div class="stub-commits"></div>' },
    TaskPanelReferencesTab: { template: '<div class="stub-references"></div>' },
    TaskPanelTagEditor: { template: '<div class="stub-tag-editor"></div>' },
    UiButton: { template: '<button type="button"><slot /></button>' },
    UiLoader: { template: '<div class="stub-loader"><slot /></div>' },
}

function extractStyle(source: string): string {
    const match = source.match(/<style[^>]*>([\s\S]*?)<\/style>/)
    return match ? match[1] : ''
}

describe('TaskPanel layout styles', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    afterEach(() => {
        document.body.innerHTML = ''
    })

    it('applies overlay and panel styling when open', async () => {
        const wrapper = mount(TaskPanel, {
            attachTo: document.body,
            props: {
                open: true,
                initialProject: 'DEMO',
            },
            global: {
                stubs: componentStubs,
            },
        })

        await flushPromises()
        await flushPromises()

        const overlay = document.querySelector('.task-panel__overlay') as HTMLElement | null
        expect(overlay).not.toBeNull()
        if (!overlay) {
            wrapper.unmount()
            return
        }
        const panel = overlay.querySelector('.task-panel') as HTMLElement | null
        expect(panel).not.toBeNull()

        const styleContent = extractStyle(TaskPanelSource)
        expect(styleContent).toMatch(/\.task-panel__overlay[^{}]*\{[^}]*position:\s*fixed/)
        expect(styleContent).toMatch(/\.task-panel__overlay[^{}]*\{[^}]*display:\s*flex/)
        expect(styleContent).toMatch(/\.task-panel__overlay[^{}]*\{[^}]*justify-content:\s*flex-end/)
        expect(styleContent).toMatch(/\.task-panel__overlay[^{}]*\{[^}]*background:\s*rgba\(15,\s*15,\s*15,\s*0\.55\)/)

        expect(styleContent).toMatch(/\.task-panel[^{}]*\{[^}]*display:\s*flex/)
        expect(styleContent).toMatch(/\.task-panel[^{}]*\{[^}]*flex-direction:\s*column/)

        expect(styleContent).toMatch(/\.task-panel__body[^{}]*\{[^}]*overflow-y:\s*auto/)
        expect(styleContent).toMatch(/\.task-panel__body[^{}]*\{[^}]*padding:/)

        wrapper.unmount()
    })
})
