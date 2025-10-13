import type { Ref } from 'vue'
import { onBeforeUnmount, reactive, ref } from 'vue'
import type { TaskDTO } from '../api/types'

type RelationSuggestion = {
    id: string
    title: string
}

type RelationDef = {
    key: string
    label: string
    placeholder: string
}

type RelationshipsPayload = {
    depends_on: string[]
    blocks: string[]
    related: string[]
    children: string[]
    fixes: string[]
    parent?: string
    duplicate_of?: string
}

interface UseTaskRelationshipsOptions {
    mode: Ref<'create' | 'edit'>
    ready: Ref<boolean>
    suppressWatch: Ref<boolean>
    applyPatch: (patch: Record<string, unknown>) => Promise<void>
    projectForSuggestions: () => string | undefined
    suggestTasks: (query: string, project?: string, limit?: number) => Promise<RelationSuggestion[]>
}

function relationArray(value: unknown): string[] {
    if (Array.isArray(value)) {
        return value.map((item) => String(item))
    }
    return []
}

function relationString(value: unknown): string {
    return typeof value === 'string' ? value : ''
}

function normalizeRelationshipArray(value: unknown): string[] {
    return relationArray(value)
        .map((entry) => entry.trim())
        .filter(Boolean)
}

function relationshipsFromTaskData(data?: TaskDTO | null): RelationshipsPayload {
    const raw = (data?.relationships || {}) as Record<string, unknown>
    const parent = relationString(raw.parent).trim()
    const duplicate = relationString(raw.duplicate_of).trim()
    return {
        depends_on: normalizeRelationshipArray(raw.depends_on),
        blocks: normalizeRelationshipArray(raw.blocks),
        related: normalizeRelationshipArray(raw.related),
        children: normalizeRelationshipArray(raw.children),
        fixes: normalizeRelationshipArray(raw.fixes),
        parent: parent || undefined,
        duplicate_of: duplicate || undefined,
    }
}

function serializeRelationshipsPayload(payload: RelationshipsPayload): string {
    const result: Record<string, unknown> = {
        depends_on: payload.depends_on,
        blocks: payload.blocks,
        related: payload.related,
        children: payload.children,
        fixes: payload.fixes,
    }
    if (payload.parent) {
        result.parent = payload.parent
    }
    if (payload.duplicate_of) {
        result.duplicate_of = payload.duplicate_of
    }
    return JSON.stringify(result)
}

