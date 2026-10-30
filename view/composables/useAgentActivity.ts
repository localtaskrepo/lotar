import { computed, getCurrentScope, onScopeDispose, ref, type ComputedRef } from 'vue'
import { api } from '../api/client'
import type { AgentJob } from '../api/types'
import { useSse } from './useSse'

/** A job that is running or waiting to run for a task (DEV-249). */
export interface AgentActivity {
    jobId: string
    taskId: string
    status: 'running' | 'queued'
    agent: string
    /** Latest progress text the agent reported, if any. */
    message: string | null
    startedAt: string | null
}

const ACTIVE = new Set(['running', 'queued', 'pending', 'starting'])
const DONE_EVENTS = new Set(['agent_job_completed', 'agent_job_failed', 'agent_job_cancelled'])
const EVENTS = [
    'agent_job_started',
    'agent_job_init',
    'agent_job_progress',
    'agent_job_message',
    'agent_job_completed',
    'agent_job_failed',
    'agent_job_cancelled',
]

const jobs = ref(new Map<string, AgentActivity>())
let users = 0
let stream: ReturnType<typeof useSse> | null = null
let unsubscribers: Array<() => void> = []

function fromJob(job: AgentJob): AgentActivity | null {
    if (!job?.id || !job.ticket_id || !ACTIVE.has(job.status)) return null
    return {
        jobId: job.id,
        taskId: job.ticket_id,
        status: job.status === 'running' || job.status === 'starting' ? 'running' : 'queued',
        agent: job.agent || job.runner || 'Agent',
        message: job.last_message?.trim() || null,
        startedAt: job.started_at ?? null,
    }
}

async function reload() {
    try {
        const response = await api.listAgentJobs()
        const next = new Map<string, AgentActivity>()
        for (const job of response.jobs ?? []) {
            const activity = fromJob(job)
            if (activity) next.set(activity.jobId, activity)
        }
        jobs.value = next
    } catch {
        // Advisory only: the indicator stays as it was.
    }
}

/** Apply one live job event; exported for tests. */
interface AgentJobEventPayload {
    id?: string
    ticket_id?: string
    agent?: string | null
    runner?: string
    message?: string | null
    started_at?: string | null
}

export function applyAgentJobEvent(kind: string, payload: AgentJobEventPayload | null | undefined) {
    if (!payload?.id) return
    const next = new Map(jobs.value)
    if (DONE_EVENTS.has(kind)) {
        next.delete(payload.id)
    } else {
        const previous = next.get(payload.id)
        const message =
            (kind === 'agent_job_progress' || kind === 'agent_job_message') && typeof payload.message === 'string'
                ? payload.message.trim() || null
                : (previous?.message ?? null)
        const jobId = payload.id
        const taskId = payload.ticket_id || previous?.taskId
        if (!taskId) return
        next.set(jobId, {
            jobId,
            taskId,
            status: 'running',
            agent: payload.agent || payload.runner || previous?.agent || 'Agent',
            message,
            startedAt: payload.started_at || previous?.startedAt || null,
        })
    }
    jobs.value = next
}

function connect() {
    void reload()
    try {
        stream = useSse('/api/events', { kinds: EVENTS.join(',') })
    } catch {
        // No event stream (tests, very old browsers): the list load still shows jobs.
        stream = null
        return
    }
    for (const kind of EVENTS) {
        const handler = (event: MessageEvent) => {
            try {
                applyAgentJobEvent(kind, JSON.parse(event.data || '{}'))
            } catch {
                // Ignore malformed payloads.
            }
        }
        stream.on(kind, handler)
        unsubscribers.push(() => stream?.off?.(kind, handler))
    }
    // Events missed while disconnected: reconcile from the job list.
    const onOpen = () => void reload()
    stream.on('open', onOpen)
    unsubscribers.push(() => stream?.off?.('open', onOpen))
}

function disconnect() {
    unsubscribers.forEach((off) => off())
    unsubscribers = []
    stream?.close?.()
    stream = null
}

/**
 * Which tasks an agent is working on right now, kept live from the job list
 * and agent job events. One connection is shared by every user.
 */
export function useAgentActivity() {
    users += 1
    if (users === 1) connect()
    if (getCurrentScope()) {
        onScopeDispose(() => {
            users -= 1
            if (users === 0) {
                disconnect()
                jobs.value = new Map()
            }
        })
    }
    const byTask = computed(() => {
        const map = new Map<string, AgentActivity[]>()
        for (const activity of jobs.value.values()) {
            const list = map.get(activity.taskId) ?? []
            list.push(activity)
            map.set(activity.taskId, list)
        }
        return map
    })
    function activityFor(taskId: () => string | null | undefined): ComputedRef<AgentActivity[]> {
        return computed(() => {
            const id = taskId()
            return id ? (byTask.value.get(id) ?? []) : []
        })
    }
    return { byTask, activityFor, reload }
}

/** One line for a tooltip: who is doing what. */
export function describeAgentActivity(activities: AgentActivity[]): string {
    return activities
        .map((activity) =>
            activity.status === 'queued'
                ? `${activity.agent} is queued to work on this task`
                : activity.message
                  ? `${activity.agent} is working on this: ${activity.message}`
                  : `${activity.agent} is working on this task`,
        )
        .join('\n')
}
