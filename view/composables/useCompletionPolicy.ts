import { getCurrentScope, onScopeDispose, reactive, ref, watch } from 'vue'
import { api } from '../api/client'
import type { TaskDTO, TaskDueBucket } from '../api/types'
import { MS_PER_DAY, parseTaskDate, startOfLocalDay } from '../utils/date'
import { projectOf } from '../utils/text'
import { useSse } from './useSse'

/**
 * DEV-21 shared completion/overdue policy.
 *
 * One module-level cache of per-project terminal-state policies, resolved by
 * the server (`/api/config/show?project=<prefix>` returns
 * `effective_done_states`, `done_states_mode` and `task_calendar_day`). The
 * UI never re-implements legacy inference and never guesses a hardcoded
 * "Done" status. CLASSIFICATION PRECEDENCE: the embedded actual-root
 * `task_state.done_states` on each task DTO wins first (homonymous prefixes
 * across roots resolve different policies, so a prefix-level config lookup
 * cannot classify foreign-root tasks); the cached prefix policy serves the
 * config editor, Board presentation defaults, and the no-metadata legacy
 * fixture fallback; without either, `task_state.is_done` decides.
 *
 * Clock basis: due buckets and policy dates are SERVER-local calendar days.
 * A bounded shared GLOBAL probe (`api.showConfig()`, no project) watches the
 * live `task_calendar_day`; when the server day moves, cached policies are
 * refreshed AND retained task queries are invalidated so embedded
 * `task_state` metadata is refetched. Stale authoritative DTOs are never
 * recomputed with the browser clock: date-only due values are re-derived by
 * pure calendar-string comparison against the server day, timestamp due
 * values keep their last server bucket (neutral) until the refresh lands.
 */

export type { TaskDueBucket }

export type DoneStatesMode = 'explicit' | 'inferred'

/** Normalized status key: case/whitespace/separator-insensitive. */
export function normalizeStatusKey(value: string | null | undefined): string {
    return typeof value === 'string'
        ? value.trim().toLowerCase().replace(/[\s_-]+/g, '')
        : ''
}

/** Match the server's ASCII case-folding without merging distinct labels. */
export function completionStatusKey(value: string | null | undefined): string {
    return typeof value === 'string'
        ? value.replace(/[A-Z]/g, character => character.toLowerCase())
        : ''
}

export interface CompletionPolicy {
    /** Project prefix (actual task-id prefix) this policy was resolved for. */
    project: string
    /** Raw terminal status labels as resolved by the server. */
    doneLabels: string[]
    /** Normalized terminal status keys for matching. */
    doneStates: Set<string>
    mode: DoneStatesMode
    /** Server-local calendar day (YYYY-MM-DD) observed with this policy. */
    calendarDay: string
}

interface PolicyRecord extends CompletionPolicy {
    fetchedAt: number
}

const POLICY_TTL_MS = 15 * 60 * 1000
const SERVER_DAY_PROBE_INTERVAL_MS = 60_000
/** Coalesce save + config_updated SSE refreshes arriving back to back. */
const REFRESH_DEDUPE_MS = 300

const policyRecords = reactive(new Map<string, PolicyRecord>())
const inflight = new Map<string, Promise<void>>()
/** Per-prefix request sequence: a late stale response must never publish. */
const requestSeq = new Map<string, number>()
/** Latest server-local calendar day seen across policy fetches/probes. */
const serverCalendarDay = ref('')

function extractPolicy(prefix: string, config: unknown): CompletionPolicy | null {
    const source = (config ?? {}) as Record<string, any>
    const doneLabels: string[] = Array.isArray(source.effective_done_states)
        ? source.effective_done_states.map((value: unknown) => String(value)).filter((value: string) => value.trim().length > 0)
        : []
    // Fail closed: without a server-resolved list we never guess terminal
    // statuses client-side (fixtures may omit the field entirely).
    if (!doneLabels.length) return null
    const mode: DoneStatesMode = source.done_states_mode === 'explicit' ? 'explicit' : 'inferred'
    const calendarDay = typeof source.task_calendar_day === 'string' ? source.task_calendar_day : ''
    return {
        project: prefix,
        doneLabels,
        doneStates: new Set(doneLabels.map(completionStatusKey)),
        mode,
        calendarDay,
    }
}

