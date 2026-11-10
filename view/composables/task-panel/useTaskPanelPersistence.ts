import { errorMessage } from '../../utils/errors'
import type { ComputedRef, Ref } from 'vue'
import { nextTick, ref, watch } from 'vue'
import type { TaskCreate, TaskDTO, TaskRelationships } from '../../api/types'
import { completionStatusKey } from '../useCompletionPolicy'
import { fromDateInputValue, toDateInputValue } from '../../utils/date'
import { applyTaskSnapshot, isDeletedTask, projectOf } from '../../utils/text'

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
    addTask: (payload: TaskCreate) => Promise<TaskDTO>
    updateTask: (id: string, patch: Record<string, unknown>) => Promise<TaskDTO>
    getTask: (id: string, opts?: { includeDeleted?: boolean }) => Promise<TaskDTO>
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
    /** DEV-183: the server answered for a moved task under its new ID. */
    onRedirect?: (id: string) => void
    buildRelationships: () => TaskRelationships
    buildCustomFields: () => Record<string, unknown>
    applyTask: (data: TaskDTO) => void
    applyTaskCustomFields: (values: Record<string, unknown>) => void
    applyRelationshipsFromTask: (task: TaskDTO | null | undefined) => void
    validate: () => boolean
    closePanel: () => void
    resetActivity: () => void
    refreshConfig: (project: string) => Promise<void>
    loadCommitHistory: (id: string, limit?: number) => Promise<void>
    emit: TaskPanelEmitter
    /**
     * DEV-244: runs once the task exists, before the panel closes; uploads
     * files attached while creating it. Returns the task as it is now.
     */
    afterCreate?: (created: TaskDTO) => Promise<TaskDTO>
    /** DEV-246: tell the user which task was created. */
    announceCreated?: (created: TaskDTO) => void
    /**
     * DEV-263: fields whose local edit conflicts with a newer outside change.
     * Autosaves leave them out until the user picks "reload" or "keep mine",
     * so a blur never silently overwrites the newer value.
     */
    isFieldHeld?: (field: string) => boolean
    /**
     * DEV-301: remember unsaved form values (other than `saving`) before a
     * save response is applied; the returned function puts them back.
     */
    preserveUnsaved?: (saving: string[]) => () => void
    /**
     * DEV-301: merge a server version that raced a refused save: unchanged
     * fields update, fields changed on both sides become conflicts.
     */
    mergeServerTask?: (server: TaskDTO) => void
}

export interface TaskPanelPersistenceApi {
    handleSubmit: () => Promise<void>
    onFieldBlur: (field: string) => Promise<void>
    updateField: (field: string) => Promise<void>
    applyPatch: (patch: Record<string, unknown>) => Promise<void>
    updateStatus: (status: string) => Promise<void>
    reloadTask: () => Promise<TaskDTO | undefined>
    loadTask: (id: string) => Promise<TaskDTO | undefined>
    /** DEV-263: number of save requests running (any task); reactive. */
    activeSaves: Readonly<Ref<number>>
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
    watch(
        () => options.form.project,
        () => {
            createGeneration += 1
        },
        { flush: 'sync' },
    )
    // DEV-92: a soft-deleted task is read-only — every field mutation
    // (including status patches and queued autosaves) stops here. The
    // server also rejects mutations on deleted tasks ("restore it first").
    const canEdit = () =>
        options.mode.value === 'edit' &&
        options.ready.value &&
        !options.loading.value &&
        !isDeletedTask(options.task) &&
        options.task.id === options.getTaskId() &&
        options.form.id === options.task.id

