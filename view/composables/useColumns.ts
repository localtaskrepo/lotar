import { computed, inject, provide, ref, watch, type InjectionKey, type Ref } from 'vue'

export type BuiltinColKey =
    | 'id'
    | 'title'
    | 'status'
    | 'priority'
    | 'task_type'
    | 'reporter'
    | 'assignee'
    | 'effort'
    | 'tags'
    | 'sprints'
    | 'due_date'
    | 'modified'

export type ColKey = BuiltinColKey | `custom:${string}`

export type FieldOption = { key: ColKey | string; label: string }

const BUILTIN_COLUMNS: BuiltinColKey[] = [
    'id',
    'title',
    'status',
    'priority',
    'task_type',
    'reporter',
    'assignee',
    'effort',
    'tags',
    'sprints',
    'due_date',
    'modified',
]

const DEFAULT_VISIBLE: BuiltinColKey[] = [
    'id',
    'title',
    'status',
    'priority',
    'reporter',
    'assignee',
    'tags',
    'sprints',
    'due_date',
    'modified',
]

const BUILTIN_LABELS: Record<BuiltinColKey, string> = {
    id: 'ID',
    title: 'Title',
    status: 'Status',
    priority: 'Priority',
    task_type: 'Type',
    reporter: 'Reporter',
    assignee: 'Assignee',
    effort: 'Effort',
    tags: 'Tags',
    sprints: 'Sprints',
    due_date: 'Due',
    modified: 'Updated',
}

export interface UseColumnsOptions {
    storagePrefix?: string
    defaultVisible?: (ColKey | string)[]
}

function readJson<T>(key: string): T | null {
    try {
        const raw = typeof localStorage !== 'undefined' ? localStorage.getItem(key) : null
        if (!raw) return null
        return JSON.parse(raw) as T
    } catch {
        return null
    }
}

function writeJson(key: string, value: unknown) {
    try {
        if (typeof localStorage !== 'undefined') {
            localStorage.setItem(key, JSON.stringify(value))
        }
    } catch {
        // ignore quota / availability errors
    }
}

function isCustomKey(key: string): key is `custom:${string}` {
    return key.startsWith('custom:')
}

function isCustomFieldName(name: string): boolean {
    return !!name && name !== '*'
}

function headerLabelFor(key: string): string {
    if (isCustomKey(key)) return key.slice('custom:'.length)
    return BUILTIN_LABELS[key as BuiltinColKey] ?? key
}

/**
 * Extract enabled custom field values from a task for display.
 * `enabledKeys` may contain bare custom field names or `custom:`-prefixed keys
 * (both storage generations); empty values are skipped.
 */
export function customFieldEntriesFromTask(
    task: { custom_fields?: Record<string, unknown> | null },
    enabledKeys: Iterable<string>,
): { key: string; label: string; value: string }[] {
    const custom = task.custom_fields ?? {}
    const out: { key: string; label: string; value: string }[] = []
    for (const enabled of enabledKeys) {
        const name = isCustomKey(enabled) ? enabled.slice('custom:'.length) : enabled
        if (!name || name === '*') continue
        const raw = custom[name]
        if (raw === undefined || raw === null || raw === '') continue
        out.push({ key: name, label: name, value: String(raw) })
    }
    return out
}

// The declared custom-field list comes from the (global) project config, so it is
// shared by every column store in the app: whichever page loads the config first
// publishes it for all of them. The version counter lets every live instance
// re-read its stored selections once the field list becomes known.
const sharedCustomFieldKeys = ref<string[]>([])
const customFieldKeysVersion = ref(0)

const COLUMN_STORE_KEY: InjectionKey<ColumnStore> = Symbol('lotar.columnStore')

/** Share a page-level column store with descendant components (e.g. TaskTable). */
export function provideColumnStore(store: ColumnStore) {
    provide(COLUMN_STORE_KEY, store)
}

/** Returns the nearest provided column store, or null when the caller is standalone. */
export function injectColumnStore(): ColumnStore | null {
    return inject(COLUMN_STORE_KEY, null)
}

export interface ColumnStore {
    showColumnMenu: Ref<boolean>
    columns: Ref<ColKey[]>
    columnOrder: Ref<ColKey[]>
    fieldOptions: import('vue').ComputedRef<FieldOption[]>
    visibleColumns: import('vue').ComputedRef<ColKey[]>
    toggleColumn: (col: ColKey | string, event: Event) => void
    isVisible: (key: ColKey | string) => boolean
    resetColumns: () => void
    headerLabel: (key: ColKey | string) => string
    customFieldKey: (col: ColKey | string) => string | null
    setProjectKey: (key: string) => void
    setCustomFieldKeys: (keys: string[]) => void
}