async function fetchPolicy(prefix: string, { force = false }: { force?: boolean } = {}): Promise<void> {
    const seq = (requestSeq.get(prefix) ?? 0) + 1
    requestSeq.set(prefix, seq)
    let config: unknown
    try {
        config = await api.showConfig(prefix)
    } catch {
        // Fetch failed: keep whatever valid data is cached (never clear good
        // records on a failed refresh). Runtime task_state still carries truth.
        return
    }
    // A newer request for this prefix superseded this response.
    if (requestSeq.get(prefix) !== seq) return
    const policy = extractPolicy(prefix, config)
    if (policy) {
        policyRecords.set(prefix, { ...policy, fetchedAt: Date.now() })
        if (policy.calendarDay) observeServerDay(policy.calendarDay)
        ensureProbeActive()
    } else if (force) {
        // A forced refresh that legitimately resolves to "no list" (e.g.
        // downgraded backend) fails closed.
        policyRecords.delete(prefix)
    } else {
        // First-load miss: nothing cached to keep.
        policyRecords.delete(prefix)
    }
}

/** Resolve (or reuse) the terminal-state policy for one project prefix. */
export function ensureCompletionPolicy(prefix: string): Promise<void> {
    const key = prefix.trim()
    if (!key) return Promise.resolve()
    const cached = policyRecords.get(key)
    if (cached && Date.now() - cached.fetchedAt < POLICY_TTL_MS) return Promise.resolve()
    let pending = inflight.get(key)
    if (!pending) {
        pending = fetchPolicy(key)
            .catch(() => undefined)
            .finally(() => {
                inflight.delete(key)
            })
        inflight.set(key, pending)
    }
    return pending
}

/** Ensure policies for every distinct project prefix in a task list. */
export function ensureCompletionPolicies(tasks: readonly TaskDTO[] | null | undefined): Promise<void> {
    const prefixes = new Set<string>()
    for (const task of tasks || []) {
        const prefix = projectOf(task?.id)
        if (prefix) prefixes.add(prefix)
    }
    return Promise.all(Array.from(prefixes, (prefix) => ensureCompletionPolicy(prefix))).then(() => undefined)
}

/**
 * Force-refetch one prefix (bypasses the TTL; supersedes any older in-flight
 * response via the sequence guard; failures keep the cached record).
 */
export function refreshCompletionPolicy(prefix: string): Promise<void> {
    const key = prefix.trim()
    if (!key) return Promise.resolve()
    const pending = fetchPolicy(key, { force: true })
        .catch(() => undefined)
        .finally(() => {
            if (inflight.get(key) === pending) inflight.delete(key)
        })
    inflight.set(key, pending)
    return pending
}

let refreshInFlight: Promise<void> | null = null
let lastRefreshCompletedAt = 0

/**
 * Force-refetch every currently cached policy (config change, day change).
 *
 * Task entities carry EMBEDDED actual-root done_states (task_state), which
 * outrank this cache — so after a policy-bearing config change the retained
 * task queries are invalidated too, refetching entities with fresh embedded
 * sets. Without this, old embedded sets would keep winning over the new
 * policy legitimately. Deduped; failures keep cached records.
 */
export function refreshCompletionPolicies(): Promise<void> {
    if (refreshInFlight) return refreshInFlight
    if (lastRefreshCompletedAt && Date.now() - lastRefreshCompletedAt < REFRESH_DEDUPE_MS) {
        return Promise.resolve()
    }
    const prefixes = Array.from(policyRecords.keys())
    refreshInFlight = Promise.all(prefixes.map((prefix) => refreshCompletionPolicy(prefix)))
        .then(() => undefined)
        .then(() => {
            invalidateRetainedTaskQueries()
        })
        .finally(() => {
            refreshInFlight = null
            lastRefreshCompletedAt = Date.now()
        })
    return refreshInFlight
}

/**
 * Refresh policies after a config save in THIS tab. A global save affects
 * every cached project policy; a project save only its own prefix (fetched
 * fresh when not cached yet). Avoids clearing valid caches: records are
 * replaced by fresh responses and kept on failure. The embedded done_states
 * on task entities are refreshed via retained-query invalidation.
 */
export function refreshCompletionPoliciesForScope(scope: string): Promise<void> {
    const trimmed = (scope || '').trim()
    if (!trimmed) return refreshCompletionPolicies()
    const refresh = policyRecords.has(trimmed)
        ? refreshCompletionPolicy(trimmed)
        : ensureCompletionPolicy(trimmed)
    return refresh.then(() => {
        invalidateRetainedTaskQueries()
    })
}

