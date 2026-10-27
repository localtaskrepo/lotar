import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import ThemePicker from '../components/ThemePicker.vue'
import { balancedColumns } from '../utils/themeLayout'
import { readPaletteButtonPreference, storePaletteButtonPreference } from '../utils/preferences'
import { THEMES } from '../utils/themes'

// DEV-219: the theme picker never strands a lone card, names where the
// configured default comes from, and the header palette button can be hidden.
describe('balancedColumns', () => {
    it('keeps the last row full or more than half full', () => {
        expect(balancedColumns(7, 10)).toBe(7)
        expect(balancedColumns(7, 6)).toBe(4) // 4 + 3, not 6 + 1
        expect(balancedColumns(7, 5)).toBe(4)
        expect(balancedColumns(8, 3)).toBe(3) // 3 + 3 + 2
        expect(balancedColumns(6, 4)).toBe(3) // 3 + 3
        expect(balancedColumns(7, 3)).toBe(1) // 3 + 3 + 1 and 2+2+2+1 both strand one: list
        expect(balancedColumns(7, 1)).toBe(1)
    })

    it('never leaves a single card on the last row', () => {
        for (let count = 2; count <= 16; count += 1) {
            for (let fit = 1; fit <= 12; fit += 1) {
                const columns = balancedColumns(count, fit)
                if (columns === 1) continue
                expect(columns).toBeLessThanOrEqual(fit)
                const rest = count % columns
                expect(rest === 0 || rest > columns / 2).toBe(true)
            }
        }
    })
})

describe('ThemePicker', () => {
    it('offers the configured default apart from the grouped themes', async () => {
        const wrapper = mount(ThemePicker, {
            props: { modelValue: null, defaultTheme: 'forest', defaultHint: 'Forest · from your ~/.lotar' },
        })
        const defaultOption = wrapper.find('.theme-option--default')
        expect(defaultOption.text()).toContain('Configured default')
        expect(defaultOption.text()).toContain('Forest · from your ~/.lotar')
        expect(defaultOption.classes()).toContain('theme-option--selected')

        const headings = wrapper.findAll('.theme-picker__heading').map((heading) => heading.text())
        expect(headings[0]).toBe('Classic')
        expect(wrapper.findAll('.theme-picker__grid .theme-option')).toHaveLength(THEMES.length)

        await wrapper.findAll('.theme-picker__grid input')[1]!.setValue(true)
        const emitted = wrapper.emitted('update:modelValue')!
        expect(emitted[emitted.length - 1]).toEqual([THEMES[1]!.id])
    })
})

describe('palette button preference', () => {
    it('defaults to shown and remembers hiding it', () => {
        localStorage.clear()
        expect(readPaletteButtonPreference()).toBe(true)
        storePaletteButtonPreference(false)
        expect(readPaletteButtonPreference()).toBe(false)
    })
})
