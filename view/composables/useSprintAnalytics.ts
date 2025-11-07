import { reactive } from 'vue'
import { api } from '../api/client'
import type {
    SprintBurndownResponse,
    SprintSummaryReportResponse,
    SprintVelocityResponse,
} from '../api/types'

type SprintMetric = 'tasks' | 'points' | 'hours'

type VelocityParams = {
    limit?: number
    include_active?: boolean
    metric?: SprintMetric
}

type SprintFetchOptions = {
    force?: boolean
}

type VelocityFetchOptions = {
    force?: boolean
}

export const DEFAULT_VELOCITY_PARAMS: Required<VelocityParams> = {
    limit: 8,
    include_active: true,
    metric: 'points',
}

function normalizeVelocityParams(params: VelocityParams = {}): Required<VelocityParams> {
    return {
        limit: typeof params.limit === 'number' && params.limit > 0 ? Math.floor(params.limit) : DEFAULT_VELOCITY_PARAMS.limit,
        include_active: typeof params.include_active === 'boolean' ? params.include_active : DEFAULT_VELOCITY_PARAMS.include_active,
        metric: (params.metric ?? DEFAULT_VELOCITY_PARAMS.metric) satisfies SprintMetric,
    }
}

function velocityKey(params: Required<VelocityParams>): string {
    return `${params.metric}:${params.limit}:${params.include_active ? '1' : '0'}`
}

export function useSprintAnalytics() {
    const summaryCache = reactive<Record<number, SprintSummaryReportResponse | undefined>>({})
    const summaryLoading = reactive<Record<number, boolean>>({})
    const summaryError = reactive<Record<number, string | null | undefined>>({})
    const summaryPending = new Map<number, Promise<SprintSummaryReportResponse>>()

    const burndownCache = reactive<Record<number, SprintBurndownResponse | undefined>>({})
    const burndownLoading = reactive<Record<number, boolean>>({})
    const burndownError = reactive<Record<number, string | null | undefined>>({})
    const burndownPending = new Map<number, Promise<SprintBurndownResponse>>()

    const velocityCache = reactive<Record<string, SprintVelocityResponse | undefined>>({})
    const velocityLoading = reactive<Record<string, boolean>>({})
    const velocityError = reactive<Record<string, string | null | undefined>>({})
    const velocityPending = new Map<string, Promise<SprintVelocityResponse>>()

    async function fetchSummary(sprintId: number, options: SprintFetchOptions = {}): Promise<SprintSummaryReportResponse> {
        if (sprintId <= 0) {
            throw new Error('sprintId must be greater than zero')
        }
        if (!options.force) {
            const cached = summaryCache[sprintId]
            if (cached) return cached
            const pending = summaryPending.get(sprintId)
            if (pending) return pending
        }
        const request = (async () => {
            summaryLoading[sprintId] = true
            summaryError[sprintId] = null
            try {
                const response = await api.sprintSummary(sprintId)
                summaryCache[sprintId] = response
                return response
            } catch (error: any) {
                const message = error?.message || 'Failed to load sprint summary'
                summaryError[sprintId] = message
                throw error
            } finally {
                summaryLoading[sprintId] = false
                summaryPending.delete(sprintId)
            }
        })()
        summaryPending.set(sprintId, request)
        return request
    }

    async function fetchBurndown(sprintId: number, options: SprintFetchOptions = {}): Promise<SprintBurndownResponse> {
        if (sprintId <= 0) {
            throw new Error('sprintId must be greater than zero')
        }
        if (!options.force) {
            const cached = burndownCache[sprintId]
            if (cached) return cached
            const pending = burndownPending.get(sprintId)
            if (pending) return pending
        }
        const request = (async () => {
            burndownLoading[sprintId] = true
            burndownError[sprintId] = null
            try {
                const response = await api.sprintBurndown(sprintId)
                burndownCache[sprintId] = response
                return response
            } catch (error: any) {
                const message = error?.message || 'Failed to load sprint burndown'
                burndownError[sprintId] = message
                throw error
            } finally {
                burndownLoading[sprintId] = false
                burndownPending.delete(sprintId)
            }
        })()
        burndownPending.set(sprintId, request)
        return request
    }

    async function fetchSprintAnalytics(sprintId: number, options: SprintFetchOptions = {}): Promise<void> {
        await Promise.all([fetchSummary(sprintId, options), fetchBurndown(sprintId, options)])
    }

    function getSummary(sprintId: number | null | undefined): SprintSummaryReportResponse | undefined {
        if (!sprintId) return undefined
        return summaryCache[sprintId]
    }

    function getBurndown(sprintId: number | null | undefined): SprintBurndownResponse | undefined {
        if (!sprintId) return undefined
        return burndownCache[sprintId]
    }

    function isSummaryLoading(sprintId: number | null | undefined): boolean {
        if (!sprintId) return false
        return Boolean(summaryLoading[sprintId])
    }

    function isBurndownLoading(sprintId: number | null | undefined): boolean {
        if (!sprintId) return false
        return Boolean(burndownLoading[sprintId])
    }

    function getSummaryError(sprintId: number | null | undefined): string | null {
        if (!sprintId) return null
        return summaryError[sprintId] ?? null
    }

    function getBurndownError(sprintId: number | null | undefined): string | null {
        if (!sprintId) return null
        return burndownError[sprintId] ?? null
    }

    async function loadVelocity(params: VelocityParams = {}, options: VelocityFetchOptions = {}): Promise<SprintVelocityResponse> {
        const normalized = normalizeVelocityParams(params)
        const key = velocityKey(normalized)
        if (!options.force) {
            const cached = velocityCache[key]
            if (cached) return cached
            const pending = velocityPending.get(key)
            if (pending) return pending
        }
        const request = (async () => {
            velocityLoading[key] = true
            velocityError[key] = null
            try {
                const response = await api.sprintVelocity(normalized)
                velocityCache[key] = response
                return response
            } catch (error: any) {
                const message = error?.message || 'Failed to load sprint velocity'
                velocityError[key] = message
                throw error
            } finally {
                velocityLoading[key] = false
                velocityPending.delete(key)
            }
        })()
        velocityPending.set(key, request)
        return request
    }

    function getVelocity(params: VelocityParams = {}): SprintVelocityResponse | undefined {
        const normalized = normalizeVelocityParams(params)
        return velocityCache[velocityKey(normalized)]
    }

    function isVelocityLoading(params: VelocityParams = {}): boolean {
        const normalized = normalizeVelocityParams(params)
        return Boolean(velocityLoading[velocityKey(normalized)])
    }

    function getVelocityError(params: VelocityParams = {}): string | null {
        const normalized = normalizeVelocityParams(params)
        return velocityError[velocityKey(normalized)] ?? null
    }

    return {
        DEFAULT_VELOCITY_PARAMS,
        fetchSummary,
        fetchBurndown,
        fetchSprintAnalytics,
        getSummary,
        getBurndown,
        isSummaryLoading,
        isBurndownLoading,
        getSummaryError,
        getBurndownError,
        loadVelocity,
        getVelocity,
        isVelocityLoading,
        getVelocityError,
    }
}