/** Drop cached policies (all, or one prefix) so the next ensure refetches. */
export function invalidateCompletionPolicies(prefix?: string): void {
    if (prefix === undefined || prefix === null) {
        policyRecords.clear()
        // Drop refresh bookkeeping too, so an immediate follow-up refresh is
        // not swallowed by the dedupe window of a pre-invalidation refresh,
        // and a pending pre-invalidation fetch cannot pin later ensures.
        lastRefreshCompletedAt = 0
        refreshInFlight = null
        inflight.clear()
    } else {
        policyRecords.delete(prefix.trim())
    }
}

// ---- Retained task-query invalidation ---------------------------------------
// When the server calendar day moves, task entities keep stale task_state
// metadata; retained DEV-65 queries must refetch. The store registers its
// invalidation entry point here (kept injectable for tests/isolation).

let taskQueryInvalidator: (() => void) | null = null

export function registerTaskQueryInvalidator(invalidator: (() => void) | null): void {
    taskQueryInvalidator = invalidator
}

function invalidateRetainedTaskQueries(): void {
    try {
        taskQueryInvalidator?.()
    } catch {
        // A listener failure must never break the day-change refresh path.
    }
}

// ---- Shared global server-calendar probe -------------------------------------

let probeTimer: ReturnType<typeof setInterval> | null = null

function observeServerDay(day: string): void {
    if (!day) return
    serverCalendarDay.value = day
}

/**
 * One shared GLOBAL probe: `api.showConfig()` with no project returns the
 * globally resolved config including the live server-local
 * `task_calendar_day`. Bounded to a single call per interval (never
 * N-projects-per-minute); failures keep current knowledge and caches.
 *
 * When the server day moves, cached policies are refreshed and the retained
 * task queries invalidated before the probe resolves. A failed refresh keeps
 * the previous day marker, so the next probe retries (bounded to 1/min).
 */
export async function probeServerCalendarDay(): Promise<void> {
    let day: unknown
    try {
        day = ((await api.showConfig()) as Record<string, any> | null | undefined)?.task_calendar_day
    } catch {
        return
    }
    if (typeof day !== 'string' || !day) return
    const previous = serverCalendarDay.value
    if (!previous || day === previous) {
        observeServerDay(day)
        return
    }
    // Server calendar rolled: refresh policies (their calendarDay is
    // re-observed on publish; the refresh also invalidates retained task
    // queries whose embedded task_state is now provably stale). Never
    // recompute entities with browser time.
    await refreshCompletionPolicies()
    if (!policyRecords.size) {
        // No policies cached: nothing to protect, stop re-triggering.
        observeServerDay(day)
    }
}

function ensureProbeActive(): void {
    if (typeof window === 'undefined') return
    // Exactly one live probe interval; re-arming resets the 60s window on
    // every policy publish (rare) and self-heals across timer-environment
    // swaps (e.g. test fake clocks).
    if (probeTimer !== null) {
        clearInterval(probeTimer)
        probeTimer = null
    }
    probeTimer = setInterval(() => {
        if (!policyRecords.size) return // idle: nothing cached, nothing stale
        void probeServerCalendarDay()
    }, SERVER_DAY_PROBE_INTERVAL_MS)
    // Node test runners: never hold the process open for the probe alone.
    const maybeUnref = (probeTimer as unknown as { unref?: () => void }).unref
    if (typeof maybeUnref === 'function') maybeUnref.call(probeTimer)
}

let focusListenerAttached = false

function activatePolicyRefresh(): void {
    if (typeof window === 'undefined') return
    // The probe interval is armed by the first cached policy (see
    // fetchPolicy); activation itself only wires the focus/visibility probe.
    if (!focusListenerAttached && typeof document !== 'undefined') {
        focusListenerAttached = true
        document.addEventListener('visibilitychange', () => {
            if (document.visibilityState === 'visible' && policyRecords.size) {
                void probeServerCalendarDay()
            }
        })
    }
}

// ---- config_updated / reconnect invalidation -------------------------------

type ConfigWatchHandle = ReturnType<typeof useSse>
let configWatchHandle: ConfigWatchHandle | null = null
let configWatchConsumers = 0
let configWatchHandler: ((ev: MessageEvent) => void) | null = null

function acquireConfigWatch(): void {
    configWatchConsumers += 1
    if (configWatchHandle) return
    if (typeof EventSource === 'undefined') return
    configWatchHandle = useSse(
        '/api/events',
        { kinds: 'config_updated', ready: true },
        {
            // Missed config changes while disconnected: refetch policies.
            onReconnect: () => {
                if (policyRecords.size) void refreshCompletionPolicies()
            },
        },
    )
    configWatchHandler = () => {
        if (policyRecords.size) void refreshCompletionPolicies()
    }
    configWatchHandle.on('config_updated', configWatchHandler)
}

