import type { Ref } from 'vue'
import { computed, nextTick, reactive, watch } from 'vue'
import type { TaskDTO } from '../../api/types'
import { toDateInputValue } from '../../utils/date'
import { applyTaskSnapshot, isDeletedTask } from '../../utils/text'
import type { TaskPanelFormState } from './useTaskPanelPersistence'

/** Task fields the panel edits and keeps in step with outside changes. */
export type ExternalSyncField =
    | 'title'
    | 'description'
    | 'status'
    | 'priority'
    | 'task_type'
    | 'reporter'
    | 'assignee'
    | 'due_date'
    | 'effort'
    | 'tags'
    | 'sprints'
    | 'releases'
    | 'custom_fields'

const FIELD_LABELS: Record<ExternalSyncField, string> = {
    title: 'Title',
    description: 'Description',
    status: 'Status',
    priority: 'Priority',
    task_type: 'Type',
    reporter: 'Reporter',
    assignee: 'Assignee',
    due_date: 'Due date',
    effort: 'Effort',
    tags: 'Tags',
    sprints: 'Sprints',
    releases: 'Releases',
    custom_fields: 'Custom fields',
}

const FIELDS = Object.keys(FIELD_LABELS) as ExternalSyncField[]

export interface ExternalConflict {
    field: ExternalSyncField
    label: string
}

interface UseTaskPanelExternalSyncOptions {
    /** The panel shows an editable, loaded task and no save is running. */
    active: () => boolean
    /** The shared store's entity for the open task (reactive). */
    entity: () => TaskDTO | undefined
    task: TaskDTO
    form: TaskPanelFormState
    suppressWatch: Ref<boolean>
    /** Text typed into a custom reporter/assignee input but not yet committed. */
    pendingOwner: (field: 'reporter' | 'assignee') => string | undefined
    customFieldsDirty: () => boolean
    applyCustomFields: (values: Record<string, unknown>) => void
    applySprints: (task: TaskDTO) => void
    /** Refresh derived panel state (ownership controls, history, known tags). */
    afterApply: (task: TaskDTO) => void
    /** Save the user's value for `field` (used by "keep mine"). */
    save: (field: ExternalSyncField) => Promise<void>
}

function compareModified(next: string | undefined, current: string | undefined): number {
    const a = next ? Date.parse(next) : NaN
    const b = current ? Date.parse(current) : NaN
    if (Number.isNaN(a) || Number.isNaN(b)) return 0
    return a < b ? -1 : a > b ? 1 : 0
}

function normalizeSprints(values: unknown): number[] {
    if (!Array.isArray(values)) return []
    return Array.from(new Set(values.map(Number).filter((value) => Number.isFinite(value)))).sort((a, b) => a - b)
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
                    Object.prototype.hasOwnProperty.call(b, key) &&
                    valuesEqual((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]),
            )
        )
    }
    return false
}

/**
 * DEV-263: keep an open task panel in step with outside edits (CLI, agents,
 * other tabs) that reach the shared task store.
 *
 * When the store holds a newer `modified` than the panel's snapshot, fields
 * the user has not changed take the new value; a field focused but still
 * unchanged is refreshed too, so a later blur saves on top of the newest
 * value. A field the user has changed that also changed elsewhere becomes a
 * conflict: the user's text stays, autosave holds it back, and the panel asks
 * whether to reload the other value or keep (and save) theirs.
 *
 * Saves carry the snapshot's `modified` as a precondition (DEV-301): a save
 * that raced an outside change is refused by the server and merged here
 * instead of overwriting it.
 */
