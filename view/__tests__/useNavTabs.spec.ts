import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
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

    it('loads Dependencies choices without overriding older hidden tabs or unknown paths', async () => {
        await nextTick()
        localStorage.setItem('lotar.preferences.hiddenTabs', JSON.stringify(['/sync', '/unknown', 42, null]))
        vi.resetModules()
        const { useNavTabs: reload } = await import('../composables/useNavTabs')
        const tabs = reload()
        expect(tabs.visibleTabs.value.map((tab) => tab.path)).toContain('/dependencies')
        expect(tabs.isTabVisible('/sync')).toBe(false)
        expect(tabs.hiddenTabs.value.has('/unknown')).toBe(true)
        expect([...tabs.hiddenTabs.value]).toEqual(['/sync', '/unknown'])
        expect(tabs.visibleTabs.value.some((tab) => tab.path === '/unknown')).toBe(false)
        tabs.toggleTab('/dependencies')
        await nextTick()
        vi.resetModules()
        const persisted = (await import('../composables/useNavTabs')).useNavTabs()
        expect(persisted.isTabVisible('/dependencies')).toBe(false)
        expect(persisted.isTabVisible('/sync')).toBe(false)
        expect(persisted.isTabVisible('/preferences')).toBe(true)
        persisted.setTabHidden('/dependencies', false)
        await nextTick()
        expect(JSON.parse(localStorage.getItem('lotar.preferences.hiddenTabs')!)).toEqual(['/sync', '/unknown'])
    })

    it.each(['not json', '{}', 'null'])('falls back to visible tabs for invalid storage %s', async (raw) => {
        await nextTick()
        localStorage.setItem('lotar.preferences.hiddenTabs', raw)
        vi.resetModules()
        const tabs = (await import('../composables/useNavTabs')).useNavTabs()
        expect(tabs.isTabVisible('/dependencies')).toBe(true)
        expect(tabs.hiddenTabs.value.size).toBe(0)
    })

    it('restores default visibility after clearing browser preferences and reloading', async () => {
        await nextTick()
        localStorage.clear()
        vi.resetModules()
        const tabs = (await import('../composables/useNavTabs')).useNavTabs()
        expect(tabs.visibleTabs.value.map((tab) => tab.path)).toContain('/dependencies')
    })

    it('defaults to visible Dependencies when browser storage is unavailable', async () => {
        await nextTick()
        const getItem = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
            throw new Error('Storage unavailable')
        })
        try {
            vi.resetModules()
            const tabs = (await import('../composables/useNavTabs')).useNavTabs()
            expect(tabs.visibleTabs.value.map((tab) => tab.path)).toContain('/dependencies')
        } finally {
            getItem.mockRestore()
        }
    })
})
