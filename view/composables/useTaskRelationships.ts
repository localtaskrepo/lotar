import { onBeforeUnmount, onMounted, reactive, watch, type Ref } from 'vue'
import type { TaskDTO, TaskRelationships } from '../api/types'
import {
    relationshipCategories,
    relationshipEntries,
    singletonRelationship,
    type RelationshipCategory,
    type RelationshipIntent,
} from '../utils/relationships'
import { useSse } from './useSse'

interface UseTaskRelationshipsOptions {
    mode: Ref<'create' | 'edit'>
    ready: Ref<boolean>
    suppressWatch: Ref<boolean>
    scope: () => string
    taskId: () => string
    editable: () => boolean
    execute: (intent: RelationshipIntent) => Promise<void>
    reconcile: () => Promise<TaskDTO | undefined>
}

export function useTaskRelationships(options: UseTaskRelationshipsOptions) {
    // String fields remain the create-draft/lifecycle boundary, never an edit
    // request payload. Existing task YAML retains its initial-map shape.
    const relationships = reactive<Record<RelationshipCategory, string>>({
        depends_on: '',
        blocks: '',
        related: '',
        children: '',
        fixes: '',
        parent: '',
        duplicate_of: '',
    })
    let generation = 0,
        alive = true,
        pending = false
    let stream: ReturnType<typeof useSse> | undefined
    watch(
        () => [options.scope(), options.ready.value],
        () => {
            generation += 1
            pending = false
        },
        { flush: 'sync' },
    )
    onBeforeUnmount(() => {
        alive = false
        generation += 1
        stream?.close()
    })
    async function refreshRelationships() {
        if (pending || options.mode.value !== 'edit' || !options.ready.value) return
        const request = generation
        try {
            const updated = await options.reconcile()
            if (alive && request === generation && updated) applyRelationshipsFromTask(updated)
        } catch {
            /* An operation or explicit reload reports actionable errors. */
        }
    }
    onMounted(() => {
        if (typeof EventSource === 'undefined') return
        stream = useSse(
            '/api/events',
            {},
            {
                onReconnect: () => {
                    void refreshRelationships()
                },
            },
        )
        for (const event of ['task_updated', 'task_deleted', 'task_restored'])
            stream.on(event, (message) => {
                try {
                    const payload = JSON.parse(message.data)
                    if ((payload.id || payload.task?.id) === options.taskId()) void refreshRelationships()
                } catch {
                    /* Ignore malformed events; reconnect reconciles current state. */
                }
            })
    })
    function resetRelationships() {
        for (const { key } of relationshipCategories) relationships[key] = ''
    }
    function buildRelationships(): TaskRelationships {
        return Object.fromEntries(
            relationshipCategories.map(({ key }) => {
                const values = relationshipEntries(relationships, key)
                return [key, singletonRelationship(key) ? values[0] : values]
            }),
        ) as TaskRelationships
    }
    function applyRelationshipsFromTask(data?: TaskDTO | null) {
        resetRelationships()
        for (const { key } of relationshipCategories)
            relationships[key] = relationshipEntries(data?.relationships || {}, key).join(', ')
    }
    function changeDraft(intent: RelationshipIntent) {
        const entries = relationshipEntries(relationships, intent.kind)
        if (intent.action === 'change_kind' && intent.from_kind === intent.kind) return
        if (intent.action === 'remove') {
            relationships[intent.kind] = entries.filter((id) => id !== intent.target).join(', ')
            return
        }
        if (
            singletonRelationship(intent.kind) &&
            entries[0] &&
            entries[0] !== intent.target &&
            intent.expected_current !== entries[0]
        ) {
            throw new Error('Current relationship changed. Confirm the replacement again.')
        }
        if (intent.action === 'change_kind' && intent.from_kind) {
            relationships[intent.from_kind] = relationshipEntries(relationships, intent.from_kind)
                .filter((id) => id !== intent.target)
                .join(', ')
        }
        relationships[intent.kind] = singletonRelationship(intent.kind)
            ? intent.target
            : [...new Set([...entries, intent.target])].join(', ')
    }
    async function mutateRelationship(intent: RelationshipIntent) {
        if (!options.ready.value || options.suppressWatch.value || !options.editable() || pending)
            throw new Error('Task is not ready for relationship changes.')
        if (intent.action === 'change_kind' && intent.from_kind === intent.kind) return
        if (options.mode.value === 'create') {
            changeDraft(intent)
            return
        }
        const request = generation
        pending = true
        let failure: unknown
        try {
            await options.execute(intent)
        } catch (cause) {
            failure = cause
        }
        if (!alive || request !== generation) return
        try {
            const updated = await options.reconcile()
            if (alive && request === generation && updated) applyRelationshipsFromTask(updated)
        } catch (cause) {
            if (!failure)
                failure = new Error(
                    'Relationship request completed, but reconciliation failed. Reload relationships before retrying.',
                )
        } finally {
            if (request === generation) pending = false
        }
        if (alive && request === generation && failure) throw failure
    }
    return { relationships, resetRelationships, buildRelationships, applyRelationshipsFromTask, mutateRelationship }
}
