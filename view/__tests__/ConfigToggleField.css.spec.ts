import { mount } from '@vue/test-utils'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, describe, expect, it } from 'vitest'
import ConfigToggleField from '../components/ConfigToggleField.vue'
import ConfigToggleFieldSource from '../components/ConfigToggleField.vue?raw'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const globalStyleSource = readFileSync(resolve(__dirname, '../styles.css'), 'utf8')

type ToggleValue = 'inherit' | 'true' | 'false'

describe('ConfigToggleField styles', () => {
    afterEach(() => {
        document.body.innerHTML = ''
    })

    function extractStyle(source: string): string {
        const match = source.match(/<style[^>]*>([\s\S]*?)<\/style>/)
        return match ? match[1] : ''
    }

    function mountToggle(modelValue: ToggleValue = 'inherit', extraProps: Record<string, unknown> = {}) {
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
                ...extraProps,
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
        const wrapper = mountToggle('inherit', {
            sourceLabel: 'Project',
            sourceClass: 'source-project',
        })

        expect(globalStyleSource).toMatch(/\.provenance[^{}]*\{[^}]*text-transform:\s*uppercase/)
        expect(globalStyleSource).toMatch(/\.provenance\.source-project[^{}]*\{[^}]*background:\s*rgba\(0,\s*180,\s*120,\s*0\.25\)/)
        expect(globalStyleSource).toMatch(/\.provenance\.source-global[^{}]*\{[^}]*color:\s*#8bc0ff/)

        const badge = wrapper.find('.provenance')
        expect(badge.exists()).toBe(true)
        expect(badge.classes()).toContain('source-project')

        wrapper.unmount()
    })
})
