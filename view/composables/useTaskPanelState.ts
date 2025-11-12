import { computed, reactive, ref } from 'vue'
import { api } from '../api/client'
import type { TaskDTO } from '../api/types'
import { showToast } from '../components/toast'
import { useTaskPanelActivity } from './task-panel/useTaskPanelActivity'
import { useTaskPanelComments } from './task-panel/useTaskPanelComments'
import { useTaskPanelCustomFields } from './task-panel/useTaskPanelCustomFields'
import { useTaskPanelFormLifecycle } from './task-panel/useTaskPanelFormLifecycle'
import { useTaskPanelHotkeys } from './task-panel/useTaskPanelHotkeys'
import { useTaskPanelPersistence, type TaskPanelFormState } from './task-panel/useTaskPanelPersistence'
import { useTaskPanelSprints } from './task-panel/useTaskPanelSprints'
import { useTaskPanelTags } from './task-panel/useTaskPanelTags'
import { useTaskPanelWatchers } from './task-panel/useTaskPanelWatchers'
import { useConfig } from './useConfig'
import { useProjects } from './useProjects'
import { useReferencePreview } from './useReferencePreview'
import { useSprints } from './useSprints'
import { useTaskPanelOwnership } from './useTaskPanelOwnership'
import { useTaskRelationships } from './useTaskRelationships'

export interface TaskPanelProps {
    open: boolean
    taskId?: string | null
    initialProject?: string | null
}

export interface TaskPanelEmit {
    (event: 'close'): void
    (event: 'created', task: TaskDTO): void
    (event: 'updated', task: TaskDTO): void
}

type ActivityTab = 'comments' | 'relationships' | 'history' | 'commits' | 'references'

interface CommitEntry {
    commit: string
    author: string
    email: string
    date: string
    message: string
}

type SprintAssignmentSummary = {
    id: number
    label: string
    state: string
    missing: boolean
}

