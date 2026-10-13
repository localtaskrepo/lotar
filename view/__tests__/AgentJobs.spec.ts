import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentJob, AgentJobCancelAllResponse, AgentJobLogEntry, AgentJobLogsResponse } from '../api/types'
import AgentJobs from '../pages/AgentJobs.vue'

const sseCalls: Array<{
    params: Record<string, string>
    handlers: Map<string, Set<(ev: { data: string }) => void>>
    lifecycle: Map<string, Set<() => void>>
    opts: { onReconnect?: () => void }
    close: ReturnType<typeof vi.fn>
}> = []

vi.mock('../api/client', () => ({
    api: {
        listAgentJobs: vi.fn(),
        getAgentJobLogs: vi.fn(),
        cancelAgentJob: vi.fn(),
        cancelAllAgentJobs: vi.fn(),
        sendAgentJobMessage: vi.fn(),
    },
}))

vi.mock('../composables/useSse', () => ({
    useSse: (_path: string, params: Record<string, string>, opts: { onReconnect?: () => void } = {}) => {
        const handlers = new Map<string, Set<(ev: { data: string }) => void>>()
        const lifecycle = new Map<string, Set<() => void>>()
        const close = vi.fn()
        sseCalls.push({ params, handlers, lifecycle, opts, close })
        return {
            es: {
                addEventListener: vi.fn((event: string, handler: () => void) => {
                    if (!lifecycle.has(event)) lifecycle.set(event, new Set())
                    lifecycle.get(event)!.add(handler)
                }),
                removeEventListener: vi.fn((event: string, handler: () => void) => {
                    lifecycle.get(event)?.delete(handler)
                }),
            } as unknown as EventSource,
            on: vi.fn((event: string, handler: (ev: { data: string }) => void) => {
                if (event === 'open') {
                    if (!lifecycle.has(event)) lifecycle.set(event, new Set())
                    lifecycle.get(event)!.add(handler as unknown as () => void)
                    return
                }
                if (!handlers.has(event)) handlers.set(event, new Set())
                handlers.get(event)!.add(handler)
            }),
            off: vi.fn((event: string, handler: (ev: { data: string }) => void) => {
                if (event === 'open') lifecycle.get(event)?.delete(handler as unknown as () => void)
                handlers.get(event)?.delete(handler)
            }),
            close,
        }
    },
}))

import { api } from '../api/client'

const stubs = {
    UiInput: {
        template: '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
        props: ['modelValue'],
        emits: ['update:modelValue'],
    },
    UiSelect: {
        template: '<select :value="modelValue" @change="$emit(\'update:modelValue\', $event.target.value)"><slot /></select>',
        props: ['modelValue'],
        emits: ['update:modelValue'],
    },
    UiButton: {
        props: ['type'],
        // Declare the click emit so a trigger('click') fires the parent
        // handler exactly once, matching the real component's native
        // fallthrough behavior (no fallthrough + emit double fire).
        emits: ['click'],
        template: '<button :type="type || \'button\'" @click="$emit(\'click\')"><slot /></button>',
    },
    UiCard: { template: '<section><slot /></section>' },
    UiEmptyState: { template: '<div><slot /></div>' },
    ReloadButton: { template: '<button type="button" @click="$emit(\'click\')">Reload</button>', props: ['loading'] },
    // Render <Teleport> content in place so wrapper.find can reach the dialog.
    teleport: true,
}

function deferred<T>() {
    let resolve!: (value: T) => void
    let reject!: (reason?: unknown) => void
    const promise = new Promise<T>((res, rej) => {
        resolve = res
        reject = rej
    })
    return { promise, resolve, reject }
}

function job(id: string, overrides: Partial<AgentJob> = {}): AgentJob {
    return {
        id,
        ticket_id: 'DEV-1',
        runner: 'cli',
        agent: 'reviewer',
        status: 'running',
        created_at: `2026-10-08T10:00:0${id.length % 10}.000Z`,
        started_at: '2026-10-08T10:00:01.000Z',
        finished_at: null,
        exit_code: null,
        last_message: null,
        summary: null,
        session_id: null,
        worktree_path: null,
        worktree_branch: null,
        ...overrides,
    }
}

const runningJob = job('job-running', { created_at: '2026-10-08T10:00:01.000Z' })
const queuedJob = job('job-queued', { status: 'queued', started_at: null, created_at: '2026-10-08T10:00:02.000Z' })
const completedJob = job('job-done', { status: 'completed', finished_at: '2026-10-08T10:00:03.000Z', created_at: '2026-10-08T09:00:00.000Z' })

function currentSse() {
    return sseCalls[sseCalls.length - 1]!
}

function emitSse(kind: string, payload: unknown) {
    const ev = { data: JSON.stringify(payload) }
    currentSse().handlers.get(kind)?.forEach((handler) => handler(ev))
}