    const saveQueues = new Map<string, SaveQueueState>()
    const activeSaves = ref(0)

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
        state.pending = { ...state.pending, ...patch }
        state.pendingRevisions = { ...state.pendingRevisions }
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
        activeSaves.value += 1
        // A refused stale save is retried with the fields that did not
        // clash; a second refusal in a row is reported instead (DEV-301).
        let conflictRetries = 0
        try {
            while (state.pending) {
                const snapshot = state.pending
                const snapshotRevisions = state.pendingRevisions ?? {}
                state.pending = null
                state.pendingRevisions = null
                const scope = options.panelGeneration.value
                const stillCurrent = () => scope === options.panelGeneration.value && id === options.getTaskId()
                // DEV-92: capture the lifecycle before the request; if the
                // task was soft-deleted (external delete + authoritative
                // panel reload) while the save was in flight, the late
                // pre-delete response must not clear the tombstone marker or
                // the read-only state.
                const deletedBeforeSave = isDeletedTask(options.task)
                let outcome: SaveOutcome
                let settled = snapshotRevisions
                // DEV-301: the server refuses the save (409) if the task
                // changed since the panel's snapshot was taken.
                const request: Record<string, unknown> = { ...snapshot }
                if (options.task.id === id && options.task.modified) {
                    request.expected_modified = options.task.modified
                }
                try {
                    const updated = await options.apiClient.updateTask(id, request)
                    const lifecycleFlipped = deletedBeforeSave !== isDeletedTask(options.task)
                    if (stillCurrent() && !lifecycleFlipped) {
                        applyServerTask(updated, Object.keys(snapshot))
                        options.emit('updated', updated)
                    }
                    outcome = { ok: true, stale: !stillCurrent() || lifecycleFlipped }
                    conflictRetries = 0
                } catch (error: unknown) {
                    const stale = !stillCurrent()
                    const refusedAsStale = (error as { status?: number } | null)?.status === 409
                    if (!stale && refusedAsStale && conflictRetries < 1) {
                        conflictRetries += 1
                        const retry = await reconcileStaleSave(id, snapshot, stillCurrent)
                        if (retry && Object.keys(retry).length) {
                            // Resend what did not clash, ahead of newer edits;
                            // its waiters settle with the retried request.
                            // Read the queue again: edits queued during the awaits above
                            // are not visible to the narrowing after `state.pending = null`.
                            const queued = saveQueueFor(id)
                            state.pending = { ...retry, ...queued.pending }
                            const retried: Record<string, number> = {}
                            for (const field of Object.keys(retry)) retried[field] = snapshotRevisions[field] ?? 0
                            state.pendingRevisions = { ...retried, ...queued.pendingRevisions }
                            settled = Object.fromEntries(
                                Object.entries(snapshotRevisions).filter(([field]) => !(field in retry)),
                            )
                        }
                        outcome = { ok: false, stale: false, error }
                    } else {
                        if (!stale) await reconcileFailedSave(id, snapshot, state, error, stillCurrent)
                        outcome = { ok: false, stale, error }
                    }
                }
                resolveWaiters(state, settled, outcome)
            }
        } finally {
            state.running = false
            activeSaves.value -= 1
            if (state.pending && saveQueues.get(id) === state) void flushSaveQueue(id)
        }
    }

    /**
     * Apply a save response. Fields typed meanwhile in other inputs keep the
     * user's text (DEV-301): the response holds only the saved fields' new
     * values, since the save's precondition rules out outside changes.
     */
    function applyServerTask(updated: TaskDTO, saved: string[] = []) {
        const restoreUnsaved = options.preserveUnsaved?.(saved)
        applyTaskSnapshot(options.task, updated)
        options.suppressWatch.value = true
        options.applyTask(updated)
        restoreUnsaved?.()
        void nextTick(() => {
            options.suppressWatch.value = false
        })
    }

    /**
     * DEV-301: a save refused because the task changed elsewhere. Merge the
     * current server task (conflicting fields are held for the user to
     * decide) and return the saved fields to send again.
     */
    async function reconcileStaleSave(
        id: string,
        snapshot: Record<string, unknown>,
        stillCurrent: () => boolean,
    ): Promise<Record<string, unknown> | null> {
        let server: TaskDTO
        try {
            server = await options.apiClient.getTask(id, { includeDeleted: true })
        } catch {
            options.showToast('This task changed elsewhere; reload it before editing')
            return null
        }
        if (!stillCurrent()) return null
        if (options.mergeServerTask) {
            options.mergeServerTask(server)
        } else {
            applyServerTask(server)
        }
        options.emit('updated', server)
        if (isDeletedTask(server)) return null
        const retry: Record<string, unknown> = {}
        for (const field of Object.keys(snapshot)) {
            if (isHeld(field)) continue
            const intent = currentIntentFor(field)
            retry[field] = intent === undefined ? snapshot[field] : intent
        }
        return retry
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
            // Deleted tasks 404 on the strict endpoint; reconcile must be
            // able to see the trash state too.
            server = await options.apiClient.getTask(id, { includeDeleted: true })
        } catch {
            return
        }
        if (!stillCurrent()) return
        applyTaskSnapshot(options.task, server)
        const pendingFields = new Set(Object.keys(state.pending ?? {}))
        options.suppressWatch.value = true
        try {
            for (const field of Object.keys(snapshot)) {
                if (pendingFields.has(field)) continue
                if (formIntentChanged(field, snapshot[field])) continue
                applyServerFieldToForm(field, server)
            }
        } finally {
            void nextTick(() => {
                options.suppressWatch.value = false
            })
        }
        options.emit('updated', server)
    }

    /** Value the panel would send for `field` right now, or undefined if unknown. */
    function currentIntentFor(field: string): unknown {
        switch (field) {
            case 'title':
                return options.form.title.trim()
            case 'status':
                return options.form.status
            case 'priority':
                return options.form.priority
            case 'task_type':
                return options.form.task_type
            case 'reporter':
                return options.form.reporter.trim() || null
            case 'assignee':
                return options.form.assignee.trim() || null
            case 'due_date':
                return fromDateInputValue(options.form.due_date)
            case 'effort':
                return options.form.effort || null
            case 'description':
                return options.form.description || null
            case 'tags': {
                const tags = options.form.tags.map((tag) => (tag || '').trim()).filter((tag) => tag.length > 0)
                return tags
            }
            case 'sprints':
                return normalizeSprints(options.form.sprints)
            case 'custom_fields':
                return options.buildCustomFields()
            case 'relationships':
                return options.buildRelationships()
            default:
                return undefined
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
            const aKeys = Object.keys(a)
            const bKeys = Object.keys(b)
            return (
                aKeys.length === bKeys.length &&
                aKeys.every(
                    (key) =>
                        key in (b as Record<string, unknown>) &&
                        valuesEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]),
                )
            )
        }
        return false
    }

    function normalizeSprints(values: number[] | undefined | null): number[] {
        if (!Array.isArray(values)) return [] as number[]
        const unique = Array.from(new Set(values.filter((value) => Number.isFinite(value))))
        unique.sort((a, b) => a - b)
        return unique
    }

    function applyServerFieldToForm(field: string, server: TaskDTO) {
        switch (field) {
            case 'title':
                options.form.title = server.title
                break
            case 'status':
                options.form.status = server.status
                break
            case 'priority':
                options.form.priority = server.priority
                break
            case 'task_type':
                options.form.task_type = server.task_type
                break
            case 'reporter':
                options.form.reporter = server.reporter || ''
                break
            case 'assignee':
                options.form.assignee = server.assignee || ''
                break
            case 'due_date':
                options.form.due_date = toDateInputValue(server.due_date)
                break
            case 'effort':
                options.form.effort = server.effort || ''
                break
            case 'description':
                options.form.description = server.description || ''
                break
            case 'tags':
                options.form.tags = [...server.tags]
                break
            case 'sprints':
                options.form.sprints = [...server.sprints]
                break
            case 'custom_fields':
                options.applyTaskCustomFields(server.custom_fields)
                break
            case 'relationships':
                options.applyRelationshipsFromTask(server)
                break
            default:
                break
        }
    }

    const isHeld = (field: string) => options.isFieldHeld?.(field) ?? false

    const applyPatch = async (patch: Record<string, unknown>) => {
        if (!canEdit()) return
        const allowed = Object.fromEntries(Object.entries(patch).filter(([field]) => !isHeld(field)))
        if (!Object.keys(allowed).length) return
        await enqueuePatch(options.task.id, allowed)
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
                if (options.form.reporter.trim() === (options.task.reporter || '')) return
                patch.reporter = options.form.reporter.trim() || null
                break
            case 'assignee':
                if (options.form.assignee.trim() === (options.task.assignee || '')) return
                patch.assignee = options.form.assignee.trim() || null
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
                if (!options.form.description.trim() && !(options.task.description ?? '').trim()) {
                    return
                }
                if (options.form.description === (options.task.description ?? '')) {
                    return
                }
                patch.description = options.form.description || null
                break
            case 'sprints': {
                const current = normalizeSprints(options.task.sprints)
                const next = normalizeSprints(options.form.sprints)
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
        if (!status || isHeld('status')) return
        // DEV-102: re-selecting the task's current effective status (e.g.
        // closing the dropdown on the already-selected option) is a no-op:
        // no request, no success toast. The comparison mirrors the server's
        // ASCII case-folded configured-label matching and deliberately does
        // NOT strip separators — distinct raw statuses such as "Other" vs
        // "__other__" stay a real, server-judged change.
        if (completionStatusKey(status) === completionStatusKey(options.task.status)) return
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
            let created = await options.apiClient.addTask(payload)
            if (!current()) return
            if (options.afterCreate) created = await options.afterCreate(created)
            if (options.announceCreated) options.announceCreated(created)
            else options.showToast(`Created ${created.id}`)
            options.emit('created', created)
            if (current()) options.closePanel()
        } catch (error: unknown) {
            if (!current()) return
            options.showToast(errorMessage(error) || 'Failed to create task')
        } finally {
            if (scope === options.panelGeneration.value) options.submitting.value = false
        }
    }

    const loadTask = async (id: string): Promise<TaskDTO | undefined> => {
        const scope = options.panelGeneration.value
        const request = ++loadGeneration
        const current = () =>
            scope === options.panelGeneration.value && request === loadGeneration && id === options.getTaskId()
        if (!current()) return undefined
        options.ready.value = false
        options.loading.value = true
        options.resetActivity()
        try {
            const data = await options.apiClient.getTask(id)
            if (!current()) return undefined
            if (data.id && data.id !== id && options.onRedirect) {
                // An old ID of a moved task: show the task under its current
                // ID, so edits, events and the URL all use that one.
                options.onRedirect(data.id)
                return undefined
            }
            await options.refreshConfig(projectOf(id) || '')
            if (!current()) return undefined
            applyTaskSnapshot(options.task, data)
            options.applyTask(data)
            await options.loadCommitHistory(id)
            if (!current()) return undefined
            options.ready.value = true
            return data
        } catch (error: unknown) {
            if (!current()) return undefined
            options.showToast(errorMessage(error) || 'Failed to load task')
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
        activeSaves,
    }
}
