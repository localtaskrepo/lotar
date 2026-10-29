import { formatProjectLabel } from '../utils/projectLabels'
import { computed, onScopeDispose, reactive, ref, watch } from 'vue'
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
import {
    completionStatusKey,
    embeddedDoneStatesOf,
    ensureCompletionPolicy,
    useCompletionPolicy,
} from './useCompletionPolicy'
import { useProjects } from './useProjects'
import { useReferencePreview } from './useReferencePreview'
import { useSprints } from './useSprints'
import { useTaskPanelOwnership } from './useTaskPanelOwnership'
import { useTaskRelationships } from './useTaskRelationships'
import { relationshipMutation } from '../utils/relationships'

export interface TaskPanelProps {
    open: boolean
    taskId?: string | null
    initialProject?: string | null
    initialStatus?: string | null
    initialDueDate?: string | null
    /** DEV-104: concrete sprint id prefilled into a create-mode draft. */
    initialSprint?: number | null
}

export interface TaskPanelEmit {
    (event: 'close'): void
    (event: 'created', task: TaskDTO): void
    (event: 'updated', task: TaskDTO): void
    /** DEV-183: the requested ID belongs to a moved task, now under `id`. */
    (event: 'redirected', id: string): void
}

type ActivityTab = 'comments' | 'relationships' | 'history' | 'commits' | 'references'

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
        attachmentsDir,
        members,
        refresh: refreshConfigResource,
    } = useConfig({ throwOnError: true })

    const panelGeneration = ref(0)
    const loadedConfigScope = ref<string | null>(null)
    let configGeneration = 0
    async function refreshConfig(project: string) {
        const request = ++configGeneration
        loadedConfigScope.value = null
        await refreshConfigResource(project)
        if (request === configGeneration) loadedConfigScope.value = project
    }

    const statusOptions = computed(() => statuses.value ?? [])
    const priorityOptions = computed(() => priorities.value ?? [])
    const typeOptions = computed(() => types.value ?? [])

    const allowNewCustomFields = computed(() =>
        Array.isArray(configCustomFields.value)
            ? configCustomFields.value.some((value) => (value || '').trim() === '*')
            : false,
    )

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
    const canCreate = computed(
        () =>
            props.open &&
            mode.value === 'create' &&
            ready.value &&
            !loading.value &&
            !submitting.value &&
            !!form.project.trim() &&
            loadedConfigScope.value === form.project,
    )

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

    const loadCommitHistory = (id: string, limit?: number) => loadCommitHistoryFromActivity(id, limit)

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

    // DEV-21: terminal statuses resolved per project; custom terminal names
    // (e.g. "Shipped") render with the success badge instead of a name guess.
    // Policy fetches follow the panel's confirmed config scope (never a
    // stale initial project while a task fetch is still pending).
    const completion = useCompletionPolicy()
    watch(
        loadedConfigScope,
        (scope) => {
            const prefix = (scope || '').trim()
            if (prefix && prefix === form.project) void ensureCompletionPolicy(prefix)
        },
        { immediate: true },
    )

    const statusBadgeClass = computed(() => {
        const status = (form.status || '').toLowerCase()
        // DEV-21 precedence: the loaded task's EMBEDDED actual-root done
        // states decide terminality first (homonymous prefixes across roots
        // can differ; no green "Done" substring guess once embedded states
        // are provided); then the scoped cached policy. Missing policy keeps
        // the badge neutral rather than guessing completion from its name.
        const embedded = embeddedDoneStatesOf(task)
        if (embedded) {
            if (embedded.has(completionStatusKey(form.status))) return 'badge--success'
            if (status.includes('progress')) return 'badge--info'
            if (status.includes('block')) return 'badge--danger'
            return 'badge--muted'
        }
        if (completion.isDoneStatus(form.project, form.status)) return 'badge--success'
        if (status.includes('progress')) return 'badge--info'
        if (status.includes('block')) return 'badge--danger'
        return 'badge--muted'
    })

    let relationshipRead = 0
    const { relationships, resetRelationships, buildRelationships, applyRelationshipsFromTask, mutateRelationship } =
        useTaskRelationships({
            mode,
            ready,
            suppressWatch,
            scope: () =>
                `${props.open}:${props.taskId}:${task.deleted_at || ''}:${task.archived ? 'a' : ''}:${panelGeneration.value}`,
            taskId: () => task.id || '',
            editable: () => props.open && !task.deleted_at && !task.archived,
            execute: async (intent) => {
                await api.relationshipMutate(relationshipMutation(task.id, intent))
            },
            reconcile: async () => {
                const id = task.id,
                    request = panelGeneration.value,
                    read = ++relationshipRead
                const updated = await api.getTask(id, undefined, { includeDeleted: true })
                if (
                    !props.open ||
                    props.taskId !== id ||
                    panelGeneration.value !== request ||
                    read !== relationshipRead
                )
                    return
                task.relationships = updated.relationships
                task.history = updated.history
                task.modified = updated.modified
                if (updated.deleted_at) task.deleted_at = updated.deleted_at
                emit('updated', updated)
                return updated
            },
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
        members,
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
        ensureProjectsLoaded,
        onProjectChange: onProjectChangeFromLifecycle,
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

    const cleanup = () => {
        panelGeneration.value += 1
        configGeneration += 1
        loadedConfigScope.value = null
        ready.value = false
        submitting.value = false
        lifecycleCleanup()
    }
    onScopeDispose(cleanup)

    function onProjectChange() {
        if (mode.value !== 'create') return
        panelGeneration.value += 1
        submitting.value = false
        onProjectChangeFromLifecycle()
    }

    bindCommentApplyTask(applyTask)

    async function initialize() {
        const request = ++panelGeneration.value
        const taskId = props.taskId
        const current = () => props.open && request === panelGeneration.value
        ready.value = false
        submitting.value = false
        loading.value = mode.value === 'edit'
        resetErrors()
        resetForm()
        try {
            await ensureProjectsLoaded()
            if (!current()) return
            if (mode.value === 'create') {
                const scopeProject = props.initialProject || defaults.value.project || projects.value[0]?.prefix || ''
                form.project = scopeProject
                await refreshConfig(scopeProject)
                if (!current() || form.project !== scopeProject || loadedConfigScope.value !== scopeProject) return
                applyDefaults(scopeProject)
                if (props.initialStatus && (statuses.value || []).includes(props.initialStatus)) {
                    form.status = props.initialStatus
                }
                if (props.initialDueDate) {
                    form.due_date = props.initialDueDate
                }
                // DEV-104: prefill the draft's sprint destination after
                // applyDefaults (which resets sprint state). The id is kept
                // verbatim — no sprint-metadata cross-check — so a failed or
                // stale metadata load can never silently downgrade the
                // destination to the backlog; the atomic create payload is
                // validated server-side.
                const initialSprintId = props.initialSprint
                if (
                    typeof initialSprintId === 'number' &&
                    Number.isInteger(initialSprintId) &&
                    initialSprintId > 0 &&
                    !form.sprints.includes(initialSprintId)
                ) {
                    form.sprints = [...form.sprints, initialSprintId]
                }
                ready.value = true
                loading.value = false
            } else if (taskId) {
                await loadTask(taskId)
            } else {
                ready.value = true
                loading.value = false
            }
        } catch (error: unknown) {
            if (!current()) return
            errors.config = error instanceof Error ? error.message : 'Failed to initialize task'
            loading.value = false
        }
    }

    function handleFieldBlur(field: string) {
        onFieldBlur(field)
    }

    function projectLabel(project: { prefix: string; name?: string | null }) {
        return formatProjectLabel(project)
    }

    function closePanel() {
        cleanup()
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
        panelGeneration,
        getTaskId: () => props.taskId,
        canCreate,
        mode,
        task,
        form,
        ready,
        suppressWatch,
        submitting,
        loading,
        apiClient: {
            addTask: api.addTask,
            updateTask: api.updateTask,
            // DEV-92: the panel can open trash rows — always request the
            // deleted task too; `include_deleted` is a strict widening flag
            // for active tasks, so this is safe for every load.
            getTask: (id: string) => api.getTask(id, undefined, { includeDeleted: true }),
        },
        showToast,
        onRedirect: (id: string) => emit('redirected', id),
        buildRelationships,
        buildCustomFields,
        applyTask,
        applyTaskCustomFields,
        applyRelationshipsFromTask,
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
        applyPatch,
        projects,
        refreshProjects,
        statuses,
        priorities,
        types,
        configTags,
        defaults,
        attachmentsDir,
        refreshConfig,
        statusOptions,
        priorityOptions,
        typeOptions,
        loading,
        submitting,
        ready,
        canCreate,
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
        allowNewCustomFields,
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
        relationships,
        mutateRelationship,
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
        invalidateTaskRequests: () => {
            panelGeneration.value += 1
        },
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
