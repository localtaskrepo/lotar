import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentJob, AgentJobCancelAllResponse } from '../api/types'
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
