import { computed, ref, type Ref } from 'vue'
import { storageGetJson, storageSetJson } from '../utils/storage'

/**
 * Board column layout (DEV-130). The board is an ordered list of columns;
 * each column shows one or more statuses (each keeps its own section and
 * drop target) and carries its own display settings: an optional name, a
 * WIP limit counted across all of its statuses, and card display rules
 * (hide cards older than N days, show at most N cards per status).
 * Individual statuses can be hidden. Saved per project in browser storage.
 * Status keys are normalized, so renamed or removed statuses are ignored and
 * newly configured statuses appear next to their configured neighbour.
 */
export interface BoardColumnConfig {
    statuses: string[]
    name?: string
    wipLimit?: number
    maxAgeDays?: number
    maxVisible?: number
}

export interface BoardLayout {
    columns: BoardColumnConfig[]
    hidden: string[]
}

export interface LayoutStatus {
    key: string
    label: string
}

export interface LaneColumn {
    key: string
    label: string
    synthetic: boolean
}

export interface BoardLane<C extends LaneColumn = LaneColumn> {
    key: string
    columns: C[]
    stacked: boolean
    title: string
    config: BoardColumnConfig
}

/** Legacy per-project settings that predate the combined layout. */
export interface LegacyBoardSettings {
    stack?: { hidden?: unknown; stacked?: unknown } | null
    wip?: Record<string, unknown> | null
    done?: { statuses?: unknown; maxAgeDays?: unknown; maxVisible?: unknown } | null
}

const STORAGE_VERSION = 2

function stringList(value: unknown): string[] {
    return Array.isArray(value)
        ? value.filter((entry): entry is string => typeof entry === 'string' && entry.length > 0)
        : []
}

function positiveInt(value: unknown): number | undefined {
    const parsed = typeof value === 'number' ? value : Number(value)
    return Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : undefined
}

function cleanColumn(raw: unknown): BoardColumnConfig | null {
    if (!raw || typeof raw !== 'object') return null
    const value = raw as Record<string, unknown>
    const statuses = stringList(value.statuses)
    if (!statuses.length) return null
    const column: BoardColumnConfig = { statuses }
    const name = typeof value.name === 'string' ? value.name.trim() : ''
    if (name) column.name = name
    const wipLimit = positiveInt(value.wipLimit)
    if (wipLimit) column.wipLimit = wipLimit
    const maxAgeDays = positiveInt(value.maxAgeDays)
    if (maxAgeDays) column.maxAgeDays = maxAgeDays
    const maxVisible = positiveInt(value.maxVisible)
    if (maxVisible) column.maxVisible = maxVisible
    return column
}

/** Parse a stored layout; null when absent or not the current format. */
export function parseBoardLayout(value: unknown): BoardLayout | null {
    if (!value || typeof value !== 'object') return null
    const raw = value as { version?: unknown; columns?: unknown; hidden?: unknown }
    if (raw.version !== STORAGE_VERSION || !Array.isArray(raw.columns)) return null
    return {
        columns: raw.columns.map(cleanColumn).filter((column): column is BoardColumnConfig => column !== null),
        hidden: stringList(raw.hidden),
    }
}

/**
 * Fit a layout to the currently configured statuses: unknown keys and empty
 * columns are dropped, duplicates keep their first position, and statuses the
 * layout does not mention get their own column right after the column that
 * holds their nearest configured predecessor.
 */
export function reconcileColumns(columns: BoardColumnConfig[], statuses: LayoutStatus[]): BoardColumnConfig[] {
    const known = new Set(statuses.map((status) => status.key))
    const seen = new Set<string>()
    const result: BoardColumnConfig[] = []
    for (const column of columns) {
        const keys = column.statuses.filter((key) => known.has(key) && !seen.has(key))
        keys.forEach((key) => seen.add(key))
        if (keys.length) result.push({ ...column, statuses: keys })
    }
    statuses.forEach((status, index) => {
        if (seen.has(status.key)) return
        seen.add(status.key)
        let insertAt = 0
        for (let previous = index - 1; previous >= 0; previous -= 1) {
            const at = result.findIndex((column) => column.statuses.includes(statuses[previous]!.key))
            if (at >= 0) {
                insertAt = at + 1
                break
            }
        }
        result.splice(insertAt, 0, { statuses: [status.key] })
    })
    return result
}

