import type { ComputedRef, Ref } from 'vue'
import { nextTick, watch } from 'vue'
import type { TaskDTO } from '../../api/types'
import { fromDateInputValue, toDateInputValue } from '../../utils/date'
import { projectOf } from '../../utils/text'

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
    updateTask: (id: string, patch: Record<string, unknown>) => Promise<TaskDTO>
    getTask: (id: string) => Promise<TaskDTO>
}

interface TaskPanelEmitter {
    (event: 'created', task: TaskDTO): void
    (event: 'updated', task: TaskDTO): void
}

interface UseTaskPanelPersistenceOptions {
    panelGeneration: Ref<number>
    getTaskId: () => string | null | undefined
    canCreate: ComputedRef<boolean>
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
    buildCustomFields: () => Record<string, unknown>
    applyTask: (data: TaskDTO) => void
    applyTaskCustomFields: (values: Record<string, unknown>) => void
    applyRelationshipsFromTask: (task: TaskDTO | null | undefined) => void
    snapshotRelationshipsBaselineFromTask: (task: TaskDTO | null | undefined) => void
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

/** Outcome of a queued autosave request, reported to enqueue callers. */
export interface SaveOutcome {
    ok: boolean
    stale: boolean
    error?: unknown
}

interface SaveWaiter {
    revision: number
    fields: string[]
    resolve: (outcome: SaveOutcome) => void
}

/**
 * Per-task autosave queue. Patches are coalesced field-wise (last write wins)
 * while a request is in flight and flushed strictly serially per task id, so
 * status, custom-field, and reference edits can never reorder or overwrite
 * each other server-side. Scope (panel generation) and task-id guards are
 * retained: responses and failures are only applied when the panel still
 * shows the same task in the same scope.
 *
 * Each enqueue bumps a per-queue revision and stamps its fields into
 * `pendingRevisions`. A waiter resolves only when a completed snapshot
 * carried its field at a revision >= its own, so a same-field save queued
 * during an in-flight request resolves with the outcome of the request that
 * actually persisted its (or a newer, coalesced) value — never with the
 * stale outcome of the superseded request.
 */
interface SaveQueueState {
    pending: Record<string, unknown> | null
    pendingRevisions: Record<string, number> | null
    running: boolean
    revision: number
    waiters: SaveWaiter[]
}

export function useTaskPanelPersistence(options: UseTaskPanelPersistenceOptions): TaskPanelPersistenceApi {
    let loadGeneration = 0
    let createGeneration = 0
    watch(() => options.form.project, () => { createGeneration += 1 }, { flush: 'sync' })
    const canEdit = () => options.mode.value === 'edit' && options.ready.value && !options.loading.value &&
        options.task.id === options.getTaskId() && options.form.id === options.task.id

    const saveQueues = new Map<string, SaveQueueState>()

    function saveQueueFor(id: string): SaveQueueState {
        let state = saveQueues.get(id)
        if (!state) {
            state = { pending: null, pendingRevisions: null, running: false, revision: 0, waiters: [] }
            saveQueues.set(id, state)
        }
        return state
    }

    function enqueuePatch(id: string, patch: Record<string, unknown>): Promise<SaveOutcome> {
        const state = saveQueueFor(id)
        const revision = ++state.revision
        state.pending = { ...(state.pending ?? {}), ...patch }
        state.pendingRevisions = { ...(state.pendingRevisions ?? {}) }
        const fields = Object.keys(patch)
        for (const field of fields) {
            state.pendingRevisions[field] = revision
        }
        const outcome = new Promise<SaveOutcome>((resolve) => {
            state.waiters.push({ revision, fields, resolve })
        })
        if (!state.running) void flushSaveQueue(id)
        return outcome
    }

    /**
     * Resolve waiters whose fields the completed snapshot carried at their
     * revision or newer. Older-revision waiters whose fields were coalesced
     * into a newer value resolve with that snapshot's outcome (their intent
     * was superseded and is durably covered by it); waiters whose fields have
     * not yet been sent keep waiting for their own request.
     */
    function resolveWaiters(state: SaveQueueState, snapshotRevisions: Record<string, number>, outcome: SaveOutcome) {
        const remaining: SaveWaiter[] = []
        for (const waiter of state.waiters) {
            const covered = waiter.fields.every((field) => {
                const revision = snapshotRevisions[field]
                return revision !== undefined && revision >= waiter.revision
            })
            if (covered) {
                waiter.resolve(outcome)
            } else {
                remaining.push(waiter)
            }
        }
        state.waiters = remaining
    }

    async function flushSaveQueue(id: string): Promise<void> {
        const state = saveQueues.get(id)
        if (!state || state.running) return
        state.running = true
        try {
            while (state.pending) {
                const snapshot = state.pending
                const snapshotRevisions = state.pendingRevisions ?? {}
                state.pending = null
                state.pendingRevisions = null
                const scope = options.panelGeneration.value
                const stillCurrent = () => scope === options.panelGeneration.value && id === options.getTaskId()
                let outcome: SaveOutcome
                try {
                    const updated = await options.apiClient.updateTask(id, snapshot)
                    if (stillCurrent()) {
                        applyServerTask(updated)
                        options.emit('updated', updated)
                    }
                    outcome = { ok: true, stale: !stillCurrent() }
                } catch (error: unknown) {
                    const stale = !stillCurrent()
                    if (!stale) await reconcileFailedSave(id, snapshot, state, error, stillCurrent)
                    outcome = { ok: false, stale, error }
                }
                resolveWaiters(state, snapshotRevisions, outcome)
            }
        } finally {
            state.running = false
            if (state.pending && saveQueues.get(id) === state) void flushSaveQueue(id)
        }
    }

    function applyServerTask(updated: TaskDTO) {
        Object.assign(options.task, updated)
        options.suppressWatch.value = true
        options.applyTask(updated)
        nextTick(() => {
            options.suppressWatch.value = false
        })
    }

    /**
     * After a failed save, re-fetch the last persisted server state and revert
     * only the affected fields that carry no newer intent: fields queued in
     * the meantime keep their pending value, and fields the user has edited
     * again (but not yet committed) are left untouched.
     */
    async function reconcileFailedSave(
        id: string,
        snapshot: Record<string, unknown>,
        state: SaveQueueState,
        error: unknown,
        stillCurrent: () => boolean,
    ): Promise<void> {
        const message = error instanceof Error && error.message ? error.message : 'Failed to save changes'
        options.showToast(message)
        let server: TaskDTO
        try {
            server = await options.apiClient.getTask(id)
        } catch {
            return
        }
        if (!stillCurrent()) return
        Object.assign(options.task, server)
        const pendingFields = new Set(Object.keys(state.pending ?? {}))
        options.suppressWatch.value = true
        try {
            for (const field of Object.keys(snapshot)) {
                if (pendingFields.has(field)) continue
                if (formIntentChanged(field, snapshot[field])) continue
                applyServerFieldToForm(field, server)
            }
        } finally {
            nextTick(() => {
                options.suppressWatch.value = false
            })
        }
        options.emit('updated', server)
    }

    /** Value the panel would send for `field` right now, or undefined if unknown. */
    function currentIntentFor(field: string): unknown {
        switch (field) {
            case 'title': return options.form.title.trim()
            case 'status': return options.form.status
            case 'priority': return options.form.priority
            case 'task_type': return options.form.task_type
            case 'reporter': return (options.form.reporter ?? '').trim() || null
            case 'assignee': return (options.form.assignee ?? '').trim() || null
            case 'due_date': return fromDateInputValue(options.form.due_date)
            case 'effort': return options.form.effort || null
            case 'description': return options.form.description || null
            case 'tags': {
                const tags = (options.form.tags || []).map((tag) => (tag || '').trim()).filter((tag) => tag.length > 0)
                return tags
            }
            case 'sprints': return normalizeSprints(options.form.sprints)
            case 'custom_fields': return options.buildCustomFields()
            case 'relationships': return options.buildRelationships()
            default: return undefined
        }
    }

    function formIntentChanged(field: string, sentValue: unknown): boolean {
        const current = currentIntentFor(field)
        if (current === undefined) return true
        return !valuesEqual(current, sentValue)
    }

    function valuesEqual(a: unknown, b: unknown): boolean {
        if (a === b) return true
        if (Array.isArray(a) && Array.isArray(b)) {
            return a.length === b.length && a.every((value, index) => valuesEqual(value, b[index]))
        }
        if (a && b && typeof a === 'object' && typeof b === 'object') {
            const aKeys = Object.keys(a as Record<string, unknown>)
            const bKeys = Object.keys(b as Record<string, unknown>)
            return aKeys.length === bKeys.length &&
                aKeys.every((key) => key in (b as Record<string, unknown>) &&
                    valuesEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]))
        }
        return false
    }

    function normalizeSprints(values: number[] | undefined | null): number[] {
        if (!Array.isArray(values)) return [] as number[]
        const unique = Array.from(
            new Set(values.map((value) => Number(value)).filter((value) => Number.isFinite(value))),
        )
        unique.sort((a, b) => a - b)
        return unique
    }

    function applyServerFieldToForm(field: string, server: TaskDTO) {
        switch (field) {
            case 'title': options.form.title = server.title; break
            case 'status': options.form.status = server.status; break
            case 'priority': options.form.priority = server.priority; break
            case 'task_type': options.form.task_type = server.task_type; break
            case 'reporter': options.form.reporter = server.reporter || ''; break
            case 'assignee': options.form.assignee = server.assignee || ''; break
            case 'due_date': options.form.due_date = toDateInputValue(server.due_date); break
            case 'effort': options.form.effort = server.effort || ''; break
            case 'description': options.form.description = server.description || ''; break
            case 'tags': options.form.tags = [...(server.tags || [])]; break
            case 'sprints': options.form.sprints = [...(server.sprints || [])]; break
            case 'custom_fields':
                options.applyTaskCustomFields((server.custom_fields || {}) as Record<string, unknown>)
                break
            case 'relationships':
                options.applyRelationshipsFromTask(server)
                options.snapshotRelationshipsBaselineFromTask(server)
                break
            default: break
        }
    }

    const applyPatch = async (patch: Record<string, unknown>) => {
        if (!canEdit()) return
        await enqueuePatch(options.task.id, patch)
    }

    const updateField = async (field: string) => {
        if (!canEdit()) return
        const patch: Record<string, unknown> = {}
        switch (field) {
            case 'title':
                if (!options.form.title.trim()) {
                    options.form.title = options.task.title
                    return
                }
                if (options.form.title.trim() === options.task.title) return
                patch.title = options.form.title.trim()
                break
            case 'task_type':
                if (!options.form.task_type) return
                if (options.form.task_type === options.task.task_type) return
                patch.task_type = options.form.task_type
                break
            case 'priority':
                if (!options.form.priority) return
                if (options.form.priority === options.task.priority) return
                patch.priority = options.form.priority
                break
            case 'reporter':
                if ((options.form.reporter ?? '').trim() === (options.task.reporter || '')) return
                patch.reporter = (options.form.reporter ?? '').trim() || null
                break
            case 'assignee':
                if ((options.form.assignee ?? '').trim() === (options.task.assignee || '')) return
                patch.assignee = (options.form.assignee ?? '').trim() || null
                break
            case 'due_date': {
                const due = fromDateInputValue(options.form.due_date)
                if (due === (options.task.due_date || null)) return
                patch.due_date = due
                break
            }
            case 'effort':
                if ((options.form.effort || '') === (options.task.effort || '')) return
                patch.effort = options.form.effort || null
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
                patch.description = options.form.description || null
                break
            case 'sprints': {
                const current = normalizeSprints(options.task.sprints as any)
                const next = normalizeSprints(options.form.sprints as any)
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
        if (!canEdit()) return
        if (!status) return
        const outcome = await enqueuePatch(options.task.id, { status })
        if (outcome.ok && !outcome.stale) {
            options.showToast('Status updated')
        }
    }

    const handleSubmit = async () => {
        if (!options.canCreate.value || options.submitting.value) return
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
        const scope = options.panelGeneration.value
        const request = createGeneration
        const status = options.form.status
        const current = () => scope === options.panelGeneration.value && request === createGeneration
        try {
            const dueDateValue = fromDateInputValue(options.form.due_date)
            const payload = {
                title: options.form.title.trim(),
                project: options.form.project,
                status: status || undefined,
                priority: options.form.priority,
                task_type: options.form.task_type,
                reporter: options.form.reporter || undefined,
                assignee: options.form.assignee || undefined,
                due_date: dueDateValue ?? undefined,
                effort: options.form.effort || undefined,
                description: options.form.description || undefined,
                tags: [...options.form.tags],
                sprints: options.form.sprints.length ? [...options.form.sprints] : undefined,
                relationships: options.buildRelationships(),
                custom_fields: options.buildCustomFields(),
            }
            const created = await options.apiClient.addTask(payload)
            if (!current()) return
            options.showToast('Task created')
            options.emit('created', created)
            options.closePanel()
        } catch (error: any) {
            if (!current()) return
            options.showToast(error?.message || 'Failed to create task')
        } finally {
            if (scope === options.panelGeneration.value) options.submitting.value = false
        }
    }

    const loadTask = async (id: string): Promise<TaskDTO | undefined> => {
        const scope = options.panelGeneration.value
        const request = ++loadGeneration
        const current = () => scope === options.panelGeneration.value && request === loadGeneration && id === options.getTaskId()
        if (!current()) return undefined
        options.ready.value = false
        options.loading.value = true
        options.resetActivity()
        try {
            const data = await options.apiClient.getTask(id)
            if (!current()) return undefined
            await options.refreshConfig(projectOf(id) || '')
            if (!current()) return undefined
            Object.assign(options.task, data)
            options.applyTask(data)
            await options.loadCommitHistory(id)
            if (!current()) return undefined
            options.ready.value = true
            return data
        } catch (error: any) {
            if (!current()) return undefined
            options.showToast(error?.message || 'Failed to load task')
            return undefined
        } finally {
            if (current()) options.loading.value = false
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
