import { computed, ref, type Ref } from 'vue'
import type { TaskDTO } from '../api/types'
import { listFromCsv } from './useFilterBuilder'
import { storageGetJson, storageSetJson } from '../utils/storage'
import { sortTasks, type TaskSortOrder } from '../utils/taskSort'

export type BoardSortKey =
    'modified' | 'created' | 'priority' | 'due' | 'effort' | 'title' | 'id' | 'assignee' | 'type' | 'sprint'

type BoardSortOption = { value: BoardSortKey; label: string; defaultDir: TaskSortOrder }

const BASE_OPTIONS: BoardSortOption[] = [
    { value: 'modified', label: 'Last modified', defaultDir: 'desc' },
    { value: 'created', label: 'Created', defaultDir: 'desc' },
    { value: 'priority', label: 'Priority', defaultDir: 'desc' },
    { value: 'due', label: 'Due date', defaultDir: 'asc' },
    { value: 'effort', label: 'Effort', defaultDir: 'desc' },
    { value: 'title', label: 'Title', defaultDir: 'asc' },
    { value: 'id', label: 'ID', defaultDir: 'asc' },
    { value: 'assignee', label: 'Assignee', defaultDir: 'asc' },
    { value: 'type', label: 'Type', defaultDir: 'asc' },
]
const SPRINT_OPTION: BoardSortOption = { value: 'sprint', label: 'Sprint order', defaultDir: 'asc' }

export const DEFAULT_BOARD_SORT: { key: BoardSortKey; dir: TaskSortOrder } = { key: 'modified', dir: 'desc' }

const KEYS = new Set<string>([...BASE_OPTIONS, SPRINT_OPTION].map((option) => option.value))

/** The sprint id when the sprint filter selects exactly one sprint by id. */
export function singleSprintId(sprintsCsv: string | undefined): number | null {
    const tokens = listFromCsv(sprintsCsv ?? '')
    if (tokens.length !== 1) return null
    const id = Number(tokens[0])
    return Number.isInteger(id) && id > 0 ? id : null
}

/**
 * Order board cards. `priorities` is the project's configured order (lowest
 * first), so "desc" puts the highest priority on top. `sprint` uses the
 * manual order from the Sprints page; unordered tasks follow by ID.
 */
export function sortBoardTasks(
    tasks: readonly TaskDTO[],
    key: BoardSortKey,
    dir: TaskSortOrder,
    options: { priorities?: readonly string[]; sprintId?: number | null } = {},
): TaskDTO[] {
    if (key === 'sprint') {
        const sprintId = options.sprintId
        const byId = sortTasks(tasks, 'id', 'asc')
        if (!sprintId) return byId
        const position = (task: TaskDTO) => task.sprint_order?.[sprintId] ?? Infinity
        return byId.sort((a, b) => position(a) - position(b))
    }
    if (key === 'priority') {
        const rank = new Map<string, number>()
        ;(options.priorities ?? []).forEach((label, index) => rank.set(label.trim().toLowerCase(), index))
        // Fall back to recency inside a priority, then sort stably by rank.
        const base = sortTasks(tasks, 'modified', 'desc')
        const value = (task: TaskDTO) => rank.get((task.priority ?? '').trim().toLowerCase()) ?? -1
        return base.sort((a, b) => (dir === 'desc' ? value(b) - value(a) : value(a) - value(b)))
    }
    return sortTasks(tasks, key, dir)
}

/** Board sort choice, saved per project, plus the sprint-order option. */
export function useBoardSort(project: Ref<string>, filter: Ref<Record<string, string>>) {
    const storageKey = () => (project.value ? `lotar.boardSort::${project.value}` : 'lotar.boardSort')

    function read(): { key: BoardSortKey; dir: TaskSortOrder } {
        const saved = storageGetJson<{ key?: string; dir?: string }>(storageKey())
        const key = saved?.key && KEYS.has(saved.key) ? (saved.key as BoardSortKey) : DEFAULT_BOARD_SORT.key
        const dir = saved?.dir === 'asc' || saved?.dir === 'desc' ? saved.dir : DEFAULT_BOARD_SORT.dir
        return { key, dir }
    }

    const initial = read()
    const key = ref<BoardSortKey>(initial.key)
    const dir = ref<TaskSortOrder>(initial.dir)

    const sprintId = computed(() => singleSprintId(filter.value.sprints))
    const options = computed(() => (sprintId.value ? [...BASE_OPTIONS, SPRINT_OPTION] : BASE_OPTIONS))
    /** Sprint order only applies while one sprint is selected. */
    const effectiveKey = computed<BoardSortKey>(() =>
        key.value === 'sprint' && !sprintId.value ? DEFAULT_BOARD_SORT.key : key.value,
    )
    const directional = computed(() => effectiveKey.value !== 'sprint')

    function save() {
        storageSetJson(storageKey(), { key: key.value, dir: dir.value })
    }

    function setKey(next: BoardSortKey) {
        if (next === key.value) return
        key.value = next
        dir.value = [...BASE_OPTIONS, SPRINT_OPTION].find((option) => option.value === next)?.defaultDir ?? 'asc'
        save()
    }

    function toggleDir() {
        dir.value = dir.value === 'asc' ? 'desc' : 'asc'
        save()
    }

    function reload() {
        const saved = read()
        key.value = saved.key
        dir.value = saved.dir
    }

    function apply(tasks: readonly TaskDTO[], priorities: readonly string[]) {
        return sortBoardTasks(tasks, effectiveKey.value, dir.value, { priorities, sprintId: sprintId.value })
    }

    return { key, dir, options, effectiveKey, directional, sprintId, setKey, toggleDir, reload, apply }
}
