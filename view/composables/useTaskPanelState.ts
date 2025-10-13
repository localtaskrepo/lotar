import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { api } from '../api/client'
import type { TaskDTO, TaskHistoryEntry } from '../api/types'
import { showToast } from '../components/toast'
import { useConfig } from './useConfig'
import { useProjects } from './useProjects'
import { useReferencePreview } from './useReferencePreview'
import { useTaskComments } from './useTaskComments'
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

const CATEGORY_CUSTOM_FIELD_KEY = 'category'

export function useTaskPanelState(props: Readonly<TaskPanelProps>, emit: TaskPanelEmit) {
    const mode = computed(() => (props.taskId && props.taskId !== 'new' ? 'edit' : 'create'))

    const { projects, refresh: refreshProjects } = useProjects()
    const { statuses, priorities, types, tags: configTags, defaults, refresh: refreshConfig } = useConfig()

    const statusOptions = computed(() => statuses.value ?? [])
    const priorityOptions = computed(() => priorities.value ?? [])
    const typeOptions = computed(() => types.value ?? [])

    const loading = ref(false)
    const submitting = ref(false)
    const ready = ref(false)
    const suppressWatch = ref(false)
    const task = reactive<TaskDTO>({} as TaskDTO)

    const form = reactive({
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
    const knownTags = ref<string[]>([])
    const allowCustomTags = computed(() => (configTags.value || []).includes('*'))
    const configTagOptions = computed(() => {
        const map = new Map<string, string>()
        for (const tag of (configTags.value || []).filter((value: string) => value !== '*')) {
            const trimmed = tag.trim()
            if (!trimmed) continue
            const key = trimmed.toLowerCase()
            if (!map.has(key)) {
                map.set(key, trimmed)
            }
        }
        return Array.from(map.values())
    })

    const customFields = reactive<Record<string, string>>({})
    const customFieldKeys = reactive<Record<string, string>>({})
    const newField = reactive({ key: '', value: '' })

    const activityTabs: Array<{ id: ActivityTab; label: string }> = [
        { id: 'comments', label: 'Comments' },
        { id: 'relationships', label: 'Relationships' },
        { id: 'history', label: 'History' },
        { id: 'commits', label: 'Commits' },
        { id: 'references', label: 'References' },
    ]
    const activityTab = ref<ActivityTab>('comments')

    const changeLog = ref<TaskHistoryEntry[]>([])
    const commitHistory = ref<CommitEntry[]>([])
    const commitsLoading = ref(false)

    const statusBadgeClass = computed(() => {
        const status = (form.status || '').toLowerCase()
        if (status.includes('done')) return 'badge--success'
        if (status.includes('progress')) return 'badge--info'
        if (status.includes('block')) return 'badge--danger'
        return 'badge--muted'
    })

    const projectForSuggestions = () => {
        if (form.project) return form.project
        if (form.id) return form.id.split('-')[0]
        if (props.initialProject) return props.initialProject
        return defaults.value.project || projectFromList()
    }

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

    let resetCommentsRef: () => void = () => { }
    let cancelEditCommentRef: () => void = () => { }

    function mergeKnownTags(tags: Array<string | null | undefined>) {
        if (!tags?.length) return
        const map = new Map<string, string>()
        knownTags.value.forEach((tag) => {
            const trimmed = tag.trim()
            if (!trimmed) return
            map.set(trimmed.toLowerCase(), trimmed)
        })
        tags.forEach((raw) => {
            if (!raw) return
            const trimmed = raw.trim()
            if (!trimmed) return
            const key = trimmed.toLowerCase()
            if (!map.has(key)) {
                map.set(key, trimmed)
            }
        })
        knownTags.value = Array.from(map.values()).sort((a, b) => a.localeCompare(b))
    }

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

    function setTags(tags: string[]) {
        form.tags = [...tags]
        mergeKnownTags(tags)
    }

    function updateCustomFieldKey(key: string, value: string) {
        customFieldKeys[key] = value
    }

    function updateCustomFieldValue(key: string, value: string) {
        customFields[key] = value
    }

    function updateNewFieldKey(value: string) {
        newField.key = value
    }

    function updateNewFieldValue(value: string) {
        newField.value = value
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

    function buildCustomFields() {
        const out: Record<string, string> = {}
        Object.entries(customFields).forEach(([key, value]) => {
            const target = customFieldKeys[key] || key
            if (target) {
                out[target] = value
            }
        })
        return out
    }

    async function commitCustomFields() {
        if (mode.value !== 'edit' || !ready.value || suppressWatch.value) return
        await applyPatch({ custom_fields: buildCustomFields() })
    }

    function addField() {
        const key = newField.key.trim()
        if (!key || customFields[key] !== undefined) return
        customFields[key] = newField.value
        customFieldKeys[key] = key
        newField.key = ''
        newField.value = ''
        commitCustomFields()
    }

    function removeField(key: string) {
        delete customFields[key]
        delete customFieldKeys[key]
        commitCustomFields()
    }

    function resetCustomFields() {
        Object.keys(customFields).forEach((key) => delete customFields[key])
        Object.keys(customFieldKeys).forEach((key) => delete customFieldKeys[key])
        newField.key = ''
        newField.value = ''
    }

    function seedCategoryField(value?: string | null) {
        const trimmed = typeof value === 'string' ? value.trim() : ''
        if (!trimmed) return
        const existingKey = Object.keys(customFields).find((key) => key.toLowerCase() === CATEGORY_CUSTOM_FIELD_KEY)
        if (existingKey) {
            if (!customFieldKeys[existingKey]) {
                customFieldKeys[existingKey] = existingKey
            }
            if (!customFields[existingKey]) {
                customFields[existingKey] = trimmed
            }
            return
        }
        customFields[CATEGORY_CUSTOM_FIELD_KEY] = trimmed
        customFieldKeys[CATEGORY_CUSTOM_FIELD_KEY] = CATEGORY_CUSTOM_FIELD_KEY
    }

    function projectFromList() {
        return projects.value[0]?.prefix || ''
    }

    function resetErrors() {
        Object.keys(errors).forEach((key) => delete errors[key])
    }

    function closePanel() {
        emit('close')
    }

    function cleanup() {
        resetErrors()
        resetForm()
        resetCommentsRef()
        changeLog.value = []
        commitHistory.value = []
    }

    function resetForm() {
        suppressWatch.value = true
        form.id = ''
        form.title = ''
        form.project = props.initialProject || ''
        form.status = ''
        form.priority = ''
        form.task_type = ''
        form.reporter = ''
        form.assignee = ''
        form.due_date = ''
        form.effort = ''
        form.description = ''
        form.tags = []
        resetOwnership()
        activityTab.value = 'comments'
        resetCustomFields()
        resetRelationships()
        resetReferencePreviews()
        resetCommentsRef()
        snapshotRelationshipsBaselineFromInputs()
        nextTick(() => (suppressWatch.value = false))
    }

    function validate() {
        resetErrors()
        let valid = true
        if (!(form.title || '').trim()) {
            errors.title = 'Title is required'
            valid = false
        }
        if (!(form.project || '').trim()) {
            errors.project = 'Project is required'
            valid = false
        }
        if (!(form.task_type || '').trim()) {
            errors.task_type = 'Type is required'
            valid = false
        }
        if (!(form.priority || '').trim()) {
            errors.priority = 'Priority is required'
            valid = false
        }
        if (!(form.status || '').trim()) {
            errors.status = 'Status is required'
            valid = false
        }
        return valid
    }

    async function handleSubmit() {
        if (mode.value !== 'create') return
        if (!validate()) return
        submitting.value = true
        try {
            const payload = {
                title: form.title.trim(),
                project: form.project,
                priority: form.priority,
                task_type: form.task_type,
                reporter: form.reporter || undefined,
                assignee: form.assignee || undefined,
                due_date: form.due_date || undefined,
                effort: form.effort || undefined,
                description: form.description || undefined,
                tags: form.tags,
                relationships: buildRelationships(),
                custom_fields: buildCustomFields(),
            }
            const created = await api.addTask(payload as any)
            if (form.status && created.status !== form.status) {
                await api.setStatus(created.id, form.status)
                created.status = form.status as any
            }
            showToast('Task created')
            emit('created', created)
            closePanel()
        } catch (error: any) {
            showToast(error?.message || 'Failed to create task')
        } finally {
            submitting.value = false
        }
    }

    async function onFieldBlur(field: string) {
        if (!ready.value || suppressWatch.value || mode.value !== 'edit') return
        await updateField(field)
    }

    async function updateField(field: string) {
        if (!task.id) return
        const patch: Record<string, any> = {}
        switch (field) {
            case 'title':
                if (!form.title.trim()) {
                    form.title = task.title
                    return
                }
                patch.title = form.title.trim()
                break
            case 'task_type':
                if (!form.task_type) return
                patch.task_type = form.task_type
                break
            case 'priority':
                if (!form.priority) return
                patch.priority = form.priority
                break
            case 'reporter':
                patch.reporter = (form.reporter ?? '').trim()
                break
            case 'assignee':
                patch.assignee = (form.assignee ?? '').trim()
                break
            case 'due_date':
                patch.due_date = form.due_date || undefined
                break
            case 'effort':
                patch.effort = form.effort || undefined
                break
            case 'description':
                if (!((form.description ?? '').trim()) && !((task.description ?? '').trim())) {
                    return
                }
                if ((form.description ?? '') === (task.description ?? '')) {
                    return
                }
                patch.description = form.description || undefined
                break
            default:
                return
        }
        await applyPatch(patch)
    }

    async function applyPatch(patch: Record<string, any>) {
        if (!task.id) return
        try {
            const updated = await api.updateTask(task.id, patch as any)
            Object.assign(task, updated)
            suppressWatch.value = true
            applyTask(updated)
            emit('updated', updated)
        } catch (error: any) {
            showToast(error?.message || 'Failed to save changes')
        } finally {
            nextTick(() => (suppressWatch.value = false))
        }
    }

    async function updateStatus(status: string) {
        if (mode.value !== 'edit' || !task.id) return
        if (!status) return
        try {
            const updated = await api.setStatus(task.id, status)
            Object.assign(task, updated)
            suppressWatch.value = true
            applyTask(updated)
            emit('updated', updated)
            showToast('Status updated')
        } catch (error: any) {
            showToast(error?.message || 'Failed to change status')
            form.status = task.status
        } finally {
            nextTick(() => (suppressWatch.value = false))
        }
    }

    async function reloadTask() {
        if (mode.value === 'edit' && task.id) {
            await loadTask(task.id)
        }
    }

    function formatDate(value: string) {
        try {
            return new Date(value).toLocaleString()
        } catch {
            return value
        }
    }

    function formatCommit(value: string) {
        if (!value) return ''
        return value.slice(0, 7)
    }

    function formatFieldName(value: string) {
        if (!value) return ''
        return value
            .split(/[_\s]+/)
            .filter(Boolean)
            .map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
            .join(' ')
    }

    function formatChangeValue(value?: string | null) {
        if (value === undefined || value === null) return ''
        const trimmed = value.trim()
        if (!trimmed.length) return ''
        return trimmed.length > 60 ? `${trimmed.slice(0, 57)}…` : trimmed
    }

    function onProjectChange() {
        if (mode.value === 'create') {
            refreshConfig(form.project)
                .then(() => applyDefaults(form.project))
                .catch(() => { })
        }
    }

    async function ensureProjectsLoaded() {
        if (!projects.value.length) {
            await refreshProjects()
        }
    }

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
        }
    }

    function applyDefaults(project: string) {
        suppressWatch.value = true
        form.project = project || defaults.value.project || projectFromList() || ''
        form.priority = defaults.value.priority || priorities.value[0] || ''
        form.status = defaults.value.status || statuses.value[0] || ''
        form.task_type = defaults.value.type || types.value[0] || ''
        form.reporter = defaults.value.reporter || ''
        form.assignee = defaults.value.assignee || ''
        form.tags = (defaults.value.tags || [])
            .map((tag: string) => tag.trim())
            .filter((tag: string) => tag.length > 0)
        mergeKnownTags(form.tags)
        seedCategoryField(defaults.value.category)
        syncOwnershipControls()
        preloadPeople(form.project)
        nextTick(() => (suppressWatch.value = false))
    }

    function applyTask(data: TaskDTO) {
        suppressWatch.value = true
        form.id = data.id
        form.project = data.id.split('-')[0]
        form.title = data.title
        form.status = data.status
        form.priority = data.priority
        form.task_type = data.task_type
        form.reporter = data.reporter || ''
        form.assignee = data.assignee || ''
        form.due_date = data.due_date || ''
        form.effort = data.effort || ''
        form.description = data.description || ''
        form.tags = [...(data.tags || [])]
        mergeKnownTags(form.tags)
        resetReferencePreviews()
        task.comments = Array.isArray(data.comments) ? data.comments.map((comment) => ({ ...comment })) : []
        task.references = Array.isArray(data.references) ? data.references.map((reference) => ({ ...reference })) : []
        task.history = Array.isArray(data.history)
            ? data.history.map((entry) => ({
                ...entry,
                changes: Array.isArray(entry.changes) ? entry.changes.map((change) => ({ ...change })) : [],
            }))
            : []
        changeLog.value = task.history
            .slice()
            .reverse()
            .map((entry) => ({
                ...entry,
                changes: entry.changes?.map((change) => ({ ...change })) || [],
            }))
        syncOwnershipControls()
        preloadPeople(form.project)
        resetCustomFields()
        const custom = (data.custom_fields || {}) as Record<string, unknown>
        Object.entries(custom).forEach(([key, value]) => {
            customFields[key] = value === undefined || value === null ? '' : String(value)
            customFieldKeys[key] = key
        })
        seedCategoryField(data.category)
        applyRelationshipsFromTask(data)
        cancelEditCommentRef()
        snapshotRelationshipsBaselineFromTask(data)
        nextTick(() => (suppressWatch.value = false))
    }

    async function loadTask(id: string) {
        commitHistory.value = []
        try {
            const data = await api.getTask(id)
            Object.assign(task, data)
            await refreshConfig(id.split('-')[0])
            applyTask(data)
            await loadCommitHistory(id)
        } catch (error: any) {
            showToast(error?.message || 'Failed to load task')
        } finally {
            loading.value = false
        }
    }

    async function loadCommitHistory(id: string) {
        commitsLoading.value = true
        try {
            const items = await api.taskHistory(id, 8)
            commitHistory.value = items
        } catch {
            commitHistory.value = []
        } finally {
            commitsLoading.value = false
        }
    }

    async function refreshCommits() {
        if (!task.id) return
        await loadCommitHistory(task.id)
    }

    const closeListener = (event: KeyboardEvent) => {
        if (event.key === 'Escape' && props.open) {
            closePanel()
        }
    }

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
    } = useTaskComments({
        mode,
        task,
        submitting,
        applyTask,
        emitUpdated: (updated) => emit('updated', updated),
    })

    resetCommentsRef = resetComments
    cancelEditCommentRef = cancelEditComment

    watch(
        () => props.open,
        async (isOpen) => {
            if (!isOpen) {
                cleanup()
                return
            }
            await ensureProjectsLoaded()
            await initialize()
        },
        { immediate: true },
    )

    watch(
        () => props.taskId,
        async (next, prev) => {
            if (!props.open) return
            if (next === prev) return
            await initialize()
        },
    )

    watch(
        () => form.tags.slice(),
        async (tags, prev = []) => {
            if (!ready.value || suppressWatch.value || mode.value !== 'edit') return
            const normalized = tags.map((tag) => tag.trim()).filter((tag) => tag.length > 0)
            const previous = prev.map((tag) => tag.trim()).filter((tag) => tag.length > 0)
            if (normalized.length === previous.length && normalized.every((tag, index) => tag === previous[index])) {
                return
            }
            await applyPatch({ tags: normalized })
        },
    )

    watch(
        () => form.project,
        (project) => {
            if (suppressWatch.value) return
            preloadPeople(project)
        },
    )

    watch(
        () => relationships.value,
        () => {
            if (mode.value !== 'edit' || !ready.value || suppressWatch.value) return
            commitRelationships()
        },
        { deep: true },
    )

    onMounted(() => {
        if (typeof window !== 'undefined') {
            window.addEventListener('keydown', closeListener)
        }
    })

    onUnmounted(() => {
        if (typeof window !== 'undefined') {
            window.removeEventListener('keydown', closeListener)
        }
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
