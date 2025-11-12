import type { ComputedRef, Ref } from 'vue'
import { nextTick, watch } from 'vue'
import type { TaskDTO } from '../../api/types'
import { toDateInputValue } from '../../utils/date'
import type { TaskPanelFormState } from './useTaskPanelPersistence'

type ActivityTab = 'comments' | 'relationships' | 'history' | 'commits' | 'references'

interface ProjectInfo {
    prefix: string
    name?: string | null
}

interface TaskPanelDefaults {
    project?: string
    priority?: string
    priorities?: string[]
    status?: string
    statuses?: string[]
    type?: string
    types?: string[]
    reporter?: string
    assignee?: string
    tags?: string[]
    customFields?: Record<string, unknown>
}

interface UseTaskPanelFormLifecycleOptions {
    mode: ComputedRef<'create' | 'edit'>
    ready: Ref<boolean>
    loading: Ref<boolean>
    submitting: Ref<boolean>
    suppressWatch: Ref<boolean>
    form: TaskPanelFormState
    task: TaskDTO
    errors: Record<string, string>
    defaults: Ref<TaskPanelDefaults>
    statuses: Ref<string[] | undefined | null>
    priorities: Ref<string[] | undefined | null>
    types: Ref<string[] | undefined | null>
    projects: Ref<ProjectInfo[]>
    activityTab: Ref<ActivityTab>
    getInitialProject: () => string | null | undefined
    resetSprintsState: () => void
    mergeKnownTags: (tags: Array<string | null | undefined>) => void
    resetCustomFields: () => void
    ensureConfiguredCustomFields: () => void
    applyDefaultsFromConfig: (defaults: Record<string, unknown>) => void
    syncOwnershipControls: () => void
    resetOwnership: () => void
    preloadPeople: (project: string) => void
    resetReferencePreviews: () => void
    resetRelationships: () => void
    snapshotRelationshipsBaselineFromInputs: () => void
    snapshotRelationshipsBaselineFromTask: (task: TaskDTO | null | undefined) => void
    applyRelationshipsFromTask: (task: TaskDTO | null | undefined) => void
    applyTaskCustomFields: (values: Record<string, unknown>) => void
    applyTaskSprints: (task: TaskDTO) => void
    syncFromTaskHistory: (task: TaskDTO) => void
    resetActivity: () => void
    resetComments: () => void
    cancelEditComment: () => void
    refreshProjects: () => Promise<void>
    refreshConfig: (project: string) => Promise<void>
}

export interface TaskPanelFormLifecycleApi {
    resetErrors: () => void
    resetForm: () => void
    cleanup: () => void
    validate: () => boolean
    applyDefaults: (project: string) => void
    applyTask: (task: TaskDTO) => void
    projectForSuggestions: () => string
    projectFromList: () => string
    ensureProjectsLoaded: () => Promise<void>
    onProjectChange: () => void
}