export function useTaskRelationships(options: UseTaskRelationshipsOptions) {
    const relationships = reactive<Record<string, string>>({
        depends_on: '',
        blocks: '',
        related: '',
        children: '',
        fixes: '',
        parent: '',
        duplicate_of: '',
    })

    const relationshipsBaseline = ref('')

    const relationDefs: RelationDef[] = [
        { key: 'depends_on', label: 'Depends on', placeholder: 'IDs comma separated' },
        { key: 'blocks', label: 'Blocks', placeholder: 'IDs comma separated' },
        { key: 'related', label: 'Related', placeholder: 'IDs comma separated' },
        { key: 'children', label: 'Children', placeholder: 'IDs comma separated' },
        { key: 'fixes', label: 'Fixes', placeholder: 'IDs comma separated' },
        { key: 'parent', label: 'Parent', placeholder: 'ID' },
        { key: 'duplicate_of', label: 'Duplicate of', placeholder: 'ID' },
    ]

    const relationSuggestions = reactive<Record<string, RelationSuggestion[]>>(
        Object.fromEntries(relationDefs.map(({ key }) => [key, []])) as Record<string, RelationSuggestion[]>,
    )

    const relationActiveIndex = reactive<Record<string, number>>(
        Object.fromEntries(relationDefs.map(({ key }) => [key, -1])) as Record<string, number>,
    )

    const relationPendingRequest = reactive<Record<string, number>>(
        Object.fromEntries(relationDefs.map(({ key }) => [key, 0])) as Record<string, number>,
    )

    let relationSuggestTimer: ReturnType<typeof setTimeout> | null = null
    let relationSuggestSeq = 0

    function splitCsv(value: string) {
        return value
            .split(',')
            .map((s) => s.trim())
            .filter(Boolean)
    }

    function relationLastToken(field: string) {
        const value = relationships[field] || ''
        const parts = value.split(',')
        return parts[parts.length - 1].trim()
    }

    function clearRelationSuggestions(field?: string) {
        if (field) {
            relationSuggestions[field] = []
            relationActiveIndex[field] = -1
            relationPendingRequest[field] = 0
        } else {
            Object.keys(relationSuggestions).forEach((key) => {
                relationSuggestions[key] = []
                relationActiveIndex[key] = -1
                relationPendingRequest[key] = 0
            })
        }
        if (relationSuggestTimer) {
            clearTimeout(relationSuggestTimer)
            relationSuggestTimer = null
        }
    }

    async function commitRelationships() {
        if (options.mode.value !== 'edit' || !options.ready.value || options.suppressWatch.value) return
        const payload = buildRelationships()
        const serialized = serializeRelationshipsPayload(payload)
        if (serialized === relationshipsBaseline.value) {
            return
        }
        await options.applyPatch({ relationships: payload })
    }

    function buildRelationships(): RelationshipsPayload {
        return {
            depends_on: splitCsv(relationships.depends_on),
            blocks: splitCsv(relationships.blocks),
            related: splitCsv(relationships.related),
            children: splitCsv(relationships.children),
            fixes: splitCsv(relationships.fixes),
            parent: relationships.parent.trim() || undefined,
            duplicate_of: relationships.duplicate_of.trim() || undefined,
        }
    }

    function snapshotRelationshipsBaselineFromTask(data?: TaskDTO | null) {
        relationshipsBaseline.value = serializeRelationshipsPayload(relationshipsFromTaskData(data))
    }

    function snapshotRelationshipsBaselineFromInputs() {
        relationshipsBaseline.value = serializeRelationshipsPayload(buildRelationships())
    }

    function resetRelationships() {
        relationDefs.forEach(({ key }) => {
            relationships[key] = ''
        })
        clearRelationSuggestions()
    }

    function applyRelationshipsFromTask(data: TaskDTO | null | undefined) {
        resetRelationships()
        const relData = (data?.relationships || {}) as Record<string, unknown>
        relationships.depends_on = relationArray(relData.depends_on).join(', ')
        relationships.blocks = relationArray(relData.blocks).join(', ')
        relationships.related = relationArray(relData.related).join(', ')
        relationships.children = relationArray(relData.children).join(', ')
        relationships.fixes = relationArray(relData.fixes).join(', ')
        relationships.parent = relationString(relData.parent)
        relationships.duplicate_of = relationString(relData.duplicate_of)
    }

    function updateRelationshipField(field: string, value: string) {
        relationships[field] = value
    }

    function handleRelationshipBlur(field: string) {
        clearRelationSuggestions(field)
        commitRelationships()
    }

    function pickRelation(field: string, id: string) {
        if (field === 'parent' || field === 'duplicate_of') {
            relationships[field] = id
        } else {
            const currentValue = relationships[field] || ''
            let entries = splitCsv(currentValue)
            const lastToken = relationLastToken(field)
            const hasTrailingSeparator = /,\s*$/.test(currentValue)
            if (lastToken && !hasTrailingSeparator && entries.length) {
                entries = entries.slice(0, entries.length - 1)
            }
            const alreadyPresent = entries.some((existing: string) => existing.toLowerCase() === id.toLowerCase())
            if (!alreadyPresent) {
                entries.push(id)
            }
            relationships[field] = entries.join(', ')
        }
        clearRelationSuggestions(field)
        commitRelationships()
    }

    function onRelationKey(field: string, event: KeyboardEvent) {
        const list = relationSuggestions[field] || []
        if (!list.length) return
        if (event.key === 'ArrowDown') {
            event.preventDefault()
            relationActiveIndex[field] = (relationActiveIndex[field] + 1) % list.length
        } else if (event.key === 'ArrowUp') {
            event.preventDefault()
            relationActiveIndex[field] = (relationActiveIndex[field] - 1 + list.length) % list.length
        } else if (event.key === 'Enter') {
            event.preventDefault()
            const idx = relationActiveIndex[field]
            const choice = idx >= 0 ? list[idx] : list[0]
            if (choice) {
                pickRelation(field, choice.id)
            }
        } else if (event.key === 'Escape') {
            clearRelationSuggestions(field)
        }
    }

    function onRelationInput(field: string) {
        if (relationSuggestTimer) {
            clearTimeout(relationSuggestTimer)
            relationSuggestTimer = null
        }
        relationSuggestTimer = setTimeout(async () => {
            const token = relationLastToken(field)
            const query = token.trim()
            if (!query) {
                clearRelationSuggestions(field)
                return
            }
            const isNumeric = /^[0-9]+$/.test(query)
            const project = options.projectForSuggestions()
            const requestId = ++relationSuggestSeq
            relationPendingRequest[field] = requestId
            try {
                const candidates: string[] = [query]
                if (isNumeric && project) {
                    candidates.push(`${project}-${query}`)
                }
                let list: RelationSuggestion[] = []
                for (const candidate of candidates) {
                    const result = await options.suggestTasks(candidate, project || undefined, 12)
                    if (relationPendingRequest[field] !== requestId) {
                        return
                    }
                    list = result
                    if (list.length) {
                        break
                    }
                }
                if (relationPendingRequest[field] !== requestId) {
                    return
                }
                relationSuggestions[field] = list
                relationActiveIndex[field] = list.length ? 0 : -1
            } catch {
                if (relationPendingRequest[field] === requestId) {
                    relationSuggestions[field] = []
                    relationActiveIndex[field] = -1
                }
            }
        }, 160)
    }

    onBeforeUnmount(() => {
        if (relationSuggestTimer) {
            clearTimeout(relationSuggestTimer)
            relationSuggestTimer = null
        }
    })

    return {
        relationDefs,
        relationships,
        relationSuggestions,
        relationActiveIndex,
        resetRelationships,
        buildRelationships,
        snapshotRelationshipsBaselineFromTask,
        snapshotRelationshipsBaselineFromInputs,
        applyRelationshipsFromTask,
        updateRelationshipField,
        handleRelationshipBlur,
        onRelationInput,
        onRelationKey,
        pickRelation,
        commitRelationships,
    }
}
