import type { ComputedRef, Ref } from 'vue'
import { nextTick } from 'vue'
import type { TaskDTO } from '../../api/types'
import { fromDateInputValue } from '../../utils/date'

export interface TaskPanelFormState {
    id: string
    title: string
    project: string
    status: string
    priority: string
    task_type: string
    reporter: string
    assignee: string
    due_date: string
    effort: string
    description: string
    tags: string[]
    sprints: number[]
}

interface TaskPanelApiClient {
    addTask: (payload: any) => Promise<TaskDTO>
    setStatus: (id: string, status: string) => Promise<TaskDTO>
    updateTask: (id: string, patch: Record<string, unknown>) => Promise<TaskDTO>
    getTask: (id: string) => Promise<TaskDTO>
}

interface TaskPanelEmitter {
    (event: 'created', task: TaskDTO): void
    (event: 'updated', task: TaskDTO): void
}

interface UseTaskPanelPersistenceOptions {
    mode: ComputedRef<'create' | 'edit'>
    task: TaskDTO
    form: TaskPanelFormState
    ready: Ref<boolean>
    suppressWatch: Ref<boolean>
    submitting: Ref<boolean>
    loading: Ref<boolean>
    apiClient: TaskPanelApiClient
    showToast: (message: string) => void
    buildRelationships: () => unknown
    buildCustomFields: () => Record<string, string>
    applyTask: (data: TaskDTO) => void
    validate: () => boolean
    closePanel: () => void
    resetActivity: () => void
    refreshConfig: (project: string) => Promise<void>
    loadCommitHistory: (id: string, limit?: number) => Promise<void>
    emit: TaskPanelEmitter
}

export interface TaskPanelPersistenceApi {
    handleSubmit: () => Promise<void>
    onFieldBlur: (field: string) => Promise<void>
    updateField: (field: string) => Promise<void>
    applyPatch: (patch: Record<string, unknown>) => Promise<void>
    updateStatus: (status: string) => Promise<void>
    reloadTask: () => Promise<TaskDTO | undefined>
    loadTask: (id: string) => Promise<TaskDTO | undefined>
}

export function useTaskPanelPersistence(options: UseTaskPanelPersistenceOptions): TaskPanelPersistenceApi {
    const applyPatch = async (patch: Record<string, unknown>) => {
        if (!options.task.id) return
        try {
            const updated = await options.apiClient.updateTask(options.task.id, patch)
            Object.assign(options.task, updated)
            options.suppressWatch.value = true
            options.applyTask(updated)
            options.emit('updated', updated)
        } catch (error: any) {
            options.showToast(error?.message || 'Failed to save changes')
        } finally {
            nextTick(() => (options.suppressWatch.value = false))
        }
    }

    const updateField = async (field: string) => {
        if (!options.task.id) return
        const patch: Record<string, unknown> = {}
        switch (field) {
            case 'title':
                if (!options.form.title.trim()) {
                    options.form.title = options.task.title
                    return
                }
                patch.title = options.form.title.trim()
                break
            case 'task_type':
                if (!options.form.task_type) return
                patch.task_type = options.form.task_type
                break
            case 'priority':
                if (!options.form.priority) return
                patch.priority = options.form.priority
                break
            case 'reporter':
                patch.reporter = (options.form.reporter ?? '').trim()
                break
            case 'assignee':
                patch.assignee = (options.form.assignee ?? '').trim()
                break
            case 'due_date': {
                const due = fromDateInputValue(options.form.due_date)
                patch.due_date = due ?? undefined
                break
            }
            case 'effort':
                patch.effort = options.form.effort || undefined
                break
            case 'description':
                if (
                    !((options.form.description ?? '').trim()) &&
                    !((options.task.description ?? '').trim())
                ) {
                    return
                }
                if ((options.form.description ?? '') === (options.task.description ?? '')) {
                    return
                }
                patch.description = options.form.description || undefined
                break
            case 'sprints': {
                const normalize = (values: number[] | undefined | null) => {
                    if (!Array.isArray(values)) return [] as number[]
                    const unique = Array.from(
                        new Set(values.map((value) => Number(value)).filter((value) => Number.isFinite(value))),
                    )
                    unique.sort((a, b) => a - b)
                    return unique
                }
                const current = normalize(options.task.sprints as any)
                const next = normalize(options.form.sprints as any)
                if (current.length === next.length && current.every((value, index) => value === next[index])) {
                    return
                }
                patch.sprints = next
                break
            }
            default:
                return
        }
        await applyPatch(patch)
    }

    const onFieldBlur = async (field: string) => {
        if (!options.ready.value || options.suppressWatch.value || options.mode.value !== 'edit') return
        await updateField(field)
    }

    const updateStatus = async (status: string) => {
        if (options.mode.value !== 'edit' || !options.task.id) return
        if (!status) return
        try {
            const updated = await options.apiClient.setStatus(options.task.id, status)
            Object.assign(options.task, updated)
            options.suppressWatch.value = true
            options.applyTask(updated)
            options.emit('updated', updated)
            options.showToast('Status updated')
        } catch (error: any) {
            options.showToast(error?.message || 'Failed to change status')
            options.form.status = options.task.status
        } finally {
            nextTick(() => (options.suppressWatch.value = false))
        }
    }

    const handleSubmit = async () => {
        if (options.mode.value !== 'create') return
        if (!(options.form.project || '').trim()) {
            options.validate()
            options.showToast('Project is required')
            return
        }
        if (!options.validate()) {
            options.showToast('Please fill out required fields')
            return
        }
        options.submitting.value = true
        try {
            const dueDateValue = fromDateInputValue(options.form.due_date)
            const payload = {
                title: options.form.title.trim(),
                project: options.form.project,
                priority: options.form.priority,
                task_type: options.form.task_type,
                reporter: options.form.reporter || undefined,
                assignee: options.form.assignee || undefined,
                due_date: dueDateValue ?? undefined,
                effort: options.form.effort || undefined,
                description: options.form.description || undefined,
                tags: options.form.tags,
                sprints: options.form.sprints.length ? [...options.form.sprints] : undefined,
                relationships: options.buildRelationships(),
                custom_fields: options.buildCustomFields(),
            }
            const created = await options.apiClient.addTask(payload)
            if (options.form.status && created.status !== options.form.status) {
                const synced = await options.apiClient.setStatus(created.id, options.form.status)
                Object.assign(created, synced)
            }
            options.showToast('Task created')
            options.emit('created', created)
            options.closePanel()
        } catch (error: any) {
            options.showToast(error?.message || 'Failed to create task')
        } finally {
            options.submitting.value = false
        }
    }

    const loadTask = async (id: string): Promise<TaskDTO | undefined> => {
        options.resetActivity()
        try {
            const data = await options.apiClient.getTask(id)
            Object.assign(options.task, data)
            await options.refreshConfig(id.split('-')[0])
            options.applyTask(data)
            await options.loadCommitHistory(id)
            return data
        } catch (error: any) {
            options.showToast(error?.message || 'Failed to load task')
            return undefined
        } finally {
            options.loading.value = false
        }
    }

    const reloadTask = async (): Promise<TaskDTO | undefined> => {
        if (options.mode.value === 'edit' && options.task.id) {
            const updated = await loadTask(options.task.id)
            if (updated) {
                options.emit('updated', updated)
            }
            return updated
        }
        return undefined
    }

    return {
        handleSubmit,
        onFieldBlur,
        updateField,
        applyPatch,
        updateStatus,
        reloadTask,
        loadTask,
    }
}
