import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { computed } from 'vue'
import type { TaskDTO } from '../api/types'

const showConfig = vi.hoisted(() => vi.fn())

vi.mock('../api/client', () => ({
    api: {
        showConfig: (...args: unknown[]) => showConfig(...(args as [string?])),
    },
}))

import {
    ensureCompletionPolicies,
    ensureCompletionPolicy,
    invalidateCompletionPolicies,
    localDueBucketOf,
    normalizeStatusKey,
    probeServerCalendarDay,
    refreshCompletionPolicies,
    refreshCompletionPoliciesForScope,
    refreshCompletionPolicy,
    registerTaskQueryInvalidator,
    resolveDueBucket,
    resolveIsOverdue,
    taskIsDone,
    useCompletionPolicy,
} from '../composables/useCompletionPolicy'

function configFixture(overrides: Record<string, unknown> = {}) {
    return {
        issue_states: ['Todo', 'Doing', 'Done', 'Closed'],
        effective_done_states: ['Done'],
        done_states_mode: 'inferred',
        task_calendar_day: '2026-01-05',
        ...overrides,
    }
}

function taskFixture(overrides: Partial<TaskDTO> = {}): TaskDTO {
    return {
        id: 'ACME-1',
        title: 'Task',
        status: 'Todo',
        priority: 'High',
        task_type: 'Task',
        created: '2026-01-01T00:00:00Z',
        modified: '2026-01-01T00:00:00Z',
        tags: [],
        relationships: {},
        comments: [],
        references: [],
        sprints: [],
        history: [],
        custom_fields: {},
        ...overrides,
    } as TaskDTO
}

