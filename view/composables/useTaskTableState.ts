import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import type { TaskDTO } from '../api/types'
import { storageGetJson, storageSetJson } from '../utils/storage'
import { formatRelativeTime, formatTaskDate, isTaskOverdue } from '../utils/date'
import type { TaskTouch } from './useActivity'
import { injectColumnStore, useColumns, type ColKey } from './useColumns'
import { numericOf, projectOf } from '../utils/text'
import { colKeyToSortBy, sortTasks } from '../utils/taskSort'

export interface TaskTableProps {
    tasks: TaskDTO[]
    loading?: boolean
    statuses?: string[]
    selectable?: boolean
    selectedIds?: string[]
    projectKey?: string
    bulk?: boolean
    showBulkControls?: boolean
    touches?: Record<string, TaskTouch>
    sprintLookup?: Record<number, { label: string; state?: string }>
    sprintOptions?: Array<{ value: string; label: string }>
    sprintSelection?: string
    allowClosedSprint?: boolean
    hasSprints?: boolean
    hasMissingSprints?: boolean
    missingSprintMessage?: string
    sprintsLoading?: boolean
    /**
     * Controlled sort state. When provided, the table renders it as-is (the
     * caller owns ordering, typically server-side), header clicks emit
     * `update:sort` instead of mutating local state, and only columns with a
     * server sort key offer the sort affordance.
     */
    sort?: { key: ColKey | string | null; dir: 'asc' | 'desc' } | null
}

export interface TaskTableEmit {
    (event: 'update:sort', value: { key: ColKey | null; dir: 'asc' | 'desc' }): void
    (event: 'open', id: string): void
    (event: 'delete', id: string): void
    (event: 'update-tags', payload: { id: string; tags: string[] }): void
    (event: 'edit-tags', id: string): void
    (event: 'set-status', payload: { id: string; status: string }): void
    (event: 'assign', id: string): void
    (event: 'unassign', id: string): void
    (event: 'sprint-add', id: string): void
    (event: 'sprint-remove', id: string): void
    (event: 'update:selectedIds', value: string[]): void
    (event: 'update:bulk', value: boolean): void
    (event: 'bulk-assign'): void
    (event: 'bulk-unassign'): void
    (event: 'add'): void
    (event: 'update:sprint-selection', value: string): void
    (event: 'update:allow-closed-sprint', value: boolean): void
    (event: 'bulk-sprint-add'): void
    (event: 'bulk-sprint-remove'): void
    (event: 'bulk-delete'): void
}

const SORT_KEY = 'lotar.taskTable.sort'

