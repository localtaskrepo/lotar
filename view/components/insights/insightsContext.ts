import { computed, inject, provide, type ComputedRef, type InjectionKey, type Ref } from 'vue'
import { useRouter } from 'vue-router'
import type { TaskDTO } from '../../api/types'
import { useCompletionPolicy, type CompletionPolicyApi } from '../../composables/useCompletionPolicy'
import { useTaskPanelController } from '../../composables/useTaskPanelController'
import {
    bucketsFor,
    bucketUnitFor,
    buildTimeline,
    changeRatio,
    countBy,
    countPerBucket,
    cycleDays,
    DAY_MS,
    effortTotals,
    formatDays,
    inferInitialStatuses,
    inRange,
    leadDays,
    orderByConfigured,
    parseDue,
    percentile,
    PERIOD_OPTIONS,
    periodRange,
    previousRange,
    projectOf,
    stablePercentile,
    statusKey,
    type CompletionLookup,
    type CountEntry,
    type PeriodKey,
    type Range,
    type TaskTimeline,
} from '../../utils/insightsMetrics'
import type { BarRow } from './BarList.vue'
import type { StatDelta } from './StatTile.vue'

// -- Formatting (pure) ----------------------------------------------------------
const numberFormat = new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 })
export function fmt(value: number, digits = 0): string {
    const factor = 10 ** digits
    return numberFormat.format(Math.round(value * factor) / factor)
}
export function pct(ratio: number): string {
    return `${Math.round(ratio * 100)}%`
}
export const dayFormat = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
export function formatDue(value: string | null | undefined): string {
    const at = parseDue(value)
    return at === null ? value || '—' : dayFormat.format(at)
}
/** Local-midnight timestamp for a `YYYY-MM-DD` day. */
export function localDay(value: string): number {
    return new Date(`${value}T00:00:00`).getTime()
}
/** The local `YYYY-MM-DD` day containing `at`. */
export function toDay(at: number): string {
    const date = new Date(at)
    const pad = (n: number) => String(n).padStart(2, '0')
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}
/** Summed effort, e.g. "12h + 5pt", or "—" when nothing is estimated. */
export function effortText(list: TaskDTO[]): string {
    const totals = effortTotals(list)
    const parts = []
    if (totals.hours) parts.push(`${Math.round(totals.hours)}h`)
    if (totals.points) parts.push(`${Math.round(totals.points * 10) / 10}pt`)
    return parts.join(' + ') || '—'
}
/** Non-null values of `pick` across `list`. */
export function valuesOf(list: TaskTimeline[], pick: (timeline: TaskTimeline) => number | null): number[] {
    return list.map(pick).filter((value): value is number => value !== null)
}
/** Rows whose key has no matching Tasks-list filter are not clickable. */
export function rows(entries: CountEntry[], mutedKey = '(none)', clickable = true): BarRow[] {
    return entries.map((entry) => ({
        key: entry.key,
        label: entry.key,
        value: entry.count,
        muted: entry.key === mutedKey,
        clickable: clickable && entry.key !== mutedKey,
    }))
}

// -- Shared state for the lenses -------------------------------------------------
export type PeriodChoice = PeriodKey | 'custom'

export interface InsightsInput {
    tasks: Readonly<Ref<TaskDTO[]>>
    project: Readonly<Ref<string>>
    /** Current filter-bar values, carried into Tasks-list drill-downs. */
    filter: Readonly<Ref<Record<string, string>>>
    statuses: Readonly<Ref<string[]>>
    priorities: Readonly<Ref<string[]>>
    types: Readonly<Ref<string[]>>
    period: Readonly<Ref<PeriodChoice>>
    customFrom: Readonly<Ref<string>>
    customTo: Readonly<Ref<string>>
    /** The signed-in person, for "My work". */
    me: Readonly<Ref<string>>
    now: Readonly<Ref<number>>
}

export interface InsightsContext extends InsightsInput {
    customRange: ComputedRef<Range | null>
    range: ComputedRef<Range>
    prevRange: ComputedRef<Range>
    periodLabel: ComputedRef<string>
    buckets: ComputedRef<ReturnType<typeof bucketsFor>>
    bucketLabels: ComputedRef<string[]>
    bucketPartial: ComputedRef<boolean[]>
    bucketUnit: ComputedRef<ReturnType<typeof bucketUnitFor>>