export function useTaskPanelState(props: Readonly<TaskPanelProps>, emit: TaskPanelEmit) {
    const mode = computed(() => (props.taskId && props.taskId !== 'new' ? 'edit' : 'create'))

    const { projects, refresh: refreshProjects } = useProjects()
    const {
        sprints: sprintList,
        loading: sprintsLoading,
        missingSprints,
        active: activeSprints,
        refresh: refreshSprints,
    } = useSprints()
    const {
        statuses,
        priorities,
        types,
        tags: configTags,
        customFields: configCustomFields,
        defaults,
        refresh: refreshConfig,
    } = useConfig()

    const statusOptions = computed(() => statuses.value ?? [])
    const priorityOptions = computed(() => priorities.value ?? [])
    const typeOptions = computed(() => types.value ?? [])

    const loading = ref(false)
    const submitting = ref(false)
    const ready = ref(false)
    const suppressWatch = ref(false)
    const task = reactive<TaskDTO>({} as TaskDTO)

    const form = reactive<TaskPanelFormState>({
        id: '',
        title: '',
        project: '',
        status: '',
        priority: '',
        task_type: '',
        reporter: '',
        assignee: '',
        due_date: '',
        effort: '',
        description: '',
        tags: [] as string[],
        sprints: [] as number[],
    })

    const {
        hoveredReferenceCode,
        hoveredReferenceStyle,
        hoveredReferenceLoading,
        hoveredReferenceError,
        hoveredReferenceSnippet,
        hoveredReferenceCanExpand,
        hoveredReferenceCanExpandBefore,
        hoveredReferenceCanExpandAfter,
        onReferenceEnter,
        onReferenceLeave,
        onReferencePreviewEnter,
        onReferencePreviewLeave,
        expandReferenceSnippet,
        isReferenceLineHighlighted,
        setReferencePreviewElement,
        resetReferencePreviews,
    } = useReferencePreview()

    const errors = reactive<Record<string, string>>({})

    const {
        newComment,
        editingCommentIndex,
        editingCommentText,
        editingCommentSubmitting,
        setEditingCommentTextarea,
        updateNewComment,
        updateEditingCommentText,
        addComment,
        startEditComment,
        cancelEditComment,
        saveCommentEdit,
        resetComments,
        bindApplyTask: bindCommentApplyTask,
    } = useTaskPanelComments({
        mode,
        task,
        submitting,
        emitUpdated: (updated: TaskDTO) => emit('updated', updated),
    })

    let applyPatchImpl: (patch: Record<string, unknown>) => Promise<void> = async () => {
        // Placeholder replaced once persistence initializes
    }
    const applyPatch = async (patch: Record<string, unknown>) => {
        await applyPatchImpl(patch)
    }

    let updateFieldImpl: (field: string) => Promise<void> = async () => {
        // Placeholder replaced once persistence initializes
    }
    const updateField = async (field: string) => {
        await updateFieldImpl(field)
    }

    let onFieldBlurImpl: (field: string) => Promise<void> = async () => {
        // Placeholder replaced once persistence initializes
    }
    const onFieldBlur = async (field: string) => {
        await onFieldBlurImpl(field)
    }

    let updateStatusImpl: (status: string) => Promise<void> = async () => {
        // Placeholder replaced once persistence initializes
    }
    const updateStatus = async (status: string) => {
        await updateStatusImpl(status)
    }

    let reloadTaskImpl: () => Promise<TaskDTO | undefined> = async () => {
        // Placeholder replaced once persistence initializes
        return undefined
    }
    const reloadTask = async () => reloadTaskImpl()

    let loadTaskImpl: (id: string) => Promise<TaskDTO | undefined> = async () => {
        // Placeholder replaced once persistence initializes
        return undefined
    }
    const loadTask = async (id: string) => loadTaskImpl(id)

    let handleSubmitImpl: () => Promise<void> = async () => {
        // Placeholder replaced once persistence initializes
    }
    const handleSubmit = async () => {
        await handleSubmitImpl()
    }
    const {
        knownTags,
        allowCustomTags,
        configTagOptions,
        mergeKnownTags,
        setTags,
        startWatchers: startTagWatchers,
    } = useTaskPanelTags({
        form,
        mode,
        ready,
        suppressWatch,
        configTags,
        applyPatch,
    })

    const {
        customFields,
        customFieldKeys,
        newField,
        buildCustomFields,
        commitCustomFields,
        addField,
        removeField,
        resetCustomFields,
        ensureConfiguredCustomFields,
        applyDefaultsFromConfig,
        applyTaskCustomFields,
        updateCustomFieldKey,
        updateCustomFieldValue,
        updateNewFieldKey,
        updateNewFieldValue,
    } = useTaskPanelCustomFields({
        mode,
        ready,
        suppressWatch,
        configCustomFields,
        applyPatch,
    })

    const {
        sprintLookup,
        assignedSprints,
        hasAssignedSprints,
        assignedSprintNotice,
        sprintOptions,
        hasSprints,
        resetSprintsState,
        applyTaskSprints,
    } = useTaskPanelSprints({
        form,
        sprintList,
        missingSprints,
        activeSprints,
    })

    const {
        changeLog,
        commitHistory,
        commitsLoading,
        syncFromTaskHistory,
        resetActivity,
        loadCommitHistory: loadCommitHistoryFromActivity,
        refreshCommits: refreshCommitsFromActivity,
        formatDate,
        formatCommit,
        formatFieldName,
        formatChangeValue,
    } = useTaskPanelActivity()

    const loadCommitHistory = (id: string, limit?: number) =>
        loadCommitHistoryFromActivity(id, limit)

    const refreshCommits = async () => {
        if (!task.id) return
        await refreshCommitsFromActivity(task.id)
    }

    const activityTabs: Array<{ id: ActivityTab; label: string }> = [
        { id: 'comments', label: 'Comments' },
        { id: 'relationships', label: 'Relationships' },
        { id: 'history', label: 'History' },
        { id: 'commits', label: 'Commits' },
        { id: 'references', label: 'References' },
    ]
    const activityTab = ref<ActivityTab>('comments')

    const statusBadgeClass = computed(() => {
        const status = (form.status || '').toLowerCase()
        if (status.includes('done')) return 'badge--success'
        if (status.includes('progress')) return 'badge--info'
        if (status.includes('block')) return 'badge--danger'
        return 'badge--muted'
    })

    let projectForSuggestionsImpl = () => {
        if (form.project) return form.project
        if (form.id) return form.id.split('-')[0]
        if (props.initialProject) return props.initialProject
        return defaults.value.project || projects.value[0]?.prefix || ''
    }

    const projectForSuggestions = () => projectForSuggestionsImpl()

    const {
        relationDefs,
        relationships,
        relationSuggestions,
        relationActiveIndex,
        resetRelationships,
        buildRelationships,
        snapshotRelationshipsBaselineFromTask,
        snapshotRelationshipsBaselineFromInputs,
        applyRelationshipsFromTask,
        updateRelationshipField: setRelationshipValue,
        handleRelationshipBlur,
        onRelationInput,
        onRelationKey,
        pickRelation,
        commitRelationships,
    } = useTaskRelationships({
        mode,
        ready,
        suppressWatch,
        applyPatch,
        projectForSuggestions,
        suggestTasks: api.suggestTasks,
    })

    const {
        whoami,
        reporterMode,
        assigneeMode,
        reporterCustom,
        assigneeCustom,
        orderedKnownUsers,
        reporterSelection,
        assigneeSelection,
        setReporterSelection,
        setAssigneeSelection,
        setReporterCustom,
        setAssigneeCustom,
        commitReporterCustom,
        commitAssigneeCustom,
        resetReporterSelection,
        resetAssigneeSelection,
        preloadPeople,
        resetOwnership,
        syncOwnershipControls,
    } = useTaskPanelOwnership({
        form,
        mode,
        ready,
        suppressWatch,
        defaults,
        mergeKnownTags,
        updateField: (field) => updateField(field),
        onFieldBlur: (field) => handleFieldBlur(field),
    })

    const {
        resetErrors,
        resetForm,
        cleanup: lifecycleCleanup,
        validate,
        applyDefaults,
        applyTask,
        projectForSuggestions: projectForSuggestionsFromLifecycle,
        ensureProjectsLoaded,
        onProjectChange,
    } = useTaskPanelFormLifecycle({
        mode,
        ready,
        loading,
        submitting,
        suppressWatch,
        form,
        task,
        errors,
        defaults,
        statuses,
        priorities,
        types,
        projects,
        activityTab,
        getInitialProject: () => props.initialProject ?? null,
        resetSprintsState,
        mergeKnownTags,
        resetCustomFields,
        ensureConfiguredCustomFields,
        applyDefaultsFromConfig,
        syncOwnershipControls,
        resetOwnership,
        preloadPeople,
        resetReferencePreviews,
        resetRelationships,
        snapshotRelationshipsBaselineFromInputs,
        snapshotRelationshipsBaselineFromTask,
        applyRelationshipsFromTask,
        applyTaskCustomFields,
        applyTaskSprints,
        syncFromTaskHistory,
        resetActivity,
        resetComments,
        cancelEditComment,
        refreshProjects,
        refreshConfig,
    })

    const cleanup = lifecycleCleanup

    projectForSuggestionsImpl = projectForSuggestionsFromLifecycle

    bindCommentApplyTask(applyTask)

    async function initialize() {
        ready.value = false
        submitting.value = false
        loading.value = mode.value === 'edit'
        resetErrors()
        resetForm()
        if (mode.value === 'create') {
            const scopeProject = props.initialProject || defaults.value.project || ''
            await refreshConfig(scopeProject)
            applyDefaults(scopeProject)
            loading.value = false
            ready.value = true
        } else if (props.taskId) {
            await loadTask(props.taskId)
            ready.value = true
        } else {
            loading.value = false
            ready.value = true
        }
    }

    function updateRelationshipField(field: string, value: string) {
        setRelationshipValue(field, value)
    }

    function handleFieldBlur(field: string) {
        onFieldBlur(field)
    }

    function projectLabel(project: { prefix: string; name?: string | null }) {
        const prefix = project?.prefix || ''
        const name = (project?.name || '').trim()
        if (name && name !== prefix) {
            return `${name} (${prefix})`
        }
        return prefix
    }

    function closePanel() {
        emit('close')
    }

    const {
        handleSubmit: handleSubmitFromPersistence,
        onFieldBlur: onFieldBlurFromPersistence,
        updateField: updateFieldFromPersistence,
        applyPatch: applyPatchFromPersistence,
        updateStatus: updateStatusFromPersistence,
        reloadTask: reloadTaskFromPersistence,
        loadTask: loadTaskFromPersistence,
    } = useTaskPanelPersistence({
        mode,
        task,
        form,
        ready,
        suppressWatch,
        submitting,
        loading,
        apiClient: {
            addTask: api.addTask,
            setStatus: api.setStatus,
            updateTask: api.updateTask,
            getTask: api.getTask,
        },
        showToast,
        buildRelationships,
        buildCustomFields,
        applyTask,
        validate,
        closePanel,
        resetActivity,
        refreshConfig,
        loadCommitHistory,
        emit,
    })

    handleSubmitImpl = handleSubmitFromPersistence
    onFieldBlurImpl = onFieldBlurFromPersistence
    updateFieldImpl = updateFieldFromPersistence
    applyPatchImpl = applyPatchFromPersistence
    updateStatusImpl = updateStatusFromPersistence
    reloadTaskImpl = reloadTaskFromPersistence
    loadTaskImpl = loadTaskFromPersistence

    useTaskPanelHotkeys({
        isOpen: () => props.open,
        onClose: closePanel,
    })

    startTagWatchers()

    useTaskPanelWatchers({
        open: () => props.open,
        taskId: () => props.taskId ?? null,
        mode,
        ready,
        suppressWatch,
        form,
        preloadPeople,
        ensureProjectsLoaded,
        initialize: async () => {
            if (!props.open) {
                cleanup()
                return
            }
            await initialize()
        },
        onClose: cleanup,
    })

    return {
        mode,
        projects,
        refreshProjects,
        statuses,
        priorities,
        types,
        configTags,
        defaults,
        refreshConfig,
        statusOptions,
        priorityOptions,
        typeOptions,
        loading,
        submitting,
        ready,
        suppressWatch,
        task,
        form,
        hoveredReferenceCode,
        hoveredReferenceStyle,
        hoveredReferenceLoading,
        hoveredReferenceError,
        hoveredReferenceSnippet,
        hoveredReferenceCanExpand,
        hoveredReferenceCanExpandBefore,
        hoveredReferenceCanExpandAfter,
        onReferenceEnter,
        onReferenceLeave,
        onReferencePreviewEnter,
        onReferencePreviewLeave,
        expandReferenceSnippet,
        isReferenceLineHighlighted,
        setReferencePreviewElement,
        resetReferencePreviews,
        errors,
        knownTags,
        allowCustomTags,
        configTagOptions,
        customFields,
        customFieldKeys,
        newField,
        activityTabs,
        activityTab,
        changeLog,
        commitHistory,
        commitsLoading,
        sprintsLoading,
        assignedSprints,
        hasAssignedSprints,
        assignedSprintNotice,
        sprintOptions,
        hasSprints,
        statusBadgeClass,
        relationDefs,
        relationships,
        relationSuggestions,
        relationActiveIndex,
        handleRelationshipBlur,
        onRelationInput,
        onRelationKey,
        pickRelation,
        updateRelationshipField,
        mergeKnownTags,
        projectLabel,
        buildCustomFields,
        commitCustomFields,
        addField,
        removeField,
        resetCustomFields,
        sprintLookup,
        refreshSprints,
        setTags,
        updateCustomFieldKey,
        updateCustomFieldValue,
        updateNewFieldKey,
        updateNewFieldValue,
        handleFieldBlur,
        onFieldBlur,
        closePanel,
        handleSubmit,
        updateStatus,
        reloadTask,
        formatDate,
        formatCommit,
        formatFieldName,
        formatChangeValue,
        onProjectChange,
        ensureProjectsLoaded,
        initialize,
        applyDefaults,
        applyTask,
        loadTask,
        loadCommitHistory,
        refreshCommits,
        newComment,
        editingCommentIndex,
        editingCommentText,
        editingCommentSubmitting,
        setEditingCommentTextarea,
        updateNewComment,
        updateEditingCommentText,
        addComment,
        startEditComment,
        cancelEditComment,
        saveCommentEdit,
        reporterMode,
        assigneeMode,
        reporterCustom,
        assigneeCustom,
        orderedKnownUsers,
        reporterSelection,
        assigneeSelection,
        setReporterSelection,
        setAssigneeSelection,
        setReporterCustom,
        setAssigneeCustom,
        commitReporterCustom,
        commitAssigneeCustom,
        resetReporterSelection,
        resetAssigneeSelection,
        preloadPeople,
        resetOwnership,
        syncOwnershipControls,
        whoami,
    }
}