function releaseConfigWatch(): void {
    configWatchConsumers = Math.max(0, configWatchConsumers - 1)
    if (configWatchConsumers > 0) return
    if (configWatchHandle) {
        if (configWatchHandler) configWatchHandle.off('config_updated', configWatchHandler)
        configWatchHandle.close()
        configWatchHandle = null
        configWatchHandler = null
    }
}

// ---- Predicates -------------------------------------------------------------

type PolicyLikeTask = {
    id?: string | null
    status?: string | null
    due_date?: string | null
    task_state?: TaskDTO['task_state']
}

const CALENDAR_DAY_ONLY = /^\d{4}-\d{2}-\d{2}$/

function calendarDaysBetween(from: string, to: string): number | null {
    if (!CALENDAR_DAY_ONLY.test(from) || !CALENDAR_DAY_ONLY.test(to)) return null
    const fromMs = Date.parse(`${from}T00:00:00Z`)
    const toMs = Date.parse(`${to}T00:00:00Z`)
    if (!Number.isFinite(fromMs) || !Number.isFinite(toMs)) return null
    return Math.round((fromMs - toMs) / MS_PER_DAY)
}

/**
 * TZ-free bucket for DATE-ONLY due values: pure calendar-string arithmetic
 * against the SERVER day (never the browser clock). Returns null when the
 * due value is not a plain YYYY-MM-DD date (timestamps need a fresh server
 * DTO) or the server day is unknown.
 */
export function calendarStringDueBucket(dueRaw: string | null | undefined, serverDay: string): TaskDueBucket | null {
    if (!dueRaw || !serverDay) return null
    const diff = calendarDaysBetween(dueRaw.trim(), serverDay)
    if (diff === null) return null
    if (diff < 0) return 'overdue'
    if (diff === 0) return 'today'
    if (diff <= 7) return 'soon'
    return 'later'
}

/**
 * Local fallback bucket from the raw due date. Uses the browser's local day,
 * so it is only consulted while policy/task_state metadata is unavailable
 * (explicit fixture/test fallback). A task due today is never overdue here
 * (start-of-day comparison).
 */
export function localDueBucketOf(task: PolicyLikeTask | null | undefined): TaskDueBucket | null {
    const due = parseTaskDate(task?.due_date)
    if (!due) return null
    const today = startOfLocalDay(new Date())
    const dueStart = startOfLocalDay(due)
    const diffDays = Math.round((dueStart.getTime() - today.getTime()) / MS_PER_DAY)
    if (diffDays < 0) return 'overdue'
    if (diffDays === 0) return 'today'
    if (diffDays <= 7) return 'soon'
    return 'later'
}

/**
 * The embedded ORDERED actual-root terminal policy from the task's own
 * runtime metadata (`task_state.done_states`). Real payloads always carry it;
 * it stays null only for legacy fixtures without metadata. Homonymous
 * prefixes across roots resolve different policies, so this set — not a
 * config/show lookup by id prefix — is authoritative for this task.
 */
export function embeddedDoneStatesOf(task: PolicyLikeTask | null | undefined): Set<string> | null {
    const states = task?.task_state?.done_states
    if (!Array.isArray(states)) return null
    const keys = states
        .map(completionStatusKey)
        .filter((key) => key.length > 0)
    // An embedded empty set is authoritative fail-closed metadata, not a
    // missing field that permits borrowing a cached neighboring policy.
    return new Set(keys)
}

/**
 * Whether the task is complete right now. Precedence:
 * 1. embedded actual-root `done_states` — derive from the task's CURRENT
 *    status so optimistic changes never inherit a stale `is_done` and never
 *    consult a different-root cached policy;
 * 2. the per-prefix cached config policy (config editor / Board
 *    presentation / fallback when metadata is absent);
 * 3. the authoritative `task_state.is_done` flag;
 * 4. otherwise false (never a hardcoded "Done" guess).
 */
export function taskIsDone(policy: CompletionPolicy | null, task: PolicyLikeTask | null | undefined): boolean {
    const embedded = embeddedDoneStatesOf(task)
    if (embedded) return embedded.has(completionStatusKey(task?.status))
    if (policy) return policy.doneStates.has(completionStatusKey(task?.status))
    return task?.task_state?.is_done === true
}

