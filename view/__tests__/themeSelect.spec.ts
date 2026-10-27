import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it } from 'vitest'
import ThemeSelect from '../components/ThemeSelect.vue'
import { formatThemeValue, parseThemeValue, themeLabel } from '../utils/themes'

// DEV-220: themes are a skin plus a palette, picked with one select box that
// tags the configured theme.
describe('theme values', () => {
    it('reads skins, palettes and legacy ids', () => {
        expect(parseThemeValue('terminal:red')).toEqual({ theme: 'terminal', palette: 'red' })
        expect(parseThemeValue('terminal:amber')).toEqual({ theme: 'terminal', palette: null })
        expect(parseThemeValue('ocean')).toEqual({ theme: 'classic', palette: 'ocean' })
        expect(parseThemeValue('lotar')).toEqual({ theme: 'classic', palette: null })
        expect(parseThemeValue('terminal:plaid')).toBeNull()
        expect(parseThemeValue('contrast:red')).toBeNull()
        expect(parseThemeValue('custom')).toBeNull()
        expect(parseThemeValue('custom', true)).toEqual({ theme: 'custom', palette: null })
        expect(formatThemeValue({ theme: 'matrix', palette: 'green' })).toBe('matrix')
        expect(themeLabel({ theme: 'classic', palette: 'ocean' })).toBe('Classic · Ocean')
        expect(themeLabel({ theme: 'contrast', palette: null })).toBe('High contrast')
    })
})

describe('ThemeSelect', () => {
    let wrappers: Array<ReturnType<typeof mount>> = []
    afterEach(() => {
        wrappers.forEach((wrapper) => wrapper.unmount())
        wrappers = []
    })

    function mountSelect(props: { modelValue: string | null } & Record<string, unknown>) {
        const wrapper = mount(ThemeSelect, { props, attachTo: document.body })
        wrappers.push(wrapper)
        return wrapper
    }
    const last = (wrapper: ReturnType<typeof mount>) => {
        const emitted = wrapper.emitted('update:modelValue') ?? []
        return emitted[emitted.length - 1]
    }

    it('lists skins, tags the configured one and picks with the keyboard', async () => {
        const wrapper = mountSelect({ modelValue: 'classic:ocean', configured: 'terminal:red' })
        expect(wrapper.find('.theme-select__label').text()).toBe('Classic · Ocean')
        await wrapper.find('.theme-select__trigger').trigger('keydown', { key: 'ArrowDown' })
        const options = wrapper.findAll('[role="option"]')
        expect(options.map((option) => option.find('.theme-select__label').text())).toContain('Terminal')
        const terminal = options.find((option) => option.attributes('data-value') === 'terminal')!
        expect(terminal.find('.theme-select__tag').text()).toBe('Configured')
        expect(wrapper.find('[aria-selected="true"]').attributes('data-value')).toBe('classic')

        const list = wrapper.find('[role="listbox"]')
        await list.trigger('keydown', { key: 't' })
        await list.trigger('keydown', { key: 'Enter' })
        // A different skin starts on its default palette.
        expect(last(wrapper)).toEqual(['terminal'])
        expect(wrapper.find('[role="listbox"]').exists()).toBe(false)
    })

    it('switches palettes with swatches and marks the configured one', async () => {
        const wrapper = mountSelect({ modelValue: 'terminal', configured: 'terminal:red' })
        const swatches = wrapper.findAll('[data-testid="theme-palettes"] [role="radio"]')
        expect(swatches.map((swatch) => swatch.text())).toEqual(['Amber', 'Green', 'Red', 'Cyan', 'White'])
        expect(swatches[0]!.attributes('aria-checked')).toBe('true')
        expect(swatches[2]!.find('.theme-select__dot').exists()).toBe(true)
        await swatches[2]!.trigger('click')
        expect(last(wrapper)).toEqual(['terminal:red'])
        await swatches[0]!.trigger('keydown', { key: 'ArrowRight' })
        expect(last(wrapper)).toEqual(['terminal:green'])
    })

    it('offers "not set" and the repository skin when asked', async () => {
        const wrapper = mountSelect({ modelValue: null, unsetLabel: 'Not set', customName: 'Repository skin' })
        expect(wrapper.find('.theme-select__label').text()).toBe('Not set')
        expect(wrapper.find('.theme-select__hint').text()).toBe('Classic · LoTaR')
        await wrapper.find('.theme-select__trigger').trigger('click')
        const values = wrapper.findAll('[role="option"]').map((option) => option.attributes('data-value'))
        expect(values[0]).toBe('__unset__')
        expect(values).toContain('custom')
        await wrapper.find('[data-value="custom"]').trigger('click')
        expect(last(wrapper)).toEqual(['custom'])
    })
})