export function useTaskTableState(props: Readonly<TaskTableProps>, emit: TaskTableEmit) {
    // Reuse the page-provided column store when mounted inside a page (TasksList)
    // so the page-level Columns menu and the table share one source of truth;
    // standalone usage falls back to a private store.
    const columnStore = injectColumnStore() ?? useColumns()
    if (props.projectKey) {
        columnStore.setProjectKey(props.projectKey)
    }
    watch(
        () => props.projectKey,
        (key) => {
            if (key) columnStore.setProjectKey(key)
        },
    )

    const columnOrder = columnStore.columnOrder
    const visibleColumns = columnStore.visibleColumns
    const showColumnMenu = columnStore.showColumnMenu

    function headerLabel(key: ColKey) {
        return columnStore.headerLabel(key)
    }

    const rootRef = ref<HTMLElement | null>(null)

    function onDocClick(event: MouseEvent) {
        const root = rootRef.value
        const target = event.target as Node | null
        if (!root || !target) return
        if (root.contains(target)) return
        showColumnMenu.value = false
        rowMenu.value = {}
    }

    function onDocKey(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            showColumnMenu.value = false
            rowMenu.value = {}
        }
    }

    onMounted(() => {
        if (typeof window !== 'undefined') {
            window.addEventListener('click', onDocClick)
            window.addEventListener('keydown', onDocKey)
        }
    })

    onUnmounted(() => {
        if (typeof window !== 'undefined') {
            window.removeEventListener('click', onDocClick)
            window.removeEventListener('keydown', onDocKey)
        }
    })

    function sortKey() {
        return props.projectKey ? `${SORT_KEY}::${props.projectKey}` : SORT_KEY
    }

    function readStoredSort(): { key: ColKey | null; dir: 'asc' | 'desc' } {
        return storageGetJson<{ key: ColKey | null; dir: 'asc' | 'desc' }>(sortKey())
            ?? storageGetJson<{ key: ColKey | null; dir: 'asc' | 'desc' }>(SORT_KEY)
            ?? { key: null, dir: 'desc' }
    }

    // Internal sort state (standalone usage). When the page passes the
    // controlled `sort` prop, that value wins and local state is untouched.
    const internalSort = reactive<{ key: ColKey | null; dir: 'asc' | 'desc' }>(readStoredSort())
    let loadingSort = false

    const isControlledSort = computed(() => props.sort !== undefined && props.sort !== null)

    const sort = computed<{ key: ColKey | string | null; dir: 'asc' | 'desc' }>(() =>
        isControlledSort.value ? (props.sort as { key: ColKey | null; dir: 'asc' | 'desc' }) : internalSort,
    )

    // Reload the stored sort whenever the project changes so one project's
    // sort never leaks into (or gets persisted under) another project's key.
    watch(
        () => props.projectKey,
        () => {
            loadingSort = true
            try {
                const stored = readStoredSort()
                internalSort.key = stored.key
                internalSort.dir = stored.dir
            } finally {
                loadingSort = false
            }
        },
    )

    watch(
        internalSort,
        (value) => {
            if (!loadingSort) storageSetJson(sortKey(), value)
        },
        { deep: true },
    )

    function setSort(key: ColKey, dir: 'asc' | 'desc') {
        if (sort.value.key === key && sort.value.dir === dir) {
            return
        }
        if (isControlledSort.value) {
            emit('update:sort', { key, dir })
            return
        }
        internalSort.key = key
        internalSort.dir = dir
    }

    function isSortableCol(key: ColKey | string): boolean {
        if (!isControlledSort.value) return true
        return colKeyToSortBy(key) !== null
    }

    function onSort(key: ColKey) {
        if (!isSortableCol(key)) return
        if (sort.value.key === key) {
            const nextDir = sort.value.dir === 'asc' ? 'desc' : 'asc'
            setSort(key, nextDir)
        } else {
            setSort(key, 'asc')
        }
    }

    const rowMenu = ref<Record<string, boolean>>({})

    function toggleRowMenu(id: string) {
        rowMenu.value[id] = !rowMenu.value[id]
    }

    function closeRowMenu(id: string) {
        rowMenu.value[id] = false
    }

    function isRowMenuOpen(id: string) {
        return !!rowMenu.value[id]
    }

    const filtered = computed(() => props.tasks || [])

    // Standalone tables sort through the SHARED contract comparator for every
    // column with a server sort key, so header sorts and exports can never
    // disagree. (The tasks page renders the authoritative server order
    // instead of sorting locally.)
    const sorted = computed(() => {
        const base = filtered.value
        if (isControlledSort.value) return base
        const key = sort.value.key
        if (!key) return base
        const serverSortBy = colKeyToSortBy(key)
        if (!serverSortBy) return base
        return sortTasks(base, serverSortBy, sort.value.dir)
    })

    const touchesMap = computed(() => props.touches ?? ({} as Record<string, TaskTouch>))

    const selected = ref<string[]>(props.selectedIds ? [...props.selectedIds] : [])

    watch(
        () => props.selectedIds,
        (value) => {
            selected.value = value ? [...value] : []
        },
        { flush: 'sync' },
    )

    const visibleIds = computed(() => sorted.value.map((task) => task.id))
    const allSelected = computed(() => visibleIds.value.length > 0 && visibleIds.value.every((id) => selected.value.includes(id)))
    const selectAllRef = ref<HTMLInputElement | null>(null)

    const indeterminate = computed(() => {
        const visible = new Set(visibleIds.value)
        const hasAnyVisibleSelected = selected.value.some((id) => visible.has(id))
        return hasAnyVisibleSelected && !allSelected.value
    })

    watch(
        [selected, visibleIds],
        () => {
            if (selectAllRef.value) {
                selectAllRef.value.indeterminate = indeterminate.value
            }
        },
        { deep: true },
    )

    function isSelected(id: string) {
        return selected.value.includes(id)
    }

    function toggleOne(id: string, event: Event) {
        if (props.loading || !visibleIds.value.includes(id)) return
        const checked = (event.target as HTMLInputElement).checked
        const set = new Set(selected.value)
        if (checked) set.add(id)
        else set.delete(id)
        selected.value = Array.from(set)
        emit('update:selectedIds', [...selected.value])
    }

    function toggleAll(event: Event) {
        if (props.loading) return
        const checked = (event.target as HTMLInputElement).checked
        const visible = visibleIds.value
        if (checked) {
            selected.value = Array.from(new Set([...selected.value, ...visible]))
        } else {
            const drop = new Set(visible)
            selected.value = selected.value.filter((id) => !drop.has(id))
        }
        emit('update:selectedIds', [...selected.value])
    }

    function onToggleBulk(event: Event) {
        emit('update:bulk', (event.target as HTMLInputElement).checked)
    }


    function fmtDate(value: string) {
        const formatted = formatTaskDate(value)
        if (formatted) return formatted
        return value
    }

    function fmtDateTime(value: string) {
        try {
            return new Date(value).toLocaleString()
        } catch {
            return value
        }
    }


    function touchBadge(touch: TaskTouch) {
        switch (touch.kind) {
            case 'created':
                return 'New'
            case 'updated':
                return 'Updated'
            case 'deleted':
                return 'Removed'
            default:
                return 'Activity'
        }
    }

    function isOverdue(task: TaskDTO) {
        return isTaskOverdue(task)
    }

    return {
        columnOrder,
        fieldOptions: columnStore.fieldOptions,
        visibleColumns,
        headerLabel,
        isVisible: columnStore.isVisible,
        showColumnMenu,
        toggleColumn: columnStore.toggleColumn,
        resetColumns: columnStore.resetColumns,
        customFieldKey: columnStore.customFieldKey,
        rootRef,
        sort,
        onSort,
        isControlledSort,
        isSortableCol,
        rowMenu,
        toggleRowMenu,
        closeRowMenu,
        isRowMenuOpen,
        sorted,
        touchesMap,
        selected,
        allSelected,
        selectAllRef,
        indeterminate,
        isSelected,
        toggleOne,
        toggleAll,
        onToggleBulk,
        projectOf,
        numericOf,
        fmtDate,
        fmtDateTime,
        relativeTime: formatRelativeTime,
        touchBadge,
        isOverdue,
        setSort,
    }
}