export function useTaskPanelFormLifecycle(options: UseTaskPanelFormLifecycleOptions): TaskPanelFormLifecycleApi {
    const projectFromList = () => options.projects.value[0]?.prefix || ''

    const resetErrors = () => {
        Object.keys(options.errors).forEach((key) => {
            delete options.errors[key]
        })
    }

    const resetForm = () => {
        options.suppressWatch.value = true
        options.form.id = ''
        options.form.title = ''
        options.form.project = options.getInitialProject() || ''
        options.form.status = ''
        options.form.priority = ''
        options.form.task_type = ''
        options.form.reporter = ''
        options.form.assignee = ''
        options.form.due_date = ''
        options.form.effort = ''
        options.form.description = ''
        options.form.tags = []
        options.resetSprintsState()
        options.resetOwnership()
        options.activityTab.value = 'comments'
        options.resetCustomFields()
        options.resetRelationships()
        options.resetReferencePreviews()
        options.resetComments()
        options.snapshotRelationshipsBaselineFromInputs()
        nextTick(() => (options.suppressWatch.value = false))
    }

    const applyDefaults = (project: string) => {
        const defaults = options.defaults.value || {}
        const priorityOptions = options.priorities.value || []
        const statusOptions = options.statuses.value || []
        const typeOptions = options.types.value || []

        options.suppressWatch.value = true
        options.form.project = project || defaults.project || projectFromList() || ''
        options.form.priority = defaults.priority || priorityOptions[0] || ''
        options.form.status = defaults.status || statusOptions[0] || ''
        options.form.task_type = defaults.type || typeOptions[0] || ''
        options.form.reporter = defaults.reporter || ''
        options.form.assignee = defaults.assignee || ''
        options.form.tags = (defaults.tags || [])
            .map((tag: string) => tag.trim())
            .filter((tag: string) => tag.length > 0)
        options.resetSprintsState()
        options.mergeKnownTags(options.form.tags)
        options.resetCustomFields()
        options.ensureConfiguredCustomFields()
        options.applyDefaultsFromConfig(defaults.customFields || {})
        options.syncOwnershipControls()
        options.preloadPeople(options.form.project)
        nextTick(() => (options.suppressWatch.value = false))
    }

    const cleanup = () => {
        resetErrors()
        resetForm()
        options.resetActivity()
        options.cancelEditComment()
    }

    const validate = () => {
        resetErrors()
        let valid = true
        if (!(options.form.title || '').trim()) {
            options.errors.title = 'Title is required'
            valid = false
        }
        if (!(options.form.project || '').trim()) {
            options.errors.project = 'Project is required'
            valid = false
        }
        if (!(options.form.task_type || '').trim()) {
            options.errors.task_type = 'Type is required'
            valid = false
        }
        if (!(options.form.priority || '').trim()) {
            options.errors.priority = 'Priority is required'
            valid = false
        }
        if (!(options.form.status || '').trim()) {
            options.errors.status = 'Status is required'
            valid = false
        }
        return valid
    }

    const applyTask = (data: TaskDTO) => {
        options.suppressWatch.value = true
        const project = data.id ? data.id.split('-')[0] : options.form.project

        options.form.id = data.id
        options.form.project = project || ''
        options.form.title = data.title
        options.form.status = data.status
        options.form.priority = data.priority
        options.form.task_type = data.task_type
        options.form.reporter = data.reporter || ''
        options.form.assignee = data.assignee || ''
        options.form.due_date = toDateInputValue(data.due_date)
        options.form.effort = data.effort || ''
        options.form.description = data.description || ''
        options.form.tags = [...(data.tags || [])]
        options.mergeKnownTags(options.form.tags)
        options.applyTaskSprints(data)
        options.resetReferencePreviews()

        options.task.comments = Array.isArray(data.comments)
            ? data.comments.map((comment) => ({ ...comment }))
            : []
        options.task.references = Array.isArray(data.references)
            ? data.references.map((reference) => ({ ...reference }))
            : []
        options.task.history = Array.isArray(data.history)
            ? data.history.map((entry) => ({
                ...entry,
                changes: Array.isArray(entry.changes)
                    ? entry.changes.map((change) => ({ ...change }))
                    : [],
            }))
            : []

        options.syncFromTaskHistory(data)
        options.syncOwnershipControls()
        options.preloadPeople(options.form.project)
        options.applyTaskCustomFields((data.custom_fields || {}) as Record<string, unknown>)
        options.applyRelationshipsFromTask(data)
        options.cancelEditComment()
        options.snapshotRelationshipsBaselineFromTask(data)

        nextTick(() => (options.suppressWatch.value = false))
    }

    const ensureProjectsLoaded = async () => {
        if (!options.projects.value.length) {
            await options.refreshProjects()
        }
    }

    const projectForSuggestions = () => {
        if (options.form.project) return options.form.project
        if (options.form.id) return options.form.id.split('-')[0]
        const initial = options.getInitialProject()
        if (initial) return initial
        if (options.defaults.value.project) return options.defaults.value.project
        return projectFromList()
    }

    const onProjectChange = () => {
        if (options.mode.value !== 'create') return
        options
            .refreshConfig(options.form.project)
            .then(() => applyDefaults(options.form.project))
            .catch(() => { })
    }

    watch(
        () => options.form.project,
        (project) => {
            if (options.suppressWatch.value) return
            options.preloadPeople(project)
        },
    )

    return {
        resetErrors,
        resetForm,
        cleanup,
        validate,
        applyDefaults,
        applyTask,
        projectForSuggestions,
        projectFromList,
        ensureProjectsLoaded,
        onProjectChange,
    }
}