    completion: CompletionPolicyApi
    lookup: CompletionLookup
    timelines: ComputedRef<TaskTimeline[]>
    openTimelines: ComputedRef<TaskTimeline[]>
    open: ComputedRef<TaskDTO[]>
    completed: ComputedRef<TaskTimeline[]>
    completedPrev: ComputedRef<TaskTimeline[]>
    created: ComputedRef<TaskTimeline[]>
    createdPrev: ComputedRef<TaskTimeline[]>
    overdue: ComputedRef<TaskDTO[]>
    /** Matches the server's `due=soon`: tomorrow through seven days out. */
    dueSoon: ComputedRef<TaskDTO[]>
    noDueDate: ComputedRef<TaskDTO[]>
    wip: ComputedRef<TaskTimeline[]>
    completedSeries: ComputedRef<number[]>
    leadP50: ComputedRef<number | null>
    cycleP85: ComputedRef<number | null>
    openByStatus: ComputedRef<BarRow[]>
    openByTag: ComputedRef<BarRow[]>

    /** Count rows for `list` by priority, highest first. */
    priorityRows(list: TaskDTO[]): BarRow[]
    /** Days since work on an in-progress task started. */
    startedDays(timeline: TaskTimeline): number
    /** Change vs the previous period; `goodWhen` says which direction is good news. */
    delta(current: number, previous: number, goodWhen: 'up' | 'down' | null, digits?: number): StatDelta | null
    /** Change in a duration vs the previous period (shorter is better). */
    durationDelta(current: number | null, previous: number | null): StatDelta | null
    /** Open the Tasks list with the current filters plus `extra`; `openOnly` adds the open statuses. */
    openTasks(extra: Record<string, string>, openOnly?: boolean): void
    openTask(id: string): void
}

const INSIGHTS_KEY: InjectionKey<InsightsContext> = Symbol('insights')

