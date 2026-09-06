import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import type { TaskDTO } from '../api/types'
import { storageGetJson, storageSetJson } from '../utils/storage'
import { formatRelativeTime, formatTaskDate, isTaskOverdue, parseTaskDateToMillis } from '../utils/date'
import type { TaskTouch } from './useActivity'
import { injectColumnStore, useColumns, type ColKey } from './useColumns'
import { numericOf, projectOf } from '../utils/text'

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
}

export interface TaskTableEmit {
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

    const sort = reactive<{ key: ColKey | null; dir: 'asc' | 'desc' }>(
        storageGetJson<{ key: ColKey | null; dir: 'asc' | 'desc' }>(sortKey())
            ?? storageGetJson<{ key: ColKey | null; dir: 'asc' | 'desc' }>(SORT_KEY)
            ?? { key: null, dir: 'desc' },
    )

    function setSort(key: ColKey, dir: 'asc' | 'desc') {
        if (sort.key === key && sort.dir === dir) {
            return
        }
        sort.key = key
        sort.dir = dir
    }

    function onSort(key: ColKey) {
        if (sort.key === key) {
            const nextDir = sort.dir === 'asc' ? 'desc' : 'asc'
            setSort(key, nextDir)
        } else {
            setSort(key, 'asc')
        }
    }

    watch(
        sort,
        (value) => {
            storageSetJson(sortKey(), value)
        },
        { deep: true },
    )

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

    const sorted = computed(() => {
        const base = filtered.value
        if (!sort.key) return base
        const arr = [...base]
        const key = sort.key
        const dir = sort.dir === 'asc' ? 1 : -1
        arr.sort((a, b) => {
            const av = (a as any)[key]
            const bv = (b as any)[key]
            if (av == null && bv == null) return 0
            if (av == null) return -1 * dir
            if (bv == null) return 1 * dir
            if (key === 'sprints') {
                const toKey = (value: unknown) =>
                    Array.isArray(value) && value.length > 0 ? value.join(',') : ''
                return toKey(av).localeCompare(toKey(bv)) * dir
            }
            if (key === 'due_date' || key === 'modified') {
                const at = parseTaskDateToMillis(av as any) ?? 0
                const bt = parseTaskDateToMillis(bv as any) ?? 0
                return (at - bt) * dir
            }
            return String(av).localeCompare(String(bv)) * dir
        })
        return arr
    })

    const touchesMap = computed(() => props.touches ?? ({} as Record<string, TaskTouch>))

    const selected = ref<string[]>(props.selectedIds ? [...props.selectedIds] : [])
    const suppressSelectedEmit = ref(false)

    watch(
        () => props.selectedIds,
        (value) => {
            suppressSelectedEmit.value = true
            selected.value = value ? [...value] : []
            nextTick(() => {
                suppressSelectedEmit.value = false
            })
        },
    )

    watch(selected, (value) => {
        if (suppressSelectedEmit.value) return
        emit('update:selectedIds', value)
    })

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
        const checked = (event.target as HTMLInputElement).checked
        const set = new Set(selected.value)
        if (checked) set.add(id)
        else set.delete(id)
        selected.value = Array.from(set)
    }

    function toggleAll(event: Event) {
        const checked = (event.target as HTMLInputElement).checked
        const visible = visibleIds.value
        if (checked) {
            selected.value = [...visible]
        } else {
            const drop = new Set(visible)
            selected.value = selected.value.filter((id) => !drop.has(id))
        }
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