export function useColumns(options: UseColumnsOptions = {}): ColumnStore {
    const storagePrefix = options.storagePrefix ?? 'lotar.taskTable'
    const defaultVisible: (ColKey | string)[] = options.defaultVisible ?? DEFAULT_VISIBLE
    const customFieldKeys = sharedCustomFieldKeys

    const showColumnMenu = ref(false)
    const projectKey = ref('')
    const columns = ref<ColKey[]>([])
    const columnOrder = ref<ColKey[]>([])
    let loading = false

    function storageKeyFor(pk: string) {
        return pk ? `${storagePrefix}.columns::${pk}` : `${storagePrefix}.columns`
    }
    function storageKeyForOrder(pk: string) {
        return pk ? `${storagePrefix}.columnOrder::${pk}` : `${storagePrefix}.columnOrder`
    }
    function storageKeyForLegacy(pk: string) {
        return pk ? `${storagePrefix}::${pk}` : storagePrefix
    }

    function allColumnsFor(): ColKey[] {
        const customs = customFieldKeys.value
            .filter(isCustomFieldName)
            .map((k) => `custom:${k}` as ColKey)
        return [...BUILTIN_COLUMNS, ...customs]
    }

    function isAllowedKey(key: string): boolean {
        if (!BUILTIN_COLUMNS.includes(key as BuiltinColKey)) {
            if (!isCustomKey(key)) return false
            return customFieldKeys.value.includes(key.slice('custom:'.length))
        }
        return true
    }

    function normalizeVisible(value: unknown): ColKey[] {
        if (value === null || value === undefined) return [...defaultVisible] as ColKey[]
        let entries: unknown[] = []
        if (Array.isArray(value)) {
            entries = value
        } else if (value && typeof value === 'object') {
            // Legacy boolean-map format written by useFieldVisibility stored custom
            // fields under their bare names; map them onto the custom: prefix so
            // those selections survive the migration.
            entries = Object.entries(value as Record<string, unknown>)
                .filter(([, v]) => v === true)
                .map(([k]) => {
                    if (isAllowedKey(k)) return k
                    return isCustomFieldName(k) && customFieldKeys.value.includes(k)
                        ? `custom:${k}`
                        : k
                })
        }
        const seen = new Set<ColKey>()
        const out: ColKey[] = []
        for (const item of entries) {
            if (typeof item !== 'string') continue
            if (!isAllowedKey(item)) continue
            if (seen.has(item as ColKey)) continue
            seen.add(item as ColKey)
            out.push(item as ColKey)
        }
        // A deliberately empty selection (everything hidden) is valid and preserved.
        return out
    }

    function normalizeColumnOrder(value: unknown): ColKey[] {
        const allowed = allColumnsFor()
        const out: ColKey[] = []
        if (Array.isArray(value)) {
            for (const item of value) {
                if (typeof item !== 'string') continue
                if (!isAllowedKey(item)) continue
                if (!out.includes(item as ColKey)) out.push(item as ColKey)
            }
        }
        for (const col of allowed) {
            if (!out.includes(col)) out.push(col)
        }
        return out.length ? out : allowed
    }

    function loadForProject(pk: string) {
        loading = true
        try {
            let stored = readJson<unknown>(storageKeyFor(pk))
            if (stored === null) {
                // Migrate the legacy boolean-map format written by earlier versions.
                stored = readJson<unknown>(storageKeyForLegacy(pk))
            }
            columns.value = normalizeVisible(stored)
            columnOrder.value = normalizeColumnOrder(readJson<unknown>(storageKeyForOrder(pk)))
        } finally {
            loading = false
        }
    }

    function writeForProject() {
        if (loading) return
        writeJson(storageKeyFor(projectKey.value), columns.value)
        writeJson(storageKeyForOrder(projectKey.value), columnOrder.value)
    }

    watch(projectKey, (pk) => loadForProject(pk), { flush: 'sync' })
    // Sync flush matters: the loading guard must still be active when the watcher
    // reacts to assignments made inside loadForProject, otherwise a deferred
    // callback would persist the normalized-before-config state and erase saved
    // custom-field selections.
    watch([columns, columnOrder], writeForProject, { deep: true, flush: 'sync' })
    watch(customFieldKeysVersion, () => loadForProject(projectKey.value), { flush: 'sync' })

    loadForProject(projectKey.value)

    const columnsSet = computed(() => new Set(columns.value))
    const visibleColumns = computed(() => columnOrder.value.filter((c) => columnsSet.value.has(c)))
    const fieldOptions = computed<FieldOption[]>(() =>
        allColumnsFor().map((col) => ({ key: col, label: headerLabelFor(col) })),
    )

    function toggleColumn(col: ColKey | string, event: Event) {
        const checked = Boolean((event.target as HTMLInputElement | null)?.checked)
        const next = new Set(columns.value)
        if (checked) next.add(col as ColKey)
        else next.delete(col as ColKey)
        columns.value = Array.from(next)
    }

    function isVisible(key: ColKey | string): boolean {
        return columnsSet.value.has(key as ColKey)
    }

    function resetColumns() {
        columns.value = [...defaultVisible] as ColKey[]
        columnOrder.value = [...allColumnsFor()]
    }

    function headerLabel(key: ColKey | string): string {
        return headerLabelFor(key)
    }

    function customFieldKey(col: ColKey | string): string | null {
        return isCustomKey(col) ? col.slice('custom:'.length) : null
    }

    function setProjectKey(key: string) {
        if (projectKey.value !== key) {
            projectKey.value = key
        }
    }

    function setCustomFieldKeys(keys: string[]) {
        const next = [...new Set(keys.map((k) => k.trim()).filter(isCustomFieldName))]
        const same =
            next.length === customFieldKeys.value.length &&
            next.every((k, i) => k === customFieldKeys.value[i])
        if (same) return
        customFieldKeys.value = next
        // Stored selections may have been normalized while the custom field list
        // was still unknown; every live store re-reads its raw state now.
        customFieldKeysVersion.value += 1
    }

    return {
        showColumnMenu,
        columns,
        columnOrder,
        fieldOptions,
        visibleColumns,
        toggleColumn,
        isVisible,
        resetColumns,
        headerLabel,
        customFieldKey,
        setProjectKey,
        setCustomFieldKeys,
    }
}