/** Build a layout from the settings used before the combined dialog. */
export function migrateLegacySettings(statuses: LayoutStatus[], legacy: LegacyBoardSettings): BoardLayout {
    const hidden = stringList(legacy.stack?.hidden)
    const stacked = new Set(stringList(legacy.stack?.stacked))
    const columns: BoardColumnConfig[] = []
    let lastVisible: BoardColumnConfig | null = null
    for (const status of statuses) {
        const isHidden = hidden.includes(status.key)
        if (!isHidden && stacked.has(status.key) && lastVisible) {
            lastVisible.statuses.push(status.key)
            continue
        }
        const column: BoardColumnConfig = { statuses: [status.key] }
        columns.push(column)
        if (!isHidden) lastVisible = column
    }
    const labelOf = new Map(statuses.map((status) => [status.key, status.label]))
    const wip = legacy.wip && typeof legacy.wip === 'object' ? legacy.wip : {}
    const doneStatuses = new Set(stringList(legacy.done?.statuses))
    const maxAgeDays = positiveInt(legacy.done?.maxAgeDays)
    const maxVisible = positiveInt(legacy.done?.maxVisible)
    for (const column of columns) {
        const labels = column.statuses.map((key) => labelOf.get(key) ?? key)
        const limit = labels.reduce((sum, label) => sum + (positiveInt(wip[label]) ?? 0), 0)
        if (limit > 0) column.wipLimit = limit
        if ((maxAgeDays || maxVisible) && labels.some((label) => doneStatuses.has(label))) {
            if (maxAgeDays) column.maxAgeDays = maxAgeDays
            if (maxVisible) column.maxVisible = maxVisible
        }
    }
    return { columns, hidden }
}

/**
 * Arrange board columns into lanes following the layout. Hidden statuses are
 * left out (a column with only hidden statuses disappears); synthetic columns
 * (the unknown-status "Other" bucket) always get their own trailing lane.
 */
export function computeBoardLanes<C extends LaneColumn>(columns: C[], layout: BoardLayout): BoardLane<C>[] {
    const byKey = new Map(columns.filter((column) => !column.synthetic).map((column) => [column.key, column]))
    const statuses = columns
        .filter((column) => !column.synthetic)
        .map((column) => ({ key: column.key, label: column.label }))
    const hidden = new Set(layout.hidden)
    const lanes: BoardLane<C>[] = []
    for (const config of reconcileColumns(layout.columns, statuses)) {
        const members = config.statuses
            .filter((key) => !hidden.has(key))
            .map((key) => byKey.get(key))
            .filter((column): column is C => column !== undefined)
        if (!members.length) continue
        lanes.push({
            key: members.map((column) => column.key).join('+'),
            columns: members,
            stacked: members.length > 1,
            title: config.name || members.map((column) => column.label).join(' · '),
            config,
        })
    }
    for (const column of columns) {
        if (column.synthetic) {
            lanes.push({
                key: column.key,
                columns: [column],
                stacked: false,
                title: column.label,
                config: { statuses: [] },
            })
        }
    }
    return lanes
}

export function useBoardLayout(project: Ref<string>, statuses: Ref<LayoutStatus[]>) {
    const stored = ref<BoardLayout | null>(null)
    const legacy = ref<LegacyBoardSettings>({})

    const key = (name: string) => (project.value ? `${name}::${project.value}` : name)

    function load() {
        stored.value = parseBoardLayout(storageGetJson<unknown>(key('lotar.boardColumns')))
        legacy.value = stored.value
            ? {}
            : {
                  stack: storageGetJson<LegacyBoardSettings['stack']>(key('lotar.boardLayout')),
                  wip: storageGetJson<Record<string, unknown>>(key('lotar.wip')),
                  done: storageGetJson<LegacyBoardSettings['done']>(key('lotar.doneFilters')),
              }
    }

    /** The effective layout for the current statuses. */
    const layout = computed<BoardLayout>(() => {
        const base = stored.value ?? migrateLegacySettings(statuses.value, legacy.value)
        return { columns: reconcileColumns(base.columns, statuses.value), hidden: base.hidden }
    })

    function save(next: BoardLayout) {
        const clean: BoardLayout = {
            columns: next.columns.map(cleanColumn).filter((column): column is BoardColumnConfig => column !== null),
            hidden: stringList(next.hidden),
        }
        stored.value = clean
        storageSetJson(key('lotar.boardColumns'), { version: STORAGE_VERSION, ...clean })
    }

    function showAll() {
        save({ ...layout.value, hidden: [] })
    }

    function reset() {
        save({ columns: reconcileColumns([], statuses.value), hidden: [] })
    }

    const isHidden = (statusKey: string) => layout.value.hidden.includes(statusKey)

    return { layout, load, save, showAll, reset, isHidden }
}
