import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import type { TaskDTO } from '../api/types'
import { formatTaskDate, parseTaskDateToMillis, startOfLocalDay } from '../utils/date'
import type { TaskTouch } from './useActivity'

export interface TaskTableProps {
    tasks: TaskDTO[]
    loading?: boolean
    statuses?: string[]
    selectable?: boolean
    selectedIds?: string[]
    projectKey?: string
    bulk?: boolean
    bulkAssignee?: string
    touches?: Record<string, TaskTouch>
}

export interface TaskTableEmit {
    (event: 'open', id: string): void
    (event: 'delete', id: string): void
    (event: 'update-tags', payload: { id: string; tags: string[] }): void
    (event: 'set-status', payload: { id: string; status: string }): void
    (event: 'assign', id: string): void
    (event: 'unassign', id: string): void
    (event: 'update:selectedIds', value: string[]): void
    (event: 'update:bulk', value: boolean): void
    (event: 'update:bulkAssignee', value: string): void
    (event: 'update:bulk-assignee', value: string): void
    (event: 'bulk-assign'): void
    (event: 'bulk-unassign'): void
    (event: 'add'): void
}

type ColKey =
    | 'id'
    | 'title'
    | 'status'
    | 'priority'
    | 'task_type'
    | 'reporter'
    | 'assignee'
    | 'effort'
    | 'tags'
    | 'due_date'
    | 'modified'

const allColumns: ColKey[] = [
    'id',
    'title',
    'status',
    'priority',
    'task_type',
    'reporter',
    'assignee',
    'effort',
    'tags',
    'due_date',
    'modified',
]

const defaultColumns: ColKey[] = [
    'id',
    'title',
    'status',
    'priority',
    'reporter',
    'assignee',
    'tags',
    'due_date',
    'modified',
]

const COLS_KEY = 'lotar.taskTable.columns'
const SORT_KEY = 'lotar.taskTable.sort'