/** Simulate the EventSource transition to OPEN (first open or reconnect). */
function openSse() {
    currentSse().lifecycle.get('open')?.forEach((handler) => handler())
}

async function mountAgentJobs() {
    // Attached mounting lets the shared modal primitive activate its dialog stack,
    // focus management and Escape handling the way a real document would.
    const wrapper = mount(AgentJobs, { attachTo: document.body, global: { stubs } })
    await flushPromises()
    return wrapper
}

function stopAllButton(wrapper: ReturnType<typeof mount>) {
    const button = wrapper.findAll('button').find((candidate) => candidate.text() === 'Stop all')
    expect(button, 'Stop all button').toBeTruthy()
    return button!
}

function dialog(wrapper: ReturnType<typeof mount>) {
    return wrapper.find('[role="dialog"]')
}

function dialogButton(wrapper: ReturnType<typeof mount>, label: string) {
    const button = dialog(wrapper)
        .findAll('button')
        .find((candidate) => candidate.text().includes(label))
    expect(button, `dialog button ${label}`).toBeTruthy()
    return button!
}

async function openStopAllDialog(wrapper: ReturnType<typeof mount>) {
    await stopAllButton(wrapper).trigger('click')
    await flushPromises()
}

/** Dispatch Escape in a way that reaches whichever element the modal primitive listens on. */
async function pressEscape(wrapper: ReturnType<typeof mount>) {
    const target = dialog(wrapper)
    if (target.exists()) {
        await target.trigger('keydown', { key: 'Escape' })
    }
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
}

beforeEach(() => {
    sseCalls.splice(0)
    vi.clearAllMocks()
    ;(api.listAgentJobs as any).mockResolvedValue({
        jobs: [runningJob, queuedJob, completedJob],
        queue_stats: { running: 1, queued: 1, max_parallel: 2 },
    })
    ;(api.getAgentJobLogs as any).mockResolvedValue({ job: runningJob, events: [] })
    ;(api.cancelAgentJob as any).mockResolvedValue({ cancelled: true, job: { ...runningJob, status: 'cancelled' } })
    ;(api.cancelAllAgentJobs as any).mockResolvedValue({
        cancelled: 2,
        jobs: [
            { ...runningJob, status: 'cancelled', finished_at: '2026-10-08T10:01:00.000Z' },
            { ...queuedJob, status: 'cancelled', finished_at: '2026-10-08T10:01:00.000Z' },
            completedJob,
        ],
    } satisfies AgentJobCancelAllResponse)
})

afterEach(() => {
    vi.restoreAllMocks()
})