describe('useCompletionPolicy', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        vi.setSystemTime(new Date('2026-01-05T12:00:00'))
        showConfig.mockReset()
        invalidateCompletionPolicies()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    describe('policy extraction', () => {
        it('extracts explicit done states, mode, and calendar day from config/show', async () => {
            showConfig.mockResolvedValue(configFixture({
                effective_done_states: ['Done', 'Shipped'],
                done_states_mode: 'explicit',
                task_calendar_day: '2026-02-03',
            }))

            await ensureCompletionPolicy('ACME')

            expect(showConfig).toHaveBeenCalledWith('ACME')
            const completion = useCompletionPolicy()
            const policy = completion.policyFor('ACME')!
            expect(policy.doneLabels).toEqual(['Done', 'Shipped'])
            expect(policy.mode).toBe('explicit')
            expect(policy.calendarDay).toBe('2026-02-03')
            expect(completion.serverCalendarDay.value).toBe('2026-02-03')
        })

        it('fails closed when the server sends no resolved list (no client-side guessing)', async () => {
            showConfig.mockResolvedValue({ issue_states: ['Todo', 'Done'] })

            await ensureCompletionPolicy('ACME')

            expect(useCompletionPolicy().policyFor('ACME')).toBeNull()
        })

        it('caches per project prefix and never applies one project policy to another', async () => {
            showConfig.mockImplementation((project?: string) =>
                project === 'ACME'
                    ? configFixture({ effective_done_states: ['Done'] })
                    : configFixture({ effective_done_states: ['Closed'] }),
            )

            await ensureCompletionPolicies([taskFixture({ id: 'ACME-1' }), taskFixture({ id: 'BETA-2' })])

            const completion = useCompletionPolicy()
            expect(completion.policyFor('ACME')!.doneLabels).toEqual(['Done'])
            expect(completion.policyFor('BETA')!.doneLabels).toEqual(['Closed'])
            // An unseen project has no policy: task_state fallback, not a
            // borrowed neighbor policy.
            expect(completion.policyFor('GAMA')).toBeNull()
            expect(completion.isTaskDone(taskFixture({ id: 'GAMA-1', status: 'Done', task_state: { is_done: false, due_bucket: null, calendar_day: '2026-01-05' } }))).toBe(false)
        })

        it('ignores requests without a prefix and dedupes concurrent ensures', async () => {
            await expect(ensureCompletionPolicy('   ')).resolves.toBeUndefined()
            expect(showConfig).not.toHaveBeenCalled()

            let resolveFetch!: (value: unknown) => void
            showConfig.mockReturnValue(new Promise((resolve) => { resolveFetch = resolve }))
            const first = ensureCompletionPolicy('ACME')
            const second = ensureCompletionPolicy('ACME')
            resolveFetch(configFixture())
            await Promise.all([first, second])
            expect(showConfig).toHaveBeenCalledTimes(1)
        })

        it('keeps failing closed when the config fetch rejects', async () => {
            showConfig.mockRejectedValue(new Error('boom'))
            await expect(ensureCompletionPolicy('ACME')).resolves.toBeUndefined()
            expect(useCompletionPolicy().policyFor('ACME')).toBeNull()
        })
    })

    describe('completion precedence', () => {
        it('derives done from the fresh policy status, not a stale is_done flag', async () => {
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Done', 'Closed'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            // Optimistic reopen: status left Done but server metadata is stale.
            const reopened = taskFixture({
                status: 'Doing',
                task_state: { is_done: true, due_bucket: null, calendar_day: '2026-01-05' },
            })
            expect(completion.isTaskDone(reopened)).toBe(false)

            // Optimistic close: status switched to a terminal state.
            const closed = taskFixture({
                status: 'Closed',
                task_state: { is_done: false, due_bucket: 'today', calendar_day: '2026-01-05' },
            })
            expect(completion.isTaskDone(closed)).toBe(true)

            // Matching status/flag still agree.
            expect(completion.isTaskDone(taskFixture({ status: 'Done', task_state: { is_done: true, due_bucket: null, calendar_day: '2026-01-05' } }))).toBe(true)
        })

        it('falls back to task_state.is_done while the policy is loading', () => {
            const completion = useCompletionPolicy()
            expect(completion.isTaskDone(taskFixture({ status: 'Shipped', task_state: { is_done: true, due_bucket: null, calendar_day: '2026-01-05' } }))).toBe(true)
            expect(completion.isTaskDone(taskFixture({ status: 'Done' }))).toBe(false)
        })

        it('treats policy matching as normalization-tolerant', async () => {
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['In Progress', 'Shipped!'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()
            expect(completion.isTaskDone(taskFixture({ status: 'in progress' }))).toBe(true)
            expect(completion.isTaskDone(taskFixture({ status: 'SHIPPED' }))).toBe(false)
        })
    })

    describe('due buckets', () => {
        it('prefers the authoritative server bucket for unchanged tasks', async () => {
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            expect(completion.dueBucketFor(taskFixture({ due_date: '2026-01-01', task_state: { is_done: false, due_bucket: 'overdue', calendar_day: '2026-01-05' } }))).toBe('overdue')
            expect(completion.dueBucketFor(taskFixture({ due_date: '2026-01-09', task_state: { is_done: false, due_bucket: 'soon', calendar_day: '2026-01-05' } }))).toBe('soon')
        })

        it('never reports overdue for a task done now, even with a stale overdue bucket', async () => {
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            const justClosed = taskFixture({
                status: 'Done',
                due_date: '2025-12-01',
                task_state: { is_done: false, due_bucket: 'overdue', calendar_day: '2026-01-05' },
            })
            expect(completion.dueBucketFor(justClosed)).toBeNull()
            expect(completion.isTaskOverdue(justClosed)).toBe(false)
        })

        it('keeps due-today for done tasks (only overdue excludes done)', async () => {
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            expect(completion.dueBucketFor(taskFixture({
                status: 'Done',
                due_date: '2026-01-05',
                task_state: { is_done: true, due_bucket: 'today', calendar_day: '2026-01-05' },
            }))).toBe('today')
        })

        it('derives locally after an optimistic reopen (stale done metadata)', async () => {
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            const reopened = taskFixture({
                status: 'Doing',
                due_date: '2025-12-01',
                task_state: { is_done: true, due_bucket: null, calendar_day: '2026-01-05' },
            })
            expect(completion.dueBucketFor(reopened)).toBe('overdue')
            expect(completion.isTaskOverdue(reopened)).toBe(true)
        })

        it('uses server buckets without a policy and local buckets without either', () => {
            const completion = useCompletionPolicy()
            expect(completion.dueBucketFor(taskFixture({ task_state: { is_done: false, due_bucket: 'later', calendar_day: '2026-01-05' } }))).toBe('later')
            expect(completion.dueBucketFor(taskFixture({ due_date: '2026-01-05' }))).toBe('today')
            expect(completion.dueBucketFor(taskFixture({ due_date: '2025-12-31' }))).toBe('overdue')
            expect(completion.dueBucketFor(taskFixture({ due_date: '2026-01-06' }))).toBe('soon')
            expect(completion.dueBucketFor(taskFixture({ due_date: '2026-02-01' }))).toBe('later')
            expect(completion.dueBucketFor(taskFixture({}))).toBeNull()
        })

        it('treats due-today as not overdue in the local fallback (start of day)', () => {
            vi.setSystemTime(new Date('2026-01-05T00:30:00'))
            expect(localDueBucketOf({ due_date: '2026-01-05' })).toBe('today')
            expect(resolveIsOverdue(null, { due_date: '2026-01-05' })).toBe(false)
        })
    })

    describe('invalidation and refresh', () => {
        it('invalidation drops cached policies so the next ensure refetches', async () => {
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            expect(showConfig).toHaveBeenCalledTimes(1)

            invalidateCompletionPolicies('ACME')
            expect(useCompletionPolicy().policyFor('ACME')).toBeNull()

            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Closed'] }))
            await ensureCompletionPolicy('ACME')
            expect(showConfig).toHaveBeenCalledTimes(2)
            expect(useCompletionPolicy().policyFor('ACME')!.doneLabels).toEqual(['Closed'])
        })

        it('refreshCompletionPolicies refetches cached prefixes so the calendar day cannot stick', async () => {
            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-05' }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            // Server day rolls over after local midnight refresh.
            showConfig.mockResolvedValue(configFixture({
                task_calendar_day: '2026-01-06',
                effective_done_states: ['Done'],
            }))
            await refreshCompletionPolicies()

            expect(completion.serverCalendarDay.value).toBe('2026-01-06')
            // A task due "yesterday" now flips to overdue through the fresh
            // server bucket instead of a frozen date.
            expect(completion.dueBucketFor(taskFixture({
                due_date: '2026-01-05',
                task_state: { is_done: false, due_bucket: 'overdue', calendar_day: '2026-01-06' },
            }))).toBe('overdue')
        })

        it('re-evaluates computeds when a late policy lands (no stale default render)', async () => {
            let resolveFetch!: (value: unknown) => void
            showConfig.mockReturnValue(new Promise((resolve) => { resolveFetch = resolve }))

            const completion = useCompletionPolicy()
            const isDone = computed(() => completion.isTaskDone(taskFixture({ id: 'ACME-9', status: 'Done' })))
            expect(isDone.value).toBe(false)

            const pending = ensureCompletionPolicy('ACME')
            resolveFetch(configFixture({ effective_done_states: ['Done'] }))
            await pending

            expect(isDone.value).toBe(true)
        })

        it('a failed forced refresh keeps the valid cached policy (no false clearing)', async () => {
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            showConfig.mockRejectedValue(new Error('flaky'))
            await expect(refreshCompletionPolicies()).resolves.toBeUndefined()

            expect(completion.policyFor('ACME')!.doneLabels).toEqual(['Done'])
            expect(completion.isTaskDone(taskFixture({ id: 'ACME-1', status: 'Done' }))).toBe(true)
        })

        it('a late stale response never publishes over a newer refresh', async () => {
            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-05' }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            // A forced refresh with a slow OLD response in flight...
            let resolveOld!: (value: unknown) => void
            showConfig.mockImplementation(() => new Promise((resolve) => { resolveOld = resolve }))
            const slowOld = refreshCompletionPolicy('ACME')

            // ...superseded by an immediate forced refresh carrying the NEW
            // day (bumps the sequence: the slow response cannot publish).
            showConfig.mockImplementation(async () => configFixture({ task_calendar_day: '2026-01-06' }))
            await refreshCompletionPolicy('ACME')
            expect(completion.policyFor('ACME')!.calendarDay).toBe('2026-01-06')

            // The stale response resolving last must be dropped.
            resolveOld(configFixture({ task_calendar_day: '2026-01-05' }))
            await slowOld
            expect(completion.policyFor('ACME')!.calendarDay).toBe('2026-01-06')
        })

        it('scope-aware refresh after a same-tab save touches only affected prefixes', async () => {
            showConfig.mockImplementation((project?: string) =>
                configFixture({ effective_done_states: project === 'BETA' ? ['Closed'] : ['Done'] }))
            await ensureCompletionPolicies([taskFixture({ id: 'ACME-1' }), taskFixture({ id: 'BETA-2' })])
            expect(showConfig).toHaveBeenCalledTimes(2)

            // Loaded same-tab consumer sees the fresh value after the save.
            const completion = useCompletionPolicy()
            expect(completion.policyFor('ACME')!.doneLabels).toEqual(['Done'])

            // Project save: only that prefix refetches.
            showConfig.mockImplementation((project?: string) =>
                configFixture({ effective_done_states: project === 'ACME' ? ['Shipped'] : project === 'BETA' ? ['Closed'] : ['Done'] }))
            await refreshCompletionPoliciesForScope('ACME')
            expect(showConfig).toHaveBeenCalledTimes(3)
            expect(completion.policyFor('ACME')!.doneLabels).toEqual(['Shipped'])
            expect(completion.policyFor('BETA')!.doneLabels).toEqual(['Closed'])

            // Global save: every cached prefix refreshes exactly once.
            await refreshCompletionPoliciesForScope('')
            expect(showConfig).toHaveBeenCalledTimes(5)

            // Back-to-back refreshes are coalesced by the dedupe window.
            await refreshCompletionPolicies()
            expect(showConfig).toHaveBeenCalledTimes(5)
        })
    })

    describe('server calendar day probe (F1)', () => {
        const invalidator = vi.fn()

        beforeEach(() => {
            registerTaskQueryInvalidator(invalidator)
        })

        afterEach(() => {
            registerTaskQueryInvalidator(null)
        })

        function staleEntity(overrides: Partial<TaskDTO> = {}) {
            // Entity loaded on the OLD server day; never refetched.
            return taskFixture({
                id: 'ACME-7',
                status: 'Todo',
                due_date: '2026-01-05',
                task_state: { is_done: false, due_bucket: 'today', calendar_day: '2026-01-05' },
                ...overrides,
            })
        }

        it('does not trust a stale authoritative bucket once the server day moved (server basis, not browser clock)', async () => {
            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-05', effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()
            const entity = staleEntity()

            // Fresh DTO on its own calendar day: server bucket wins.
            expect(completion.dueBucketFor(entity)).toBe('today')

            // Server day advances while the BROWSER day stays 2026-01-05
            // (cross-TZ): refresh the policy, entity untouched.
            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-06', effective_done_states: ['Done'] }))
            await refreshCompletionPolicies()
            expect(completion.serverCalendarDay.value).toBe('2026-01-06')

            // Date-only due: re-derived against the SERVER day by calendar
            // strings — the old 'today' bucket is not trusted, and the
            // browser-local derivation (still 'today') is not used either.
            expect(localDueBucketOf(entity)).toBe('today')
            expect(completion.dueBucketFor(entity)).toBe('overdue')
            expect(completion.isTaskOverdue(entity)).toBe(true)
        })

        it('returns neutral null for a known-stale timestamp snapshot until entities refetch', async () => {
            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-05', effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-06', effective_done_states: ['Done'] }))
            await refreshCompletionPolicies()

            const entity = staleEntity({ due_date: '2026-01-05T23:30:00Z' })
            // Timestamp dues are not comparable by calendar string: the stale
            // snapshot yields NO bucket — not the old 'today' (or 'overdue')
            // masquerading as current — until the retained query refetches.
            expect(completion.dueBucketFor(entity)).toBeNull()
            expect(completion.isTaskOverdue(entity)).toBe(false)
        })

        it('terminal tasks stay non-overdue after the server day moves', async () => {
            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-05', effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-06', effective_done_states: ['Done'] }))
            await refreshCompletionPolicies()

            const doneEntity = taskFixture({
                id: 'ACME-8',
                status: 'Done',
                due_date: '2026-01-05',
                task_state: { is_done: true, due_bucket: 'today', calendar_day: '2026-01-05' },
            })
            expect(completion.dueBucketFor(doneEntity)).toBeNull()
            expect(completion.isTaskOverdue(doneEntity)).toBe(false)
        })

        it('one global probe detects the day change, refreshes policies, and invalidates retained queries', async () => {
            showConfig.mockImplementation((project?: string) =>
                project === undefined
                    ? Promise.resolve({ task_calendar_day: '2026-01-05' })
                    : Promise.resolve(configFixture({ task_calendar_day: '2026-01-05', effective_done_states: ['Done'] })))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()
            invalidator.mockClear()

            // Server rolls: the GLOBAL probe response moves to the next day.
            showConfig.mockImplementation((project?: string) =>
                project === undefined
                    ? Promise.resolve({ task_calendar_day: '2026-01-06' })
                    : Promise.resolve(configFixture({ task_calendar_day: '2026-01-06', effective_done_states: ['Done'] })))

            await probeServerCalendarDay()

            // Single shared global call — not one probe per cached project.
            expect(showConfig).toHaveBeenCalledWith()
            expect(showConfig.mock.calls.filter(([project]) => project === undefined)).toHaveLength(1)

            // Policy refreshed for the new day...
            expect(completion.serverCalendarDay.value).toBe('2026-01-06')
            expect(completion.policyFor('ACME')!.calendarDay).toBe('2026-01-06')
            // ...and the retained task queries were asked to refetch their
            // (now provably stale) task_state entities.
            expect(invalidator).toHaveBeenCalledTimes(1)

            // The OLD entity — not a fed fresh DTO — is re-classified via the
            // server day immediately.
            expect(completion.dueBucketFor(staleEntity())).toBe('overdue')
        })

        it('probe with an unchanged server day refreshes nothing', async () => {
            showConfig.mockImplementation((project?: string) =>
                project === undefined
                    ? Promise.resolve({ task_calendar_day: '2026-01-05' })
                    : Promise.resolve(configFixture({ task_calendar_day: '2026-01-05', effective_done_states: ['Done'] })))
            await ensureCompletionPolicy('ACME')
            const acmeFetches = showConfig.mock.calls.filter(([project]) => project === 'ACME').length
            invalidator.mockClear()

            await probeServerCalendarDay()

            expect(showConfig.mock.calls.filter(([project]) => project === 'ACME')).toHaveLength(acmeFetches)
            expect(invalidator).not.toHaveBeenCalled()
        })

        it('a failed probe keeps current knowledge and valid caches', async () => {
            showConfig.mockImplementation((project?: string) =>
                project === undefined
                    ? Promise.resolve({ task_calendar_day: '2026-01-05' })
                    : Promise.resolve(configFixture({ task_calendar_day: '2026-01-05', effective_done_states: ['Done'] })))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()
            invalidator.mockClear()

            showConfig.mockImplementation((project?: string) =>
                project === undefined ? Promise.reject(new Error('probe failed')) : Promise.resolve(configFixture()))
            await expect(probeServerCalendarDay()).resolves.toBeUndefined()

            expect(completion.serverCalendarDay.value).toBe('2026-01-05')
            expect(completion.policyFor('ACME')!.doneLabels).toEqual(['Done'])
            expect(invalidator).not.toHaveBeenCalled()
        })

        it('the probe interval fires while policies are cached (registered timer callback)', async () => {
            showConfig.mockImplementation((project?: string) =>
                project === undefined
                    ? Promise.resolve({ task_calendar_day: '2026-01-05' })
                    : Promise.resolve(configFixture({ task_calendar_day: '2026-01-05', effective_done_states: ['Done'] })))
            await ensureCompletionPolicy('ACME')
            expect(showConfig.mock.calls.filter(([project]) => project === undefined)).toHaveLength(0)

            // Advance the real registered interval callback — the probe runs
            // (day unchanged: no refresh, no invalidation).
            await vi.advanceTimersByTimeAsync(60_000)
            expect(showConfig.mock.calls.filter(([project]) => project === undefined)).toHaveLength(1)
            expect(invalidator).not.toHaveBeenCalled()

            await vi.advanceTimersByTimeAsync(60_000)
            expect(showConfig.mock.calls.filter(([project]) => project === undefined)).toHaveLength(2)
        })
    })

    describe('embedded actual-root done states (root homonyms)', () => {
        const invalidator = vi.fn()

        beforeEach(() => {
            registerTaskQueryInvalidator(invalidator)
        })

        afterEach(() => {
            registerTaskQueryInvalidator(null)
        })

        it('matches server completion labels without collapsing separators or Unicode case', () => {
            const completion = useCompletionPolicy()
            for (const [doneState, status] of [
                ['Shipped-Now', 'Shipped Now'],
                ['Ärger', 'ärger'],
            ] as const) {
                expect(completion.isTaskDone(taskFixture({
                    status,
                    task_state: { done_states: [doneState], is_done: false, due_bucket: 'overdue', calendar_day: '2026-01-05' },
                }))).toBe(false)
            }
        })

        it('treats an embedded empty set as authoritative rather than inferring cached Done', async () => {
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()
            expect(completion.isTaskDone(taskFixture({
                id: 'ACME-1', status: 'Done',
                task_state: { done_states: [], is_done: false, due_bucket: 'overdue', calendar_day: '2026-01-05' },
            }))).toBe(false)
        })

        it('reclassifies optimistic date-only reopens against the server day, not the browser day', async () => {
            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-04' }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()
            const reopened = taskFixture({
                id: 'ACME-1', status: 'Todo', due_date: '2026-01-04',
                task_state: { done_states: ['Done'], is_done: true, due_bucket: 'today', calendar_day: '2026-01-04' },
            })
            expect(completion.dueBucketFor(reopened)).toBe('today')
            expect(completion.dueBucketFor({ ...reopened, due_date: '2026-01-04T23:30:00-08:00' })).toBeNull()
        })

        it('embedded per-task sets beat the cached prefix policy: same prefix, different roots, DONE false/true', async () => {
            // showConfig('ACME') resolves a CONFLICTING policy for one of
            // the two homonymous-root tasks — it must lose to both embedded
            // sets.
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Done'], task_calendar_day: '2026-01-05' }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()

            const shippedRoot = taskFixture({
                id: 'ACME-1',
                status: 'Done',
                task_state: { done_states: ['Shipped'], is_done: false, due_bucket: null, calendar_day: '2026-01-05' },
            })
            const doneRoot = taskFixture({
                id: 'ACME-2',
                status: 'Done',
                task_state: { done_states: ['Done'], is_done: true, due_bucket: null, calendar_day: '2026-01-05' },
            })

            // Embedded set drives classification per task, not the shared
            // prefix cache (which would call both done).
            expect(completion.isTaskDone(shippedRoot)).toBe(false)
            expect(completion.isTaskDone(doneRoot)).toBe(true)

            // ...and the embedded set wins in the other direction too: a
            // Shipped status under the [Done]-embedded task is NOT done even
            // though the cached policy happens to also exclude it.
            expect(completion.isTaskDone(taskFixture({
                id: 'ACME-2',
                status: 'Shipped',
                task_state: { done_states: ['Done'], is_done: false, due_bucket: null, calendar_day: '2026-01-05' },
            }))).toBe(false)
            // Cached prefix policy still classifies a legacy fixture without
            // metadata (guarded fallback path preserved).
            expect(completion.isTaskDone(taskFixture({ id: 'ACME-3', status: 'Done' }))).toBe(true)
            expect(completion.isTaskDone(taskFixture({ id: 'ACME-3', status: 'Shipped' }))).toBe(false)
        })

        it('optimistic Done->Todo->Done uses the embedded set, not the stale boolean or a neighbor cache', () => {
            const completion = useCompletionPolicy()
            const base = {
                id: 'ACME-9',
                due_date: '2025-12-01',
                task_state: { done_states: ['Done'], is_done: true, due_bucket: null, calendar_day: '2026-01-05' },
            }

            const closed = taskFixture({ ...base, status: 'Done' })
            expect(completion.isTaskDone(closed)).toBe(true)
            expect(completion.isTaskOverdue(closed)).toBe(false)

            // Optimistically reopened: stale is_done stays true on the DTO,
            // the embedded set + current status decide.
            const reopened = taskFixture({ ...base, status: 'Todo' })
            expect(completion.isTaskDone(reopened)).toBe(false)
            expect(completion.isTaskOverdue(reopened)).toBe(true)

            // Closed again from the same embedded set.
            const reclosed = taskFixture({ ...base, status: 'Done' })
            expect(completion.isTaskDone(reclosed)).toBe(true)
            expect(completion.isTaskOverdue(reclosed)).toBe(false)

            // No prefix policy was ever consulted for this task.
            expect(showConfig).not.toHaveBeenCalled()
        })

        it('embedded done-due-today stays today; terminal past-due embeds stay null (fresh snapshot)', () => {
            const completion = useCompletionPolicy()
            expect(completion.dueBucketFor(taskFixture({
                id: 'ACME-4',
                status: 'Done',
                due_date: '2026-01-05',
                task_state: { done_states: ['Done'], is_done: true, due_bucket: 'today', calendar_day: '2026-01-05' },
            }))).toBe('today')
            expect(completion.dueBucketFor(taskFixture({
                id: 'ACME-5',
                status: 'Shipped',
                due_date: '2025-12-01',
                task_state: { done_states: ['Shipped'], is_done: true, due_bucket: null, calendar_day: '2026-01-05' },
            }))).toBeNull()
        })

        it('embedded set drives a stale snapshot re-derivation against the server day', async () => {
            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-05', effective_done_states: ['Done'] }))
            await ensureCompletionPolicy('ACME')
            const completion = useCompletionPolicy()
            showConfig.mockResolvedValue(configFixture({ task_calendar_day: '2026-01-06', effective_done_states: ['Done'] }))
            await refreshCompletionPolicies()

            // Open task, date-only due, stale snapshot: server-day derivation.
            expect(completion.dueBucketFor(taskFixture({
                id: 'ACME-6',
                status: 'Todo',
                due_date: '2026-01-05',
                task_state: { done_states: ['Shipped'], is_done: false, due_bucket: 'today', calendar_day: '2026-01-05' },
            }))).toBe('overdue')
            // Done-now under the embedded set: overdue derivation maps to null.
            expect(completion.dueBucketFor(taskFixture({
                id: 'ACME-7',
                status: 'Shipped',
                due_date: '2026-01-05',
                task_state: { done_states: ['Shipped'], is_done: false, due_bucket: 'today', calendar_day: '2026-01-05' },
            }))).toBeNull()
        })

        it('policy-bearing config changes invalidate retained queries so embedded sets refetch', async () => {
            showConfig.mockResolvedValue(configFixture({ effective_done_states: ['Done'], task_calendar_day: '2026-01-05' }))
            await ensureCompletionPolicy('ACME')
            invalidator.mockClear()

            // config_updated SSE path: full refresh.
            await refreshCompletionPolicies()
            expect(invalidator).toHaveBeenCalledTimes(1)

            // Same-tab save path (project scope): refresh + invalidation.
            await refreshCompletionPoliciesForScope('ACME')
            expect(invalidator).toHaveBeenCalledTimes(2)

            // Global save scope goes through the same refresh+invalidate.
            await invalidateCompletionPolicies()
            await ensureCompletionPolicy('ACME')
            invalidator.mockClear()
            await refreshCompletionPoliciesForScope('')
            expect(invalidator).toHaveBeenCalledTimes(1)
        })
    })

    describe('pure predicate helpers', () => {
        it('normalizeStatusKey collapses case, whitespace, and separators', () => {
            expect(normalizeStatusKey(' In-Prog_ress ')).toBe('inprogress')
            expect(normalizeStatusKey(null)).toBe('')
        })

        it('taskIsDone without policy trusts only task_state', () => {
            expect(taskIsDone(null, { status: 'Done' })).toBe(false)
            expect(taskIsDone(null, { status: 'Anything', task_state: { is_done: true, due_bucket: null, calendar_day: '2026-01-05' } })).toBe(true)
        })

        it('resolveDueBucket without policy or state derives locally and overdue stays possible', () => {
            expect(resolveDueBucket(null, { due_date: '2025-12-01' })).toBe('overdue')
        })
    })
})
