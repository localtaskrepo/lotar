import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { toasts } from '../components/toast'
import Preferences from '../pages/Preferences.vue'

// DEV-288: "Reset saved columns and sorting" must clear every per-page table,
// sort and card-field layout it promises, and confirm that it did.
describe('Preferences table reset', () => {
    beforeEach(() => {
        localStorage.clear()
        toasts.value = []
    })

    afterEach(() => {
        localStorage.clear()
        toasts.value = []
    })

    it('clears saved columns, sorts and field layouts for every project and confirms', async () => {
        const cleared = [
            'lotar.taskTable.columns::ACME',
            'lotar.taskTable.sort',
            'lotar.tasks.sort',
            'lotar.tasks.sort::ACME',
            'lotar.sprints.sort',
            'lotar.boardFields::ACME',
            'lotar.calendarHoverFields',
            'lotar.dependencyHoverFields',
            'lotar.dependencyHoverFields::ACME',
        ]
        const kept = ['lotar.tasks.filter', 'lotar.preferences.hiddenTabs', 'lotar.theme']
        for (const key of [...cleared, ...kept]) localStorage.setItem(key, '"x"')

        const wrapper = mount(Preferences)
        const before = toasts.value.length
        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Reset saved columns and sorting')!
            .trigger('click')

        expect(cleared.filter((key) => localStorage.getItem(key) !== null)).toEqual([])
        for (const key of kept) expect(localStorage.getItem(key), key).not.toBeNull()
        expect(toasts.value.slice(before).map((toast) => toast.message)).toEqual([
            'Saved columns and sorting were reset',
        ])
        wrapper.unmount()
    })
})
