import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it } from 'vitest'
import { defineComponent, nextTick } from 'vue'
import { NAV_TABS, useNavTabs } from '../composables/useNavTabs'

function mountHarness() {
    const component = defineComponent({
        setup() {
            return useNavTabs()
        },
        template: '<div />',
    })
    return mount(component)
}

describe('useNavTabs', () => {
    beforeEach(() => {
        localStorage.clear()
        // reset module state by hiding then unhiding everything
        const { setTabHidden } = useNavTabs()
        for (const tab of NAV_TABS) setTabHidden(tab.path, false)
    })

    it('shows every tab by default', () => {
        const { visibleTabs } = useNavTabs()
        expect(visibleTabs.value.map((t) => t.path)).toEqual(NAV_TABS.map((t) => t.path))
    })

    it('hides and restores tabs with persistence', async () => {
        const wrapper = mountHarness()
        wrapper.vm.setTabHidden('/sync', true)
        wrapper.vm.setTabHidden('/scan', true)
        await nextTick()
        expect(wrapper.vm.isTabVisible('/sync')).toBe(false)
        expect(wrapper.vm.isTabVisible('/scan')).toBe(false)
        const stored = JSON.parse(localStorage.getItem('lotar.preferences.hiddenTabs') || '[]')
        expect(stored).toEqual(['/sync', '/scan'])

        wrapper.vm.setTabHidden('/sync', false)
        await nextTick()
        expect(wrapper.vm.isTabVisible('/sync')).toBe(true)
        const stored2 = JSON.parse(localStorage.getItem('lotar.preferences.hiddenTabs') || '[]')
        expect(stored2).toEqual(['/scan'])
    })

    it('never hides protected paths', () => {
        const wrapper = mountHarness()
        wrapper.vm.setTabHidden('/preferences', true)
        expect(wrapper.vm.isTabVisible('/preferences')).toBe(true)
    })
})
