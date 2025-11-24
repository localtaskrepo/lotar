import type { SprintListItem } from '../api/types'
import { parseTaskDate, startOfLocalDay, toDateKey } from './date'

export interface SprintCalendarDayEntry {
    id: number
    label: string
    state: SprintListItem['state']
    isStart: boolean
    isEnd: boolean
    startDate: Date // planned window start
    endDate: Date // planned window end
    actualStartDate: Date | null
    actualEndDate: Date | null
    isActualStart: boolean
    isActualEnd: boolean
    beforeActualStart: boolean
    afterActualEnd: boolean
}

interface SprintWindow {
    plannedStart: Date
    plannedEnd: Date
    actualStart: Date | null
    actualEnd: Date | null
}

const DAY_MS = 24 * 60 * 60 * 1000

function parseDate(value?: string | null): Date | null {
    const parsed = value ? parseTaskDate(value) : null
    return parsed ? startOfLocalDay(parsed) : null
}

function parsePlanLengthDays(value?: string | null): number | null {
    if (!value) return null
    const trimmed = value.trim()
    if (!trimmed) return null
    const normalized = trimmed.toLowerCase()
    const suffix = normalized[normalized.length - 1]
    if (suffix === 'd' || suffix === 'w') {
        const num = normalized.slice(0, -1).trim()
        const parsed = Number(num)
        if (!Number.isFinite(parsed) || parsed <= 0) return null
        return suffix === 'd' ? parsed : parsed * 7
    }
    const parts = normalized.split(/\s+/)
    if (parts.length === 2) {
        const numeric = Number(parts[0])
        if (!Number.isFinite(numeric) || numeric <= 0) return null
        const unit = parts[1]
        if (unit.startsWith('day')) return numeric
        if (unit.startsWith('week')) return numeric * 7
    }
    return null
}

function normalizeSprintWindow(sprint: SprintListItem): SprintWindow | null {
    const plannedStart = parseDate(sprint.planned_start) || parseDate(sprint.actual_start)
    let plannedEnd =
        parseDate(sprint.planned_end) ||
        parseDate(sprint.computed_end) ||
        parseDate(sprint.overdue_after) ||
        parseDate(sprint.actual_end) ||
        null

    if (!plannedStart) {
        return null
    }

    if ((!plannedEnd || plannedEnd.getTime() < plannedStart.getTime()) && sprint.plan_length) {
        const days = parsePlanLengthDays(sprint.plan_length)
        if (days) {
            const durationDays = Math.max(1, Math.floor(days))
            const offsetDays = durationDays - 1
            plannedEnd = new Date(plannedStart.getTime() + offsetDays * DAY_MS)
        }
    }

    if (!plannedEnd || plannedEnd.getTime() < plannedStart.getTime()) {
        plannedEnd = new Date(plannedStart)
    }

    const actualStart = parseDate(sprint.actual_start)
    const actualEnd = parseDate(sprint.actual_end)

    return { plannedStart, plannedEnd, actualStart, actualEnd }
}

function labelForSprint(sprint: SprintListItem): string {
    return sprint.display_name || sprint.label || `Sprint ${sprint.id}`
}

function addDay(date: Date): Date {
    return new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1)
}

export function buildSprintSchedule(
    sprints: SprintListItem[] | undefined,
    rangeStart: Date,
    rangeEnd: Date,
): Record<string, SprintCalendarDayEntry[]> {
    const normalizedStart = startOfLocalDay(rangeStart)
    const normalizedEnd = startOfLocalDay(rangeEnd)
    const schedule: Record<string, SprintCalendarDayEntry[]> = {}

    if (!sprints?.length) {
        return schedule
    }

    for (const sprint of sprints) {
        const window = normalizeSprintWindow(sprint)
        if (!window) {
            continue
        }
        const startEpoch = window.plannedStart.getTime()
        const endEpoch = window.plannedEnd.getTime()
        const actualStartEpoch = window.actualStart?.getTime() ?? null
        const actualEndEpoch = window.actualEnd?.getTime() ?? null
        const clampStart = new Date(Math.max(startEpoch, normalizedStart.getTime()))
        const clampEnd = new Date(Math.min(endEpoch, normalizedEnd.getTime()))

        if (clampEnd.getTime() < normalizedStart.getTime() || clampStart.getTime() > normalizedEnd.getTime()) {
            continue
        }

        for (let cursor = new Date(clampStart); cursor.getTime() <= clampEnd.getTime(); cursor = addDay(cursor)) {
            const key = toDateKey(cursor)
            const cursorEpoch = cursor.getTime()
            const entry: SprintCalendarDayEntry = {
                id: sprint.id,
                label: labelForSprint(sprint),
                state: sprint.state,
                isStart: toDateKey(window.plannedStart) === key,
                isEnd: toDateKey(window.plannedEnd) === key,
                startDate: window.plannedStart,
                endDate: window.plannedEnd,
                actualStartDate: window.actualStart,
                actualEndDate: window.actualEnd,
                isActualStart: actualStartEpoch !== null && cursorEpoch === actualStartEpoch,
                isActualEnd: actualEndEpoch !== null && cursorEpoch === actualEndEpoch,
                beforeActualStart: actualStartEpoch !== null && cursorEpoch < actualStartEpoch,
                afterActualEnd: actualEndEpoch !== null && cursorEpoch > actualEndEpoch,
            }
                ; (schedule[key] ||= []).push(entry)
        }
    }

    for (const key of Object.keys(schedule)) {
        schedule[key].sort((a, b) => {
            const startDiff = a.startDate.getTime() - b.startDate.getTime()
            if (startDiff !== 0) {
                return startDiff
            }
            return a.label.localeCompare(b.label)
        })
    }

    return schedule
}

export function __test_only_normalizeSprintWindow(sprint: SprintListItem): SprintWindow | null {
    return normalizeSprintWindow(sprint)
}

export const __test_only_parsePlanLengthDays = parsePlanLengthDays