describe('AgentJobs stop-all confirmation', () => {
    it('settles the initial loading state when stream-open reconciliation supersedes its request', async () => {
        const initial = deferred<{ jobs: AgentJob[] }>()
        const reconcile = deferred<{ jobs: AgentJob[] }>()
        ;(api.listAgentJobs as any).mockReturnValueOnce(initial.promise).mockReturnValueOnce(reconcile.promise)
        const wrapper = await mountAgentJobs()
        try {
            expect(wrapper.text()).toContain('Loading jobs')
            openSse()
            reconcile.resolve({ jobs: [] })
            await flushPromises()
            expect(wrapper.text()).not.toContain('Loading jobs')
            initial.resolve({ jobs: [runningJob] })
            await flushPromises()
            expect(wrapper.findAll('.job-card')).toHaveLength(0)
        } finally {
            wrapper.unmount()
        }
    })

    it('reports a superseding initial reconciliation failure instead of a permanent loader or false empty success', async () => {
        const initial = deferred<{ jobs: AgentJob[] }>()
        const reconcile = deferred<{ jobs: AgentJob[] }>()
        ;(api.listAgentJobs as any).mockReturnValueOnce(initial.promise).mockReturnValueOnce(reconcile.promise)
        const wrapper = await mountAgentJobs()
        try {
            openSse()
            reconcile.reject(new Error('queue unavailable'))
            await flushPromises()
            expect(wrapper.text()).toContain('queue unavailable')
            expect(wrapper.text()).not.toContain('Loading jobs')
            initial.resolve({ jobs: [] })
            await flushPromises()
            expect(wrapper.text()).toContain('queue unavailable')
        } finally {
            wrapper.unmount()
        }
    })

    it('opens an explicit confirmation dialog instead of calling the API immediately', async () => {
        const wrapper = await mountAgentJobs()

        await openStopAllDialog(wrapper)

        expect(dialog(wrapper).exists()).toBe(true)
        expect(wrapper.find('.ui-modal__overlay').exists()).toBe(true)
        expect(wrapper.text()).toContain('Stop all queued and running agent jobs?')
        expect(api.cancelAllAgentJobs).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('binds the dialog name to the visible heading and description', async () => {
        const wrapper = await mountAgentJobs()
        await openStopAllDialog(wrapper)

        const dialogEl = dialog(wrapper)
        const labelledBy = dialogEl.attributes('aria-labelledby')
        expect(labelledBy).toBeTruthy()
        const heading = wrapper.find('.stop-all-dialog__title')
        expect(heading.attributes('id')).toBe(labelledBy)
        expect(heading.text()).toBe('Stop all agent jobs')
        const describedBy = dialogEl.attributes('aria-describedby')
        expect(describedBy).toBeTruthy()
        expect(wrapper.find('.stop-all-dialog__message').attributes('id')).toBe(describedBy)
        expect(dialogEl.attributes('aria-label')).toBe('Stop all agent jobs')
        wrapper.unmount()
    })

    it('marks the safe Cancel action as the initial focus target', async () => {
        const wrapper = await mountAgentJobs()
        await openStopAllDialog(wrapper)

        expect(dialogButton(wrapper, 'Cancel').attributes('data-autofocus')).toBeDefined()
        wrapper.unmount()
    })

    it('never calls the API when dismissed via Cancel, backdrop, or Escape', async () => {
        const wrapper = await mountAgentJobs()

        await openStopAllDialog(wrapper)
        await dialogButton(wrapper, 'Cancel').trigger('click')
        await flushPromises()
        expect(dialog(wrapper).exists()).toBe(false)

        await openStopAllDialog(wrapper)
        await wrapper.find('.ui-modal__overlay').trigger('click')
        await flushPromises()
        expect(dialog(wrapper).exists()).toBe(false)

        await openStopAllDialog(wrapper)
        await pressEscape(wrapper)
        expect(dialog(wrapper).exists()).toBe(false)

        expect(api.cancelAllAgentJobs).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('confirms exactly once with a double-click guard and refreshes state', async () => {
        const stop = deferred<AgentJobCancelAllResponse>()
        ;(api.cancelAllAgentJobs as any).mockReturnValue(stop.promise)
        const wrapper = await mountAgentJobs()

        await openStopAllDialog(wrapper)
        await dialogButton(wrapper, 'Stop all').trigger('click')
        // The button relabels to "Stopping…" while busy; a second click must not re-fire.
        await dialogButton(wrapper, 'Stopping').trigger('click')
        await flushPromises()
        expect(api.cancelAllAgentJobs).toHaveBeenCalledTimes(1)

        stop.resolve({
            cancelled: 2,
            jobs: [
                { ...runningJob, status: 'cancelled' },
                { ...queuedJob, status: 'cancelled' },
                completedJob,
            ],
        })
        await flushPromises()

        expect(dialog(wrapper).exists()).toBe(false)
        // The returned job list replaces the page state and the queue stats refresh.
        expect(wrapper.text()).toContain('cancelled')
        expect(api.listAgentJobs).toHaveBeenCalledTimes(2)
        wrapper.unmount()
    })

    it('locks dismissal and controls while the stop-all request is in flight', async () => {
        const stop = deferred<AgentJobCancelAllResponse>()
        ;(api.cancelAllAgentJobs as any).mockReturnValue(stop.promise)
        const wrapper = await mountAgentJobs()

        await openStopAllDialog(wrapper)
        await dialogButton(wrapper, 'Stop all').trigger('click')
        await flushPromises()

        expect(dialog(wrapper).exists()).toBe(true)
        expect(dialogButton(wrapper, 'Stopping').attributes('disabled')).toBeDefined()
        expect(dialogButton(wrapper, 'Cancel').attributes('disabled')).toBeDefined()

        await pressEscape(wrapper)
        await wrapper.find('.ui-modal__overlay').trigger('click')
        await flushPromises()
        expect(dialog(wrapper).exists()).toBe(true)
        expect(api.cancelAllAgentJobs).toHaveBeenCalledTimes(1)

        stop.resolve({
            cancelled: 2,
            jobs: [
                { ...runningJob, status: 'cancelled' },
                { ...queuedJob, status: 'cancelled' },
                completedJob,
            ],
        })
        await flushPromises()
        expect(dialog(wrapper).exists()).toBe(false)
        wrapper.unmount()
    })

    it('keeps the dialog open and reports errors instead of losing context', async () => {
        const wrapper = await mountAgentJobs()

        await openStopAllDialog(wrapper)
        ;(api.cancelAllAgentJobs as any).mockRejectedValueOnce(new Error('queue unreachable'))
        await dialogButton(wrapper, 'Stop all').trigger('click')
        await flushPromises()

        expect(dialog(wrapper).exists()).toBe(true)
        expect(wrapper.find('[data-testid="stop-all-dialog-error"]').text()).toContain('queue unreachable')

        // Dismissal still works after a failure and never fires extra requests.
        await dialogButton(wrapper, 'Cancel').trigger('click')
        await flushPromises()
        expect(dialog(wrapper).exists()).toBe(false)
        expect(api.cancelAllAgentJobs).toHaveBeenCalledTimes(1)
        wrapper.unmount()
    })

    it('revalidates cancelable jobs before mutating and closes without an API call', async () => {
        const wrapper = await mountAgentJobs()

        await openStopAllDialog(wrapper)
        // Both active jobs finish while the dialog is open.
        emitSse('agent_job_completed', { id: 'job-running', ticket_id: 'DEV-1', status: 'completed', finished_at: '2026-10-08T10:02:00.000Z' })
        emitSse('agent_job_cancelled', { id: 'job-queued', ticket_id: 'DEV-1', status: 'cancelled', finished_at: '2026-10-08T10:02:00.000Z' })
        await flushPromises()
        expect(stopAllButton(wrapper).attributes('disabled')).toBeDefined()

        await dialogButton(wrapper, 'Stop all').trigger('click')
        await flushPromises()

        expect(api.cancelAllAgentJobs).not.toHaveBeenCalled()
        expect(dialog(wrapper).exists()).toBe(false)
        wrapper.unmount()
    })

    it('keeps single-job cancel immediate without any confirmation dialog', async () => {
        const wrapper = await mountAgentJobs()

        const stop = wrapper.findAll('button').find((candidate) => candidate.text() === 'Stop')!
        await stop.trigger('click')
        await flushPromises()

        expect(api.cancelAgentJob).toHaveBeenCalledWith('job-running')
        expect(dialog(wrapper).exists()).toBe(false)
        expect(api.cancelAllAgentJobs).not.toHaveBeenCalled()
        wrapper.unmount()
    })
})

describe('AgentJobs live synchronization', () => {
    it('a stale list response cannot drop a job added by a live event', async () => {
        // Reproduces the CI failure shape: a list request is slow, the job
        // starts and is delivered over SSE during the flight, and only
        // afterwards does the (older, pre-job) server snapshot resolve.
        // The event is newer than the snapshot, so the job must survive.
        const wrapper = await mountAgentJobs()
        const list = deferred<{ jobs: AgentJob[]; queue_stats: unknown }>()
        ;(api.listAgentJobs as any).mockReturnValue(list.promise)

        const reload = wrapper.findAll('button').find((candidate) => candidate.text() === 'Reload')!
        await reload.trigger('click')
        await flushPromises()

        emitSse('agent_job_started', {
            id: 'job-live',
            ticket_id: 'DEV-1',
            agent: 'live-marker',
            status: 'running',
        })
        await flushPromises()

        // The snapshot predates the job entirely.
        list.resolve({ jobs: [], queue_stats: null })
        await flushPromises()

        expect(wrapper.findAll('.job-card').length).toBe(1)
        expect(wrapper.text()).toContain('live-marker')
        wrapper.unmount()
    })

    it('reconciles with a fresh list when the stream opens after events were missed', async () => {
        // Reproduces the CI failure shape where agent_job_started fired
        // before the EventSource finished connecting and was lost: the page
        // must not trust its pre-open snapshot once the stream is open.
        const lateJob = job('job-late', {
            agent: 'late-marker',
            created_at: '2026-10-08T10:09:00.000Z',
        })
        ;(api.listAgentJobs as any)
            .mockResolvedValueOnce({ jobs: [], queue_stats: null })
            .mockResolvedValueOnce({ jobs: [lateJob], queue_stats: { running: 1, queued: 0, max_parallel: 2 } })
        const wrapper = await mountAgentJobs()
        expect(wrapper.findAll('.job-card').length).toBe(0)

        openSse()
        await flushPromises()

        expect(api.listAgentJobs).toHaveBeenCalledTimes(2)
        expect(wrapper.findAll('.job-card').length).toBe(1)
        expect(wrapper.text()).toContain('late-marker')
        wrapper.unmount()
    })
})

describe('AgentJobs log sequence join', () => {
    const logJobA = job('job-logs-a', { agent: 'logs-alpha', created_at: '2026-10-08T10:20:01.000Z' })
    const logJobB = job('job-logs-b', { agent: 'logs-beta', created_at: '2026-10-08T10:20:02.000Z' })

    function listJobs(...jobs: AgentJob[]) {
        ;(api.listAgentJobs as any).mockResolvedValue({
            jobs,
            queue_stats: { running: jobs.length, queued: 0, max_parallel: 2 },
        })
    }

    function cardFor(wrapper: ReturnType<typeof mount>, marker: string) {
        const card = wrapper.findAll('.job-card').find((candidate) => candidate.text().includes(marker))
        expect(card, `job card ${marker}`).toBeTruthy()
        return card!
    }

    async function clickLogsToggle(wrapper: ReturnType<typeof mount>, marker: string) {
        const card = cardFor(wrapper, marker)
        const label = card.find('.log-panel').exists() ? 'Hide logs' : 'Show logs'
        const button = card.findAll('button').find((candidate) => candidate.text() === label)
        expect(button, `${label} button`).toBeTruthy()
        await button!.trigger('click')
        await flushPromises()
    }

    function logEntry(kind: string, sequence: number, at: string, message?: string): AgentJobLogEntry {
        return { kind, at, sequence, message: message ?? null }
    }

    /** Emit a broadcast-kind SSE event carrying the required sequence and server at. */
    function emitLiveSse(kind: string, payload: Record<string, unknown>) {
        emitSse(kind, { id: 'job-logs-a', ticket_id: 'DEV-1', ...payload })
    }

    function panelText(wrapper: ReturnType<typeof mount>, marker = 'logs-alpha') {
        return cardFor(wrapper, marker).find('.log-panel').text()
    }

    function occurrences(text: string, needle: string) {
        return text.split(needle).length - 1
    }

    it('fetches persisted history on open even when live events already initialized a partial cache', async () => {
        // Reproduces the smoke failure shape: broadcast events initialized a
        // partial live cache, but the phase-one progress was persisted before
        // the stream joined and was never delivered. Opening the logs must
        // still load the server history while the job is running.
        listJobs(logJobA)
        ;(api.getAgentJobLogs as any).mockResolvedValue({
            job: logJobA,
            events: [
                logEntry('agent_job_started', 1, '2026-10-08T10:20:01.100Z'),
                logEntry('agent_job_init', 2, '2026-10-08T10:20:01.150Z', 'runner ready'),
                logEntry('agent_job_progress', 3, '2026-10-08T10:20:01.300Z', 'phase one complete'),
            ],
        } satisfies AgentJobLogsResponse)
        const wrapper = await mountAgentJobs()
        try {
            emitLiveSse('agent_job_started', { status: 'running', sequence: 1, at: '2026-10-08T10:20:01.100Z' })
            emitLiveSse('agent_job_progress', {
                status: 'running',
                message: 'phase one complete',
                sequence: 3,
                at: '2026-10-08T10:20:01.300Z',
            })
            await flushPromises()

            await clickLogsToggle(wrapper, 'logs-alpha')

            expect(api.getAgentJobLogs).toHaveBeenCalledWith('job-logs-a')
            expect(panelText(wrapper)).toContain('phase one complete')
            expect(panelText(wrapper)).toContain('runner ready')
            expect(cardFor(wrapper, 'logs-alpha').find('.status-chip').text()).toBe('running')
        } finally {
            wrapper.unmount()
        }
    })

    it('merges a snapshot with REST-only init/result entries interleaved with the live tail without duplicates', async () => {
        // agent_job_init and agent_job_result are persisted but never
        // broadcast, so the live tail is non-contiguous with the snapshot.
        // A watched job completing (result between last progress and
        // completed) must reopen without duplicated rows.
        listJobs(logJobA)
        ;(api.getAgentJobLogs as any).mockResolvedValue({
            job: { ...logJobA, status: 'completed', finished_at: '2026-10-08T10:20:02.000Z' },
            events: [
                logEntry('agent_job_started', 1, '2026-10-08T10:20:01.100Z'),
                logEntry('agent_job_init', 2, '2026-10-08T10:20:01.150Z', 'runner ready'),
                logEntry('agent_job_progress', 3, '2026-10-08T10:20:01.200Z', 'phase one chunk'),
                logEntry('agent_job_result', 4, '2026-10-08T10:20:01.900Z', 'result summary'),
                logEntry('agent_job_completed', 5, '2026-10-08T10:20:02.000Z', 'done summary'),
            ],
        } satisfies AgentJobLogsResponse)
        const wrapper = await mountAgentJobs()
        try {
            emitLiveSse('agent_job_started', { status: 'running', sequence: 1, at: '2026-10-08T10:20:01.100Z' })
            emitLiveSse('agent_job_progress', {
                status: 'running',
                message: 'phase one chunk',
                sequence: 3,
                at: '2026-10-08T10:20:01.200Z',
            })
            emitLiveSse('agent_job_completed', {
                status: 'completed',
                finished_at: '2026-10-08T10:20:02.000Z',
                message: 'done summary',
                sequence: 5,
                at: '2026-10-08T10:20:02.000Z',
            })
            await flushPromises()

            await clickLogsToggle(wrapper, 'logs-alpha')

            const text = panelText(wrapper)
            expect(occurrences(text, 'Job started')).toBe(1)
            expect(occurrences(text, 'runner ready')).toBe(1)
            expect(occurrences(text, 'phase one chunk')).toBe(1)
            expect(occurrences(text, 'result summary')).toBe(1)
            expect(occurrences(text, 'done summary')).toBe(1)
        } finally {
            wrapper.unmount()
        }
    })

    it('keeps a genuinely repeated identical chunk with a new sequence instead of matching the older instance', async () => {
        // Identical repeated deltas are distinct events: the snapshot holds
        // an older identical chunk, a new one arrives during the fetch
        // flight, and both must survive (content joins matched the older
        // instance and dropped the new chunk).
        listJobs(logJobA)
        const logs = deferred<AgentJobLogsResponse>()
        ;(api.getAgentJobLogs as any).mockReturnValue(logs.promise)
        const wrapper = await mountAgentJobs()
        try {
            emitLiveSse('agent_job_started', { status: 'running', sequence: 1, at: '2026-10-08T10:20:01.100Z' })
            await clickLogsToggle(wrapper, 'logs-alpha')
            expect(panelText(wrapper)).toContain('Loading logs')

            emitLiveSse('agent_job_progress', {
                status: 'running',
                message: 'repeated chunk',
                sequence: 7,
                at: '2026-10-08T10:20:03.000Z',
            })
            await flushPromises()

            logs.resolve({
                job: logJobA,
                events: [
                    logEntry('agent_job_started', 1, '2026-10-08T10:20:01.100Z'),
                    logEntry('agent_job_init', 2, '2026-10-08T10:20:01.150Z', 'runner ready'),
                    logEntry('agent_job_progress', 5, '2026-10-08T10:20:02.000Z', 'repeated chunk'),
                ],
            })
            await flushPromises()

            const text = panelText(wrapper)
            expect(occurrences(text, 'repeated chunk')).toBe(2)
            expect(occurrences(text, 'runner ready')).toBe(1)
            expect(occurrences(text, 'Job started')).toBe(1)
        } finally {
            wrapper.unmount()
        }
    })

    it('ignores delayed SSE events the snapshot already covered and appends only newer sequences', async () => {
        listJobs(logJobA)
        const logs = deferred<AgentJobLogsResponse>()
        ;(api.getAgentJobLogs as any).mockReturnValue(logs.promise)
        const wrapper = await mountAgentJobs()
        try {
            emitLiveSse('agent_job_started', { status: 'running', sequence: 1, at: '2026-10-08T10:20:01.100Z' })
            await clickLogsToggle(wrapper, 'logs-alpha')

            logs.resolve({
                job: { ...logJobA, status: 'completed', finished_at: '2026-10-08T10:20:01.400Z' },
                events: [
                    logEntry('agent_job_started', 1, '2026-10-08T10:20:01.100Z'),
                    logEntry('agent_job_init', 2, '2026-10-08T10:20:01.150Z', 'runner ready'),
                    logEntry('agent_job_progress', 3, '2026-10-08T10:20:01.200Z', 'old chunk'),
                    logEntry('agent_job_completed', 4, '2026-10-08T10:20:01.400Z', 'done summary'),
                ],
            })
            await flushPromises()

            // The debounced broadcast of an event the snapshot already
            // covered arrives late; it must not append a duplicate.
            emitLiveSse('agent_job_progress', {
                status: 'running',
                message: 'old chunk',
                sequence: 3,
                at: '2026-10-08T10:20:01.200Z',
            })
            await flushPromises()
            expect(occurrences(panelText(wrapper), 'old chunk')).toBe(1)
            expect(occurrences(panelText(wrapper), 'done summary')).toBe(1)

            // A genuinely newer event still appends exactly once.
            emitLiveSse('agent_job_progress', {
                status: 'running',
                message: 'fresh chunk',
                sequence: 6,
                at: '2026-10-08T10:20:04.000Z',
            })
            await flushPromises()
            expect(occurrences(panelText(wrapper), 'fresh chunk')).toBe(1)
            expect(occurrences(panelText(wrapper), 'runner ready')).toBe(1)
            expect(occurrences(panelText(wrapper), 'Job started')).toBe(1)
        } finally {
            wrapper.unmount()
        }
    })

    it('orders the merged log by numeric sequence, not timestamps', async () => {
        listJobs(logJobA)
        ;(api.getAgentJobLogs as any).mockResolvedValue({
            job: logJobA,
            events: [
                logEntry('agent_job_message', 30, '2026-10-08T09:00:00.000Z', 'ord-thirty'),
                logEntry('agent_job_started', 10, '2026-10-08T12:00:00.000Z'),
            ],
        } satisfies AgentJobLogsResponse)
        const wrapper = await mountAgentJobs()
        try {
            emitLiveSse('agent_job_message', {
                status: 'running',
                message: 'ord-twenty',
                sequence: 20,
                at: '2026-10-08T11:00:00.000Z',
            })
            await flushPromises()

            await clickLogsToggle(wrapper, 'logs-alpha')

            const rows = cardFor(wrapper, 'logs-alpha').findAll('.log-row')
            expect(rows.map((row) => row.text())).toEqual([
                expect.stringContaining('Job started'),
                expect.stringContaining('ord-twenty'),
                expect.stringContaining('ord-thirty'),
            ])
        } finally {
            wrapper.unmount()
        }
    })

    it('joins the 200-event snapshot suffix with the 1000-entry live buffer without duplicates or resurrection', async () => {
        listJobs(logJobA)
        const logs = deferred<AgentJobLogsResponse>()
        ;(api.getAgentJobLogs as any).mockReturnValue(logs.promise)
        const wrapper = await mountAgentJobs()
        try {
            // Fill the live buffer past its retention cap: sequences 1..1005
            // arrive, so the head (1..5) is trimmed and only 6..1005 remain.
            for (let sequence = 1; sequence <= 1005; sequence++) {
                emitLiveSse('agent_job_progress', {
                    status: 'running',
                    message: `bulk-${sequence}`,
                    sequence,
                    at: '2026-10-08T10:20:00.000Z',
                })
            }
            await flushPromises()

            // A stale delayed event below the live watermark must not be
            // recorded; if it were, it would union into the merged rows.
            emitLiveSse('agent_job_message', {
                status: 'running',
                message: 'stale resurrect',
                sequence: 2,
                at: '2026-10-08T10:20:00.000Z',
            })
            await flushPromises()

            // The server snapshot is capped to the newest 200 events.
            const snapshot = Array.from({ length: 200 }, (_, i) => {
                const sequence = 806 + i
                return logEntry('agent_job_progress', sequence, '2026-10-08T10:20:00.000Z', `bulk-${sequence}`)
            })
            await clickLogsToggle(wrapper, 'logs-alpha')
            logs.resolve({ job: logJobA, events: snapshot })
            await flushPromises()

            const text = panelText(wrapper)
            // Every overlapped sequence appears exactly once.
            expect(occurrences(text, 'bulk-900')).toBe(1)
            expect(text).toContain('bulk-1005')
            // Retention boundary: the oldest retained live entry survives,
            // the trimmed head and the stale resurrection do not come back.
            expect(text).toContain('bulk-6')
            expect(text).not.toContain('bulk-5bulk-6')
            expect(text).not.toContain('stale resurrect')

            // After the merge, snapshot-covered sequences stay ignored and
            // only sequences beyond the watermark append.
            emitLiveSse('agent_job_progress', {
                status: 'running',
                message: 'stale-late',
                sequence: 900,
                at: '2026-10-08T10:20:00.000Z',
            })
            emitLiveSse('agent_job_progress', {
                status: 'running',
                message: 'tail chunk',
                sequence: 1006,
                at: '2026-10-08T10:21:00.000Z',
            })
            await flushPromises()
            expect(panelText(wrapper)).not.toContain('stale-late')
            expect(panelText(wrapper)).toContain('tail chunk')
        } finally {
            wrapper.unmount()
        }
    })

    it('accepts queued SSE entries older than the snapshot window instead of dropping them', async () => {
        // The API snapshot only captures the newest 200 events; broadcasts
        // still queued in the SSE debounce can be older than that window
        // and were never seen by this client. They must be rendered in order
        // immediately, not rejected for being
        // below the snapshot's newest sequence.
        listJobs(logJobA)
        const first = deferred<AgentJobLogsResponse>()
        const second = deferred<AgentJobLogsResponse>()
        ;(api.getAgentJobLogs as any).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
        const wrapper = await mountAgentJobs()
        try {
            // Delivered live before the fetch: 200..249.
            for (let sequence = 200; sequence <= 249; sequence++) {
                emitLiveSse('agent_job_progress', {
                    status: 'running',
                    message: `gap-${sequence}`,
                    sequence,
                    at: '2026-10-08T10:20:00.000Z',
                })
            }
            await clickLogsToggle(wrapper, 'logs-alpha')

            // The snapshot window only covers the newest 200 events.
            const windowed = (kind: string, message: string) =>
                Array.from({ length: 200 }, (_, i) =>
                    logEntry(kind, 301 + i, '2026-10-08T10:20:01.000Z', `${message}-${301 + i}`))
            first.resolve({ job: logJobA, events: windowed('agent_job_progress', 'snap') })
            await flushPromises()
            expect(panelText(wrapper)).toContain('snap-450')
            expect(panelText(wrapper)).toContain('gap-249')

            // The debounce now delivers 250..300, older than the snapshot
            // window and never seen: render the backfill immediately, and the
            // list (which already ends at snap-500) stays sequence-ordered.
            for (let sequence = 250; sequence <= 300; sequence++) {
                emitLiveSse('agent_job_progress', {
                    status: 'running',
                    message: `gap-${sequence}`,
                    sequence,
                    at: '2026-10-08T10:20:00.500Z',
                })
            }
            await flushPromises()
            expect(panelText(wrapper)).toContain('gap-250')
            expect(panelText(wrapper)).toContain('gap-300')
            expect(panelText(wrapper).indexOf('gap-250')).toBeLessThan(panelText(wrapper).indexOf('snap-450'))

            // The next merge replays the buffered backfill in order.
            await clickLogsToggle(wrapper, 'logs-alpha')
            await clickLogsToggle(wrapper, 'logs-alpha')
            second.resolve({ job: logJobA, events: windowed('agent_job_progress', 'snap') })
            await flushPromises()
            const text = panelText(wrapper)
            expect(text).toContain('gap-250')
            expect(text).toContain('gap-300')
            expect(text.indexOf('gap-250')).toBeLessThan(text.indexOf('snap-450'))
            expect(occurrences(text, 'snap-450')).toBe(1)
        } finally {
            wrapper.unmount()
        }
    })

    it('does not stack pending log requests for repeated toggles and refetches on a later open', async () => {
        listJobs(logJobA)
        const first = deferred<AgentJobLogsResponse>()
        const second = deferred<AgentJobLogsResponse>()
        ;(api.getAgentJobLogs as any).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
        const wrapper = await mountAgentJobs()
        try {
            emitLiveSse('agent_job_started', { status: 'running', sequence: 1, at: '2026-10-08T10:20:01.100Z' })
            await clickLogsToggle(wrapper, 'logs-alpha')
            expect(api.getAgentJobLogs).toHaveBeenCalledTimes(1)

            // Close and reopen while the first request is still pending: the
            // in-flight snapshot must satisfy the reopen, not a second request.
            await clickLogsToggle(wrapper, 'logs-alpha')
            await clickLogsToggle(wrapper, 'logs-alpha')
            expect(api.getAgentJobLogs).toHaveBeenCalledTimes(1)

            first.resolve({
                job: logJobA,
                events: [logEntry('agent_job_started', 1, '2026-10-08T10:20:01.100Z')],
            })
            await flushPromises()
            expect(panelText(wrapper)).toContain('Job started')

            // A later open fetches a fresh snapshot again.
            await clickLogsToggle(wrapper, 'logs-alpha')
            await clickLogsToggle(wrapper, 'logs-alpha')
            expect(api.getAgentJobLogs).toHaveBeenCalledTimes(2)
            second.resolve({
                job: logJobA,
                events: [
                    logEntry('agent_job_started', 1, '2026-10-08T10:20:01.100Z'),
                    logEntry('agent_job_progress', 9, '2026-10-08T10:20:05.000Z', 'later chunk'),
                ],
            })
            await flushPromises()
            expect(panelText(wrapper)).toContain('later chunk')
        } finally {
            wrapper.unmount()
        }
    })

    it('fetches log snapshots per job independently', async () => {
        listJobs(logJobA, logJobB)
        const alpha = deferred<AgentJobLogsResponse>()
        const beta = deferred<AgentJobLogsResponse>()
        ;(api.getAgentJobLogs as any).mockImplementation((id: string) =>
            id === 'job-logs-a' ? alpha.promise : beta.promise)
        const wrapper = await mountAgentJobs()
        try {
            emitSse('agent_job_started', {
                id: 'job-logs-a',
                ticket_id: 'DEV-1',
                status: 'running',
                sequence: 1,
                at: '2026-10-08T10:20:01.100Z',
            })
            emitSse('agent_job_started', {
                id: 'job-logs-b',
                ticket_id: 'DEV-1',
                status: 'running',
                sequence: 1,
                at: '2026-10-08T10:20:02.100Z',
            })
            await clickLogsToggle(wrapper, 'logs-alpha')
            await clickLogsToggle(wrapper, 'logs-beta')
            expect(api.getAgentJobLogs).toHaveBeenCalledWith('job-logs-a')
            expect(api.getAgentJobLogs).toHaveBeenCalledWith('job-logs-b')
            expect(api.getAgentJobLogs).toHaveBeenCalledTimes(2)

            // Beta's snapshot resolves first; alpha stays pending without
            // blocking its sibling.
            beta.resolve({
                job: logJobB,
                events: [logEntry('agent_job_progress', 4, '2026-10-08T10:20:03.000Z', 'beta history')],
            })
            await flushPromises()
            expect(panelText(wrapper, 'logs-beta')).toContain('beta history')
            expect(panelText(wrapper, 'logs-alpha')).toContain('Loading logs')

            alpha.resolve({
                job: logJobA,
                events: [logEntry('agent_job_progress', 7, '2026-10-08T10:20:04.000Z', 'alpha history')],
            })
            await flushPromises()
            expect(panelText(wrapper, 'logs-alpha')).toContain('alpha history')
            expect(panelText(wrapper, 'logs-beta')).toContain('beta history')
        } finally {
            wrapper.unmount()
        }
    })
})
