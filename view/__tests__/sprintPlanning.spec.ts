import { describe, expect, it } from 'vitest'
import { nextTick } from 'vue'
import type { TaskDTO } from '../api/types'
import { useSprintForm } from '../composables/useSprintForm'
import {
    addPlanLength,
    clampPageOffset,
    formatElapsedDuration,
    normalizeSprintMembership,
    pageOffset,
    pageSlice,
    parseDurationToMs,
    parseNumberInput,
    planLengthBetween,
    sprintMembershipMatches,
} from '../utils/sprintPlanning'

const HOUR = 3_600_000
const DAY = 24 * HOUR

describe('sprintPlanning', () => {
    it('parses durations in any supported unit and rejects nonsense', () => {
        expect(parseDurationToMs('2w')).toBe(14 * DAY)
        expect(parseDurationToMs('1d 6 hours')).toBe(DAY + 6 * HOUR)
        expect(parseDurationToMs('90 minutes')).toBe(1.5 * HOUR)
        expect(parseDurationToMs('soon')).toBeNull()
        expect(parseDurationToMs('3 fortnights')).toBeNull()
        expect(parseDurationToMs('')).toBeNull()
    })

    it('formats plan lengths and elapsed times', () => {
        const start = new Date(2026, 5, 1, 9, 0).getTime()
        expect(planLengthBetween(start, start + 15 * DAY + 2 * HOUR)).toBe('2w 1d 2h')
        expect(planLengthBetween(start, start + 10_000)).toBe('1m')
        expect(planLengthBetween(start, start)).toBe('')
        expect(formatElapsedDuration(3 * DAY + 4 * HOUR + 5 * 60_000)).toBe('3d 4h')
        expect(formatElapsedDuration(DAY + 5 * 60_000)).toBe('1d 5m')
        expect(formatElapsedDuration(30_000)).toBe('<1m')
    })

    it('normalizes sprint membership and compares it order-insensitively', () => {
        expect(normalizeSprintMembership([3, '1', 3, -2, 'x'])).toEqual([1, 3])
        expect(normalizeSprintMembership(null)).toEqual([])
        const task = (sprints: unknown) => ({ id: 'A-1', sprints }) as unknown as TaskDTO
        expect(sprintMembershipMatches(task([2, 1]), task([1, 2]))).toBe(true)
        expect(sprintMembershipMatches(null, task([]))).toBe(true)
        expect(sprintMembershipMatches(task([1]), task([1, 2]))).toBe(false)
    })

    it('reads number inputs', () => {
        expect(parseNumberInput(' 12 ')).toBe(12)
        expect(parseNumberInput('')).toBeNull()
        expect(parseNumberInput('abc')).toBeNull()
    })
})

// March 2 -> 30 crosses both the US (Mar 8) and EU (Mar 29) clock change; in a
// zone without daylight saving these still hold, there is just no change to cross.
describe('plan lengths across a daylight-saving change', () => {
    const start = new Date(2026, 2, 2, 9, 0).getTime()
    const fourWeeksLater = new Date(2026, 2, 30, 9, 0).getTime()

    it('adds weeks and days on the calendar, keeping the time of day', () => {
        expect(addPlanLength(start, '4w')).toBe(fourWeeksLater)
        expect(addPlanLength(start, '27d 6h')).toBe(new Date(2026, 2, 29, 15, 0).getTime())
        expect(addPlanLength(start, 'soon')).toBeNull()
    })

    it('describes a span of whole local days in weeks and days', () => {
        expect(planLengthBetween(start, fourWeeksLater)).toBe('4w')
        expect(planLengthBetween(start, new Date(2026, 2, 30, 8, 30).getTime())).toBe('3w 6d 23h 30m')
    })

    it('does not count an extra day on a 25-hour day', () => {
        const autumn = new Date(2026, 9, 20, 9, 0).getTime()
        const nextMonth = new Date(2026, 10, 3, 8, 59).getTime()
        expect(planLengthBetween(autumn, nextMonth)).toBe('1w 6d 23h 59m')
    })
})

describe('paging helpers', () => {
    it('clamps page offsets to the pages that exist', () => {
        expect(pageOffset(120, 3, 50)).toBe(100)
        expect(pageOffset(120, 9, 50)).toBe(100)
        expect(pageOffset(0, 2, 50)).toBe(0)
        expect(clampPageOffset(100, 60, 50)).toBe(50)
        expect(clampPageOffset(50, 0, 50)).toBe(0)
        expect(clampPageOffset(50, 120, 50)).toBe(50)
    })

    it('returns the same list when it fits on one page', () => {
        const list = [1, 2, 3]
        expect(pageSlice(list, 0, 5)).toBe(list)
        expect(pageSlice([1, 2, 3, 4, 5], 2, 2)).toEqual([3, 4])
    })
})

describe('useSprintForm', () => {
    it('ends a four-week sprint at the same time of day across a clock change', async () => {
        const { form } = useSprintForm()
        form.starts_at = '2026-03-02T09:00'
        form.plan_length = '4w'
        await nextTick()
        expect(form.ends_at).toBe('2026-03-30T09:00')
        form.ends_at = '2026-03-23T09:00'
        await nextTick()
        expect(form.plan_length).toBe('3w')
    })

    it('keeps start, end and plan length in step', async () => {
        const { form } = useSprintForm()
        form.starts_at = '2026-06-01T09:00'
        form.plan_length = '1w'
        await nextTick()
        expect(form.ends_at).toBe('2026-06-08T09:00')

        form.ends_at = '2026-06-15T09:00'
        await nextTick()
        expect(form.plan_length).toBe('2w')
    })

    it('builds create payloads from filled fields only and update payloads that clear blanks', () => {
        const { form, createPayload, updatePayload } = useSprintForm()
        form.goal = '  Ship it '
        form.capacity_points = '20'
        form.notes = '  keep  '
        expect(createPayload('S1')).toEqual({ label: 'S1', goal: 'Ship it', capacity_points: 20, notes: '  keep  ' })
        expect(updatePayload(4, 'S1')).toMatchObject({
            sprint: 4,
            label: 'S1',
            goal: 'Ship it',
            plan_length: '',
            ends_at: '',
            capacity_hours: null,
        })
    })
})
