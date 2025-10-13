import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it } from 'vitest'
import ConfigToggleField from '../components/ConfigToggleField.vue'
import ConfigToggleFieldSource from '../components/ConfigToggleField.vue?raw'

type ToggleValue = 'inherit' | 'true' | 'false'

describe('ConfigToggleField styles', () => {
    afterEach(() => {
        document.body.innerHTML = ''
    })

    function extractStyle(source: string): string {
        const match = source.match(/<style[^>]*>([\s\S]*?)<\/style>/)
        return match ? match[1] : ''
    }

    function mountToggle(modelValue: ToggleValue = 'inherit') {
        return mount(ConfigToggleField, {
            attachTo: document.body,
            props: {
                label: 'Auto-set reporter',
                isGlobal: false,
                options: [
                    { value: 'inherit', label: 'Inherit' },
                    { value: 'true', label: 'Enabled' },
                    { value: 'false', label: 'Disabled' },
                ],
                modelValue,
                'onUpdate:modelValue': () => { },
            },
        })
    }

    it('exposes flex layout for the wrapper and toggle row', () => {
        const wrapper = mountToggle()
        const style = extractStyle(ConfigToggleFieldSource)

        expect(style).toMatch(/\.toggle-field[^{}]*\{[^}]*display:\s*flex/)
        expect(style).toMatch(/\.toggle-field[^{}]*\{[^}]*flex-direction:\s*column/)
        expect(style).toMatch(/\.toggle-control[^{}]*\{[^}]*display:\s*inline-flex/)
        expect(style).toMatch(/\.toggle-control[^{}]*\{[^}]*gap:\s*8px/)

        wrapper.unmount()
    })

    it('keeps provenance badges styled consistently', () => {
        const wrapper = mountToggle()
        const style = extractStyle(ConfigToggleFieldSource)

        expect(style).toMatch(/\.provenance[^{}]*\{[^}]*text-transform:\s*uppercase/)
        expect(style).toMatch(/\.source-project[^{}]*\{[^}]*background:\s*rgba\(0,\s*180,\s*120,\s*0\.25\)/)
        expect(style).toMatch(/\.source-global[^{}]*\{[^}]*color:\s*#8bc0ff/)

        wrapper.unmount()
    })
})