/** Build the shared Insights state and provide it to the lens components. */
export function provideInsights(input: InsightsInput): InsightsContext {
    const router = useRouter()
    const { openTaskPanel } = useTaskPanelController()
    const { tasks, project, filter, statuses, priorities, period, customFrom, customTo, now } = input

    // -- Time ---------------------------------------------------------------------
    const customRange = computed<Range | null>(() => {
        if (!customFrom.value || !customTo.value) return null
        const start = localDay(customFrom.value)
        const end = new Date(localDay(customTo.value))
        end.setDate(end.getDate() + 1)
        return end.getTime() > start ? { start, end: end.getTime() } : null
    })
    const range = computed<Range>(() =>
        period.value === 'custom'
            ? (customRange.value ?? periodRange('30d', now.value))
            : periodRange(period.value, now.value, tasks.value),
    )
    const prevRange = computed(() => previousRange(range.value))
    const comparable = computed(() => period.value !== 'all')
    const shortDay = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
    const periodLabel = computed(() => {
        if (period.value === 'custom') {
            return customRange.value
                ? `${shortDay.format(customRange.value.start)} – ${shortDay.format(customRange.value.end - 1)}`
                : 'Custom range'
        }
        return PERIOD_OPTIONS.find((option) => option.value === period.value)!.label
    })
    /** "previous 30 days", "previous period" for comparisons. */
    const previousLabel = computed(() =>
        period.value === 'custom'
            ? 'previous period'
            : `previous ${periodLabel.value.replace(/^Last /, '').toLowerCase()}`,
    )
    const buckets = computed(() => bucketsFor(range.value))
    const bucketLabels = computed(() => buckets.value.map((bucket) => bucket.label))
    const bucketPartial = computed(() => buckets.value.map((bucket) => bucket.partial))
    const bucketUnit = computed(() => bucketUnitFor(range.value))

    // -- Timelines and core sets ------------------------------------------------
    const completion = useCompletionPolicy({ tasks: () => tasks.value })
    const lookup: CompletionLookup = {
        isTaskDone: (task) => completion.isTaskDone(task),
        // The same done labels isTaskDone relies on: the task's own policy first.
        doneLabels: (task) =>
            task.task_state?.done_states?.length
                ? task.task_state.done_states
                : completion.doneLabelsFor(projectOf(task.id)),
    }
    const initialStatuses = computed(() => {
        const inferred = inferInitialStatuses(tasks.value)
        if (project.value && !inferred.has(project.value) && statuses.value[0]) {
            inferred.set(project.value, statusKey(statuses.value[0]))
        }
        return inferred
    })
    // Timelines are cached per task object, so this only re-parses changed tasks.
    const timelines = computed<TaskTimeline[]>(() =>
        tasks.value.map((task) => buildTimeline(task, lookup, initialStatuses.value.get(projectOf(task.id)) ?? null)),
    )
    const openTimelines = computed(() => timelines.value.filter((timeline) => !timeline.isDone))
    const open = computed(() => openTimelines.value.map((timeline) => timeline.task))
    const completed = computed(() => timelines.value.filter((timeline) => inRange(timeline.doneAt, range.value)))
    const completedPrev = computed(() =>
        timelines.value.filter((timeline) => inRange(timeline.doneAt, prevRange.value)),
    )
    const created = computed(() => timelines.value.filter((timeline) => inRange(timeline.created, range.value)))
    const createdPrev = computed(() => timelines.value.filter((timeline) => inRange(timeline.created, prevRange.value)))
    const overdue = computed(() => open.value.filter((task) => completion.isTaskOverdue(task)))
    const dueSoon = computed(() => open.value.filter((task) => completion.dueBucketFor(task) === 'soon'))
    const noDueDate = computed(() => open.value.filter((task) => !task.due_date))
    const wip = computed(() => openTimelines.value.filter((timeline) => timeline.inProgress))

    const completedSeries = computed(() =>
        countPerBucket(
            completed.value.map((timeline) => timeline.doneAt),
            buckets.value,
        ),
    )
    const leadP50 = computed(() => percentile(valuesOf(completed.value, leadDays), 50))
    const cycleP85 = computed(() => stablePercentile(valuesOf(completed.value, cycleDays), 85))

    // -- Breakdowns used by several lenses -----------------------------------------
    const openByStatus = computed(() =>
        rows(
            orderByConfigured(
                countBy(open.value, (task) => task.status),
                statuses.value,
            ),
        ),
    )
    /** Status filter that limits a Tasks-list drill-down to open work. */
    const openStatusFilter = computed(() =>
        openByStatus.value
            .map((row) => row.key)
            .filter((key) => key !== '(none)')
            .join(','),
    )
    const openByTag = computed(() =>
        rows(
            countBy(open.value, (task) => (task.tags?.length ? task.tags : null), 'No tag'),
            'No tag',
        ),
    )
    function priorityRows(list: TaskDTO[]): BarRow[] {
        return rows(
            orderByConfigured(
                countBy(list, (task) => task.priority),
                [...priorities.value].reverse(),
            ),
        )
    }
    function startedDays(timeline: TaskTimeline): number {
        return timeline.startedAt === null ? 0 : Math.max(0, (now.value - timeline.startedAt) / DAY_MS)
    }

    // -- Comparisons ---------------------------------------------------------------
    function delta(current: number, previous: number, goodWhen: 'up' | 'down' | null, digits = 0): StatDelta | null {
        if (!comparable.value) return null
        const ratio = changeRatio(current, previous)
        const direction = current > previous ? 'up' : current < previous ? 'down' : 'flat'
        const tone = direction === 'flat' || !goodWhen ? 'neutral' : direction === goodWhen ? 'good' : 'bad'
        const vs = `vs ${previousLabel.value}`
        if (direction === 'flat') return { text: `No change ${vs}`, direction, tone }
        const word = direction === 'up' ? 'Up' : 'Down'
        // Percentages of tiny baselines mislead ("1300%"); show the difference instead.
        const amount =
            ratio === null || previous < 10 ? fmt(Math.abs(current - previous), digits) : pct(Math.abs(ratio))
        return { text: `${word} ${amount} ${vs}`, direction, tone }
    }
    function durationDelta(current: number | null, previous: number | null): StatDelta | null {
        if (!comparable.value || current === null || previous === null) return null
        const direction = current > previous ? 'up' : current < previous ? 'down' : 'flat'
        return {
            text:
                direction === 'flat'
                    ? 'No change'
                    : `${direction === 'up' ? 'Up' : 'Down'} from ${formatDays(previous)}`,
            direction,
            tone: direction === 'flat' ? 'neutral' : direction === 'down' ? 'good' : 'bad',
        }
    }

    // -- Navigation ----------------------------------------------------------------
    function openTasks(extra: Record<string, string>, openOnly = false) {
        const target: Record<string, string> = { ...filter.value }
        if (project.value) target.project = project.value
        if (openOnly && openStatusFilter.value && !target.status) target.status = openStatusFilter.value
        for (const [key, value] of Object.entries(extra)) if (value) target[key] = value
        void router.push({ path: '/', query: target })
    }
    function openTask(id: string) {
        openTaskPanel({ taskId: id })
    }

    const context: InsightsContext = {
        ...input,
        customRange,
        range,
        prevRange,
        periodLabel,
        buckets,
        bucketLabels,
        bucketPartial,
        bucketUnit,
        completion,
        lookup,
        timelines,
        openTimelines,
        open,
        completed,
        completedPrev,
        created,
        createdPrev,
        overdue,
        dueSoon,
        noDueDate,
        wip,
        completedSeries,
        leadP50,
        cycleP85,
        openByStatus,
        openByTag,
        priorityRows,
        startedDays,
        delta,
        durationDelta,
        openTasks,
        openTask,
    }
    provide(INSIGHTS_KEY, context)
    return context
}

export function useInsights(): InsightsContext {
    const context = inject(INSIGHTS_KEY, null)
    if (!context) throw new Error('Insights lenses must be rendered inside the Insights page')
    return context
}