/**
 * Resolve the task's due bucket.
 *
 * Snapshot freshness: `state.calendar_day` is compared against the known
 * SERVER day (the cached policy's calendarDay). Without a known server day a
 * snapshot cannot be judged stale and is trusted as-is (legacy fixture path).
 *
 * - STALE snapshot (server day moved since the DTO was computed): date-only
 *   due values are re-derived against the SERVER day by calendar-string
 *   arithmetic (done-now maps overdue to null); timestamp dues return null —
 *   strictly neutral, never the stale Today/Overdue masquerading as current —
 *   until the retained-query refresh delivers a fresh DTO.
 * - Fresh snapshot: the server's `task_state.due_bucket` is authoritative.
 *   Only Overdue excludes done tasks: a done task due today can still be
 *   'today', and a terminal task past due has bucket null (never Overdue).
 *   Done-now is decided by the embedded actual-root set first (optimistic
 *   status changes win over stale buckets/flags), then the cached policy.
 * - Without any state, a local start-of-day derivation is used (explicit
 *   fixture fallback only — never applied to known-stale authoritative DTOs).
 */
export function resolveDueBucket(policy: CompletionPolicy | null, task: PolicyLikeTask | null | undefined): TaskDueBucket | null {
    const state = task?.task_state ?? null
    const doneNow = taskIsDone(policy, task)

    const staleSnapshot = !!(state?.calendar_day && policy?.calendarDay && state.calendar_day !== policy.calendarDay)
    if (staleSnapshot) {
        const derived = calendarStringDueBucket(task?.due_date, policy!.calendarDay)
        if (derived !== null) {
            return doneNow && derived === 'overdue' ? null : derived
        }
        // Timestamp (or unparseable) due on a provably stale snapshot: no
        // trustworthy bucket until entities refetch.
        return null
    }

    if (doneNow) {
        // Terminal tasks past due are null, never overdue — including stale
        // server overdue buckets from just-closed optimistic transitions.
        const bucket = state ? (state.due_bucket ?? null) : localDueBucketOf(task)
        return bucket === 'overdue' ? null : bucket
    }
    if (state && state.is_done === false) {
        return state.due_bucket ?? null
    }
    // Optimistic reopen: a date-only due can be reclassified against the
    // known server day. Timestamp dues need a fresh authoritative snapshot.
    const serverDay = policy?.calendarDay || state?.calendar_day
    if (serverDay) return calendarStringDueBucket(task?.due_date, serverDay)
    // Metadata-free legacy fixtures retain the explicit local fallback.
    return localDueBucketOf(task)
}

export function resolveIsOverdue(policy: CompletionPolicy | null, task: PolicyLikeTask | null | undefined): boolean {
    return resolveDueBucket(policy, task) === 'overdue'
}

// ---- Consumer composable -----------------------------------------------------

export function useCompletionPolicy(options: { tasks?: () => readonly TaskDTO[] } = {}) {
    acquireConfigWatch()
    if (getCurrentScope()) onScopeDispose(releaseConfigWatch)
    activatePolicyRefresh()

    if (options.tasks) {
        watch(
            options.tasks,
            (tasks) => {
                void ensureCompletionPolicies(tasks)
            },
            { immediate: true },
        )
    }

    function policyFor(prefix: string | null | undefined): CompletionPolicy | null {
        const key = (prefix ?? '').trim()
        if (!key) return null
        return policyRecords.get(key) ?? null
    }

    return {
        /** Latest server-local calendar day (YYYY-MM-DD) seen from configs. */
        serverCalendarDay,
        policyFor,
        /** Terminal labels for a project prefix (empty while unresolved). */
        doneLabelsFor(prefix: string | null | undefined): string[] {
            return policyFor(prefix)?.doneLabels ?? []
        },
        /** Whether a status is terminal for the project (policy required). */
        isDoneStatus(prefix: string | null | undefined, status: string | null | undefined): boolean {
            const policy = policyFor(prefix)
            if (!policy) return false
            return policy.doneStates.has(completionStatusKey(status))
        },
        isTaskDone(task: PolicyLikeTask | null | undefined): boolean {
            return taskIsDone(policyFor(projectOf(task?.id ?? '')), task)
        },
        isTaskOverdue(task: PolicyLikeTask | null | undefined): boolean {
            return resolveIsOverdue(policyFor(projectOf(task?.id ?? '')), task)
        },
        dueBucketFor(task: PolicyLikeTask | null | undefined): TaskDueBucket | null {
            return resolveDueBucket(policyFor(projectOf(task?.id ?? '')), task)
        },
    }
}

export type CompletionPolicyApi = ReturnType<typeof useCompletionPolicy>