export function useTaskTableState(props: Readonly<TaskTableProps>, emit: TaskTableEmit) {
    function colsKey() {
        return props.projectKey ? `${COLS_KEY}::${props.projectKey}` : COLS_KEY
    }

    const initialColumns = (() => {
        try {
            const raw = localStorage.getItem(colsKey()) ?? localStorage.getItem(COLS_KEY)
            return JSON.parse(raw || 'null') as ColKey[] | null
        } catch {
            return null
        }
    })()

    const columns = ref<ColKey[]>(Array.isArray(initialColumns) && initialColumns.length ? initialColumns : defaultColumns)
    const columnsSet = computed(() => new Set(columns.value))
    const visibleColumns = computed(() => allColumns.filter((c) => columnsSet.value.has(c)))

    watch(
        columns,
        (value) => {
            try {
                localStorage.setItem(colsKey(), JSON.stringify(value))
            } catch { }
        },
        { deep: true },
    )

    function headerLabel(key: ColKey) {
        const labels: Record<ColKey, string> = {
            id: 'ID',
            title: 'Title',
            status: 'Status',
            priority: 'Priority',
            task_type: 'Type',
            reporter: 'Reporter',
            assignee: 'Assignee',
            effort: 'Effort',
            tags: 'Tags',
            due_date: 'Due',
            modified: 'Updated',
        }
        return labels[key]
    }

    const showColumnMenu = ref(false)
    function toggleColumnMenu() {
        showColumnMenu.value = !showColumnMenu.value
    }

    const rootRef = ref<HTMLElement | null>(null)

    function onDocClick(event: MouseEvent) {
        const root = rootRef.value
        const target = event.target as Node
        if (!root || !target) return
        if (!root.contains(target)) {
            showColumnMenu.value = false
            rowMenu.value = {}
        }
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

    function toggleColumn(col: ColKey, event: Event) {
        const checked = (event.target as HTMLInputElement).checked
        const next = new Set(columns.value)
        if (checked) next.add(col)
        else next.delete(col)
        if (!next.has('id')) next.add('id')
        if (!next.has('title')) next.add('title')
        const arr = Array.from(next)
        columns.value = arr
        try {
            localStorage.setItem(colsKey(), JSON.stringify(arr))
        } catch { }
    }

    function resetColumns() {
        columns.value = [...defaultColumns]
    }

    function sortKey() {
        return props.projectKey ? `${SORT_KEY}::${props.projectKey}` : SORT_KEY
    }

    const sort = reactive<{ key: ColKey | null; dir: 'asc' | 'desc' }>(
        (() => {
            try {
                return (
                    JSON.parse((localStorage.getItem(sortKey()) ?? localStorage.getItem(SORT_KEY)) || 'null') || {
                        key: null,
                        dir: 'desc',
                    }
                )
            } catch {
                return { key: null, dir: 'desc' }
            }
        })(),
    )

    function onSort(key: ColKey) {
        if (sort.key === key) {
            sort.dir = sort.dir === 'asc' ? 'desc' : 'asc'
        } else {
            sort.key = key
            sort.dir = 'asc'
        }
    }

    watch(
        sort,
        (value) => {
            try {
                localStorage.setItem(sortKey(), JSON.stringify(value))
            } catch { }
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

    watch(
        () => props.selectedIds,
        (value) => {
            selected.value = value ? [...value] : []
        },
    )

    watch(selected, (value) => emit('update:selectedIds', value))

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
            selected.value = [...new Set([...selected.value, ...visible])]
        } else {
            const drop = new Set(visible)
            selected.value = selected.value.filter((id) => !drop.has(id))
        }
    }

    function onToggleBulk(event: Event) {
        emit('update:bulk', (event.target as HTMLInputElement).checked)
    }

    function onBulkAssignee(event: Event) {
        const value = (event.target as HTMLInputElement).value
        emit('update:bulkAssignee', value)
        emit('update:bulk-assignee', value)
    }

    const tagsEditing = ref<Record<string, boolean>>({})
    const tagsDrafts = ref<Record<string, string>>({})

    watch(
        () => props.tasks,
        (list) => {
            (list || []).forEach((task) => {
                if (!tagsEditing.value[task.id]) {
                    tagsDrafts.value[task.id] = (task.tags || []).join(', ')
                }
            })
        },
        { immediate: true, deep: true },
    )

    function isEditingTags(id: string) {
        return !!tagsEditing.value[id]
    }

    function toggleTagsEdit(id: string) {
        tagsEditing.value[id] = !tagsEditing.value[id]
    }

    function saveTags(task: TaskDTO) {
        const list = (tagsDrafts.value[task.id] || '')
            .split(',')
            .map((value) => value.trim())
            .filter(Boolean)
        emit('update-tags', { id: task.id, tags: list })
        tagsEditing.value[task.id] = false
    }

    function projectOf(id: string) {
        return (id || '').split('-')[0]
    }

    function numericOf(id: string) {
        return (id || '').split('-').slice(1).join('-')
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

    const relativeTimeFormatter =
        typeof Intl !== 'undefined' && (Intl as any).RelativeTimeFormat
            ? new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
            : null

    const relativeUnits: Array<{ unit: Intl.RelativeTimeFormatUnit; ms: number }> = [
        { unit: 'year', ms: 1000 * 60 * 60 * 24 * 365 },
        { unit: 'month', ms: 1000 * 60 * 60 * 24 * 30 },
        { unit: 'week', ms: 1000 * 60 * 60 * 24 * 7 },
        { unit: 'day', ms: 1000 * 60 * 60 * 24 },
        { unit: 'hour', ms: 1000 * 60 * 60 },
        { unit: 'minute', ms: 1000 * 60 },
        { unit: 'second', ms: 1000 },
    ]

    function relativeTime(value: string) {
        if (!value) return ''
        const target = new Date(value)
        const timestamp = target.getTime()
        if (!isFinite(timestamp)) return value
        const diff = timestamp - Date.now()
        if (!relativeTimeFormatter) return target.toLocaleString()
        for (const { unit, ms } of relativeUnits) {
            if (Math.abs(diff) >= ms || unit === 'second') {
                const amount = Math.round(diff / ms)
                return relativeTimeFormatter.format(amount, unit)
            }
        }
        return target.toLocaleString()
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
        try {
            const status = (task.status || '').toLowerCase()
            if (!task.due_date || status === 'done') return false
            const due = parseTaskDateToMillis(task.due_date)
            if (due === null) return false
            const todayStart = startOfLocalDay(new Date()).getTime()
            return due < todayStart
        } catch {
            return false
        }
    }

    return {
        allColumns,
        columns,
        columnsSet,
        visibleColumns,
        headerLabel,
        showColumnMenu,
        toggleColumnMenu,
        toggleColumn,
        resetColumns,
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
        onBulkAssignee,
        tagsEditing,
        tagsDrafts,
        isEditingTags,
        toggleTagsEdit,
        saveTags,
        projectOf,
        numericOf,
        fmtDate,
        fmtDateTime,
        relativeTime,
        touchBadge,
        isOverdue,
    }
}
