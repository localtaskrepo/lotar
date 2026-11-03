import { mount } from '@vue/test-utils'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import type { TaskDTO } from '../api/types'
import SprintTaskCell from '../components/SprintTaskCell.vue'

// DEV-288: a date-only due date is a local calendar day. Parsed with
// `new Date('2026-11-05')` it is UTC midnight, which west of UTC renders as
// the day before.
describe('SprintTaskCell due date', () => {
    const previousTz = process.env.TZ

    beforeAll(() => {
        process.env.TZ = 'America/Los_Angeles'
    })

    afterAll(() => {
        if (previousTz === undefined) delete process.env.TZ
        else process.env.TZ = previousTz
    })

    function mountDue(due: string | null) {
        const task = { id: 'DEMO-1', title: 'Demo', sprints: [1], tags: [], due_date: due } as unknown as TaskDTO
        return mount(SprintTaskCell, {
            props: {
                task,
                col: 'due_date',
                contextSprintId: 1,
                multiSprint: false,
                overdue: false,
                multiSprintTooltip: () => '',
                sprintLabel: () => '',
                sprintStateClass: () => '',
            },
        })
    }

    it('shows the stored calendar day west of UTC', () => {
        // Guard: the zone switch took effect, so the old parsing would fail here.
        expect(new Date('2026-11-05').getDate()).toBe(4)

        const wrapper = mountDue('2026-11-05')
        expect(wrapper.text()).toBe(new Date(2026, 10, 5).toLocaleDateString())
        wrapper.unmount()
    })

    it('shows a dash when there is no due date', () => {
        const wrapper = mountDue(null)
        expect(wrapper.text()).toBe('—')
        wrapper.unmount()
    })
})