export function useTaskPanelExternalSync(options: UseTaskPanelExternalSyncOptions) {
    const conflicts = reactive(new Set<ExternalSyncField>())

    function serverValue(field: ExternalSyncField, task: TaskDTO): unknown {
        switch (field) {
            case 'title':
                return task.title || ''
            case 'description':
                return task.description || ''
            case 'status':
            case 'priority':
            case 'task_type':
            case 'reporter':
            case 'assignee':
            case 'effort':
                return task[field] || ''
            case 'due_date':
                return toDateInputValue(task.due_date)
            case 'tags':
                return [...task.tags]
            case 'sprints':
                return normalizeSprints(task.sprints)
            case 'releases':
                return normalizeSprints(task.releases)
            case 'custom_fields':
                return task.custom_fields
        }
    }

    function formValue(field: ExternalSyncField): unknown {
        const form = options.form
        switch (field) {
            case 'title':
                return (form.title || '').trim()
            case 'description':
                return form.description || ''
            case 'reporter':
            case 'assignee':
                return options.pendingOwner(field) ?? form[field].trim()
            case 'tags':
                return form.tags.map((tag) => (tag || '').trim()).filter((tag) => tag.length > 0)
            case 'sprints':
                return normalizeSprints(form.sprints)
            case 'releases':
                return normalizeSprints(form.releases)
            case 'custom_fields':
                return undefined
            case 'status':
            case 'priority':
            case 'task_type':
            case 'due_date':
            case 'effort':
                return form[field] || ''
        }
    }

    /** The user's value differs from the panel's last known server value. */
    function isDirty(field: ExternalSyncField): boolean {
        if (field === 'custom_fields') return options.customFieldsDirty()
        return !valuesEqual(formValue(field), serverValue(field, options.task))
    }

    function applyField(field: ExternalSyncField, source: TaskDTO) {
        const form = options.form
        switch (field) {
            case 'title':
                form.title = source.title || ''
                break
            case 'description':
                form.description = source.description || ''
                break
            case 'status':
            case 'priority':
            case 'task_type':
            case 'reporter':
            case 'assignee':
            case 'effort':
                form[field] = source[field] || ''
                break
            case 'due_date':
                form.due_date = toDateInputValue(source.due_date)
                break
            case 'tags':
                form.tags = [...source.tags]
                break
            case 'sprints':
                options.applySprints(source)
                break
            case 'releases':
                form.releases = Array.isArray(source.releases) ? [...source.releases] : []
                break
            case 'custom_fields':
                options.applyCustomFields(JSON.parse(JSON.stringify(source.custom_fields)) as Record<string, unknown>)
                break
        }
    }

    function withSuppressedWatch(run: () => void) {
        options.suppressWatch.value = true
        try {
            run()
        } finally {
            void nextTick(() => {
                options.suppressWatch.value = false
            })
        }
    }

    /**
     * Take a server version of the task into the panel: fields the user has
     * not changed take the server value, changed ones that also differ there
     * become conflicts, and the snapshot moves to the server version.
     */
    function merge(source: TaskDTO) {
        const next = JSON.parse(JSON.stringify(source)) as TaskDTO
        const apply: ExternalSyncField[] = []
        for (const field of FIELDS) {
            if (valuesEqual(serverValue(field, next), serverValue(field, options.task))) continue
            if (!isDirty(field)) {
                apply.push(field)
                continue
            }
            // The user typed exactly what arrived: nothing to decide.
            if (field !== 'custom_fields' && valuesEqual(formValue(field), serverValue(field, next))) continue
            conflicts.add(field)
        }
        withSuppressedWatch(() => {
            for (const field of apply) applyField(field, next)
            applyTaskSnapshot(options.task, next)
            options.afterApply(next)
        })
    }

    function sync() {
        if (!options.active()) return
        const entity = options.entity()
        if (!entity || entity.id !== options.task.id) return
        // Delete/restore has its own authoritative reload (DEV-92).
        if (isDeletedTask(entity) !== isDeletedTask(options.task)) return
        if (compareModified(entity.modified, options.task.modified) <= 0) return
        merge(entity)
    }

    /**
     * DEV-301: a response from another endpoint (archive, attachments,
     * references) is the current server state. Merge it like an outside
     * change instead of replacing the snapshot, which would make the form's
     * older values look like the user's own edits.
     */
    function mergeServerTask(source: TaskDTO) {
        if (source.id !== options.task.id) return
        if (isDeletedTask(source) !== isDeletedTask(options.task)) {
            applyTaskSnapshot(options.task, source)
            return
        }
        merge(source)
    }

    /** Fields the user has changed and not saved, except those in `except`. */
    function unsavedFields(except: Iterable<string>): ExternalSyncField[] {
        const skip = new Set(except)
        return FIELDS.filter((field) => field !== 'custom_fields' && !skip.has(field) && isDirty(field))
    }

    /**
     * DEV-301: remember the form values of unsaved fields (other than the
     * ones being saved) so a save response can be applied without wiping
     * text typed elsewhere meanwhile. Call before the snapshot changes.
     */
    function preserveUnsaved(saving: Iterable<string>): () => void {
        const kept = unsavedFields(saving).map(
            (field) => [field, JSON.parse(JSON.stringify(options.form[field as keyof TaskPanelFormState]))] as const,
        )
        return () => {
            if (!kept.length) return
            withSuppressedWatch(() => {
                for (const [field, value] of kept) {
                    ;(options.form as unknown as Record<string, unknown>)[field] = value
                }
            })
        }
    }

    watch(
        () => {
            const entity = options.entity()
            return [options.active(), entity?.modified, entity?.deleted_at] as const
        },
        () => sync(),
    )

    // A conflict ends once the user's value matches the panel's server value
    // again (they typed the same text, or a save response replaced it).
    watch(
        () => Array.from(conflicts).filter((field) => !isDirty(field)),
        (resolved) => {
            if (!resolved.length) return
            for (const field of resolved) conflicts.delete(field)
            // Custom fields compare against the editor's own baseline, which a
            // conflict leaves at the older value: catch it up to the task.
            if (resolved.includes('custom_fields')) {
                withSuppressedWatch(() => applyField('custom_fields', options.task))
            }
        },
    )

    const externalConflicts = computed<ExternalConflict[]>(() =>
        FIELDS.filter((field) => conflicts.has(field)).map((field) => ({ field, label: FIELD_LABELS[field] })),
    )

    /** Replace the user's conflicting values with the newer outside values. */
    function reloadConflicts() {
        const fields = Array.from(conflicts)
        conflicts.clear()
        withSuppressedWatch(() => {
            for (const field of fields) applyField(field, options.task)
            options.afterApply(options.task)
        })
    }

    /** Keep the user's values and save them over the outside change. */
    async function keepMineConflicts() {
        const fields = Array.from(conflicts)
        conflicts.clear()
        for (const field of fields) await options.save(field)
    }

    return {
        externalConflicts,
        isFieldHeld: (field: string) => conflicts.has(field as ExternalSyncField),
        reloadConflicts,
        keepMineConflicts,
        resetExternalSync: () => conflicts.clear(),
        syncFromStore: sync,
        mergeServerTask,
        preserveUnsaved,
        unsavedFields,
    }
}
