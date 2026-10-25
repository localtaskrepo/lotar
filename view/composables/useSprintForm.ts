import { reactive, watch } from 'vue'
import type { SprintCreateRequest, SprintListItem, SprintUpdateRequest } from '../api/types'
import { fromDateTimeInputValue, toDateTimeInputValue } from '../utils/date'
import {
    addPlanLength,
    dateTimeInputToTimestamp,
    parseNumberInput,
    planLengthBetween,
    timestampToDateTimeInput,
    toStringValue,
    toTrimmedString,
} from '../utils/sprintPlanning'

export interface SprintFormState {
    label: string
    goal: string
    plan_length: string
    ends_at: string
    starts_at: string
    capacity_points: string
    capacity_hours: string
    overdue_after: string
    notes: string
    skip_defaults: boolean
}

function emptyForm(): SprintFormState {
    return {
        label: '',
        goal: '',
        plan_length: '',
        ends_at: '',
        starts_at: '',
        capacity_points: '',
        capacity_hours: '',
        overdue_after: '',
        notes: '',
        skip_defaults: false,
    }
}

/**
 * Sprint create/edit form state. Start, end and plan length stay in step:
 * changing the dates rewrites the length, and changing the length (or
 * setting a start with no end) moves the end.
 */
export function useSprintForm() {
    const form = reactive<SprintFormState>(emptyForm())

    let updatingLengthFromDates = false
    let updatingEndFromLength = false
    function setLengthFromDates(value: string) {
        updatingLengthFromDates = true
        try {
            form.plan_length = value
        } finally {
            updatingLengthFromDates = false
        }
    }
    function setEndFromLength(value: string) {
        updatingEndFromLength = true
        try {
            form.ends_at = value
        } finally {
            updatingEndFromLength = false
        }
    }
    /** The end input for `start` plus the plan length, or '' when either is missing. */
    function endFromLength(start: string, length: string): string {
        const startTs = dateTimeInputToTimestamp(start)
        const endTs = startTs === null ? null : addPlanLength(startTs, length)
        return endTs === null ? '' : timestampToDateTimeInput(endTs)
    }

    watch(
        () => [form.starts_at, form.ends_at] as const,
        ([startValue, endValue]) => {
            if (updatingEndFromLength) return
            const startTs = dateTimeInputToTimestamp(startValue)
            const endTs = dateTimeInputToTimestamp(endValue)
            if (startTs === null || endTs === null) return
            if (endTs <= startTs) {
                if (!updatingLengthFromDates && form.plan_length) setLengthFromDates('')
                return
            }
            const formatted = planLengthBetween(startTs, endTs)
            if (formatted && form.plan_length !== formatted) setLengthFromDates(formatted)
        },
        { immediate: true },
    )
    watch(
        () => form.plan_length,
        (value) => {
            if (updatingLengthFromDates) return
            const end = endFromLength(form.starts_at, value)
            if (end && form.ends_at !== end) setEndFromLength(end)
        },
    )
    watch(
        () => form.starts_at,
        (value) => {
            if (!value || form.ends_at || updatingEndFromLength) return
            const end = endFromLength(value, form.plan_length)
            if (end) setEndFromLength(end)
        },
    )

    function resetForm() {
        Object.assign(form, emptyForm())
    }

    function fillFormFromSprint(sprint: SprintListItem) {
        Object.assign(form, {
            label: sprint.label ?? sprint.display_name ?? '',
            goal: sprint.goal ?? '',
            plan_length: sprint.plan_length ?? '',
            ends_at: toDateTimeInputValue(sprint.planned_end),
            starts_at: toDateTimeInputValue(sprint.planned_start),
            capacity_points: sprint.capacity_points != null ? String(sprint.capacity_points) : '',
            capacity_hours: sprint.capacity_hours != null ? String(sprint.capacity_hours) : '',
            overdue_after: sprint.overdue_after ?? '',
            notes: sprint.notes ?? '',
            skip_defaults: false,
        } satisfies SprintFormState)
    }

    /** Create payload with only the fields that were filled in. */
    function createPayload(label: string): SprintCreateRequest {
        const payload: SprintCreateRequest = { label }
        const goal = toTrimmedString(form.goal)
        if (goal) payload.goal = goal
        const planLength = toTrimmedString(form.plan_length)
        if (planLength) payload.plan_length = planLength
        const endsAt = fromDateTimeInputValue(form.ends_at)
        if (endsAt) payload.ends_at = endsAt
        const startsAt = fromDateTimeInputValue(form.starts_at)
        if (startsAt) payload.starts_at = startsAt
        const overdueAfter = toTrimmedString(form.overdue_after)
        if (overdueAfter) payload.overdue_after = overdueAfter
        const notes = toStringValue(form.notes)
        if (notes.trim()) payload.notes = notes
        const capacityPoints = parseNumberInput(form.capacity_points)
        if (capacityPoints !== null) payload.capacity_points = capacityPoints
        const capacityHours = parseNumberInput(form.capacity_hours)
        if (capacityHours !== null) payload.capacity_hours = capacityHours
        if (form.skip_defaults) payload.skip_defaults = true
        return payload
    }

    /** Update payload for every field; blanks clear the stored value. */
    function updatePayload(sprint: number, label: string): SprintUpdateRequest {
        return {
            sprint,
            label,
            goal: toTrimmedString(form.goal),
            plan_length: toTrimmedString(form.plan_length),
            ends_at: fromDateTimeInputValue(form.ends_at) ?? '',
            starts_at: fromDateTimeInputValue(form.starts_at) ?? '',
            overdue_after: toTrimmedString(form.overdue_after),
            notes: toStringValue(form.notes),
            capacity_points: parseNumberInput(form.capacity_points),
            capacity_hours: parseNumberInput(form.capacity_hours),
        }
    }

    return { form, resetForm, fillFormFromSprint, createPayload, updatePayload }
}
