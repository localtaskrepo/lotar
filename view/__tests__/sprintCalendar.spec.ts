import { describe, expect, it } from 'vitest'
import type { SprintListItem } from '../api/types'
import { toDateKey } from '../utils/date'
import { __test_only_normalizeSprintWindow, __test_only_parsePlanLengthDays, buildSprintSchedule } from '../utils/sprintCalendar'

const baseSprint = (overrides: Partial<SprintListItem>): SprintListItem => ({
    id: 1,
    display_name: 'Sprint 1',
    state: 'active',
    warnings: [],
    ...overrides,
} as SprintListItem)

describe('sprint calendar helpers', () => {
    it('normalizes start/end with fallbacks', () => {
        const sprint = baseSprint({
            id: 10,
            state: 'pending',
            planned_start: '2024-01-02T00:00:00Z',
            computed_end: '2024-01-10T00:00:00Z',
        })
        const window = __test_only_normalizeSprintWindow(sprint)
        expect(window).toBeTruthy()
        expect(toDateKey(window!.plannedStart)).toBe(toDateKey(new Date('2024-01-02T00:00:00Z')))
        expect(toDateKey(window!.plannedEnd)).toBe(toDateKey(new Date('2024-01-10T00:00:00Z')))
    })

    it('builds schedule across overlapping sprints', () => {
        const rangeStart = new Date('2024-02-01T00:00:00Z')
        const rangeEnd = new Date('2024-02-10T00:00:00Z')
        const sprints: SprintListItem[] = [
            baseSprint({
                id: 101,
                display_name: 'Alpha',
                state: 'active',
                planned_start: '2024-02-01T00:00:00Z',
                planned_end: '2024-02-05T00:00:00Z',
            }),
            baseSprint({
                id: 202,
                display_name: 'Beta',
                state: 'overdue',
                planned_start: '2024-02-04T00:00:00Z',
                planned_end: '2024-02-08T00:00:00Z',
            }),
        ]

        const schedule = buildSprintSchedule(sprints, rangeStart, rangeEnd)
        const feb4 = schedule[toDateKey(new Date('2024-02-04T00:00:00Z'))]
        expect(feb4).toHaveLength(2)
        const alpha = feb4?.find((entry) => entry.id === 101)
        const beta = feb4?.find((entry) => entry.id === 202)
        expect(alpha?.isStart).toBe(false)
        expect(alpha?.isEnd).toBe(false)
        expect(beta?.isStart).toBe(true)
        expect(beta?.state).toBe('overdue')
    })

    it('clips schedule to visible range', () => {
        const rangeStart = new Date('2024-03-10T00:00:00Z')
        const rangeEnd = new Date('2024-03-12T00:00:00Z')
        const sprints: SprintListItem[] = [
            baseSprint({
                id: 303,
                display_name: 'Gamma',
                state: 'active',
                planned_start: '2024-03-01T00:00:00Z',
                planned_end: '2024-03-20T00:00:00Z',
            }),
        ]
        const schedule = buildSprintSchedule(sprints, rangeStart, rangeEnd)
        expect(Object.keys(schedule)).toHaveLength(3)
        const firstDay = schedule[toDateKey(new Date('2024-03-10T00:00:00Z'))]?.[0]
        expect(firstDay?.isStart).toBe(false)
        const lastDay = schedule[toDateKey(new Date('2024-03-12T00:00:00Z'))]?.[0]
        expect(lastDay?.isEnd).toBe(false)
    })

    it('falls back to plan_length when explicit end is missing', () => {
        const sprint = baseSprint({
            id: 404,
            display_name: 'Delta',
            state: 'pending',
            planned_start: '2024-05-01',
            plan_length: '14d',
            planned_end: null,
            computed_end: null,
        })
        const window = __test_only_normalizeSprintWindow(sprint)
        expect(window).toBeTruthy()
        expect(window!.plannedEnd.getTime()).toBeGreaterThan(window!.plannedStart.getTime())
        const schedule = buildSprintSchedule([sprint], new Date('2024-05-01T00:00:00Z'), new Date('2024-05-31T00:00:00Z'))
        const entries = Object.values(schedule).flat().filter((entry) => entry.id === 404)
        expect(entries).toHaveLength(14)
        const lastDay = entries[entries.length - 1]
        expect(toDateKey(lastDay.endDate)).toBe('2024-05-14')
    })

    it('parses duration expressions used in plan_length', () => {
        expect(__test_only_parsePlanLengthDays('14d')).toBe(14)
        expect(__test_only_parsePlanLengthDays('2w')).toBe(14)
        expect(__test_only_parsePlanLengthDays('3 weeks')).toBe(21)
        expect(__test_only_parsePlanLengthDays('10 days')).toBe(10)
        expect(__test_only_parsePlanLengthDays('  0d ')).toBeNull()
    })

    it('keeps planned window even when actual dates differ and flags dimming boundaries', () => {
        const rangeStart = new Date('2024-10-20')
        const rangeEnd = new Date('2024-11-05')
        const sprint = baseSprint({
            id: 505,
            display_name: 'Epsilon',
            planned_start: '2024-10-25',
            planned_end: '2024-11-04',
            actual_start: '2024-10-27',
            actual_end: '2024-10-30',
        })
        const schedule = buildSprintSchedule([sprint], rangeStart, rangeEnd)
        const plannedStartEntry = schedule['2024-10-25'][0]
        expect(plannedStartEntry.beforeActualStart).toBe(true)
        expect(plannedStartEntry.isStart).toBe(true)
        const actualStartEntry = schedule['2024-10-27'][0]
        expect(actualStartEntry.isActualStart).toBe(true)
        expect(actualStartEntry.beforeActualStart).toBe(false)
        const actualEndEntry = schedule['2024-10-30'][0]
        expect(actualEndEntry.isActualEnd).toBe(true)
        const afterActualEndEntry = schedule['2024-11-01'][0]
        expect(afterActualEndEntry.afterActualEnd).toBe(true)
        expect(afterActualEndEntry.isEnd).toBe(false)
    })
})
