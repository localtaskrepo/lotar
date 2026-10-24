import { mount } from '@vue/test-utils'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, describe, expect, it } from 'vitest'
import ConfigToggleField from '../components/ConfigToggleField.vue'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const configFormStyle = readFileSync(resolve(__dirname, '../pages/config-form.css'), 'utf8')

type ToggleValue = 'inherit' | 'true' | 'false'

// DEV-157: toggles render as settings rows — label and explanation on the
// left, a switch (global) or an Inherit/On/Off select (project) on the right.
describe('ConfigToggleField settings row', () => {
    afterEach(() => {
        document.body.innerHTML = ''
    })

    function mountToggle(isGlobal: boolean, modelValue: ToggleValue, extraProps: Record<string, unknown> = {}) {
        const updates: ToggleValue[] = []
        const wrapper = mount(ConfigToggleField, {
            attachTo: document.body,
            props: {
                label: 'Auto-set reporter',
                description: 'Fill in the reporter when a task has none.',
                isGlobal,
                options: [
                    { value: 'inherit', label: 'Inherit global (Currently On)' },
                    { value: 'true', label: 'Enabled' },
                    { value: 'false', label: 'Disabled' },
                ],
                modelValue,
                'onUpdate:modelValue': (value: ToggleValue) => updates.push(value),
                ...extraProps,
            },
        })
        return { wrapper, updates }
    }

    it('uses an accessible switch in global scope', async () => {
        const { wrapper, updates } = mountToggle(true, 'true')
        const input = wrapper.get('input[role="switch"]')
        const labelId = input.attributes('aria-labelledby')!
        expect(wrapper.get(`#${labelId}`).text()).toContain('Auto-set reporter')
        expect(wrapper.get(`#${input.attributes('aria-describedby')}`).text()).toContain('Fill in the reporter')
        expect((input.element as HTMLInputElement).checked).toBe(true)

        await input.setValue(false)
        expect(updates).toEqual(['false'])
        expect(wrapper.find('select').exists()).toBe(false)
    })

    it('offers inherit, on and off in project scope', async () => {
        const { wrapper, updates } = mountToggle(false, 'inherit')
        const select = wrapper.get('select')
        expect(select.findAll('option').map((option) => option.text())).toEqual([
            'Inherit global (Currently On)',
            'Enabled',
            'Disabled',
        ])
        await select.setValue('false')
        expect(updates).toEqual(['false'])
        expect(wrapper.find('input[role="switch"]').exists()).toBe(false)
    })

    it('shows the provenance badge next to the label', () => {
        const { wrapper } = mountToggle(false, 'true', { sourceLabel: 'Project', sourceClass: 'source-project' })
        const badge = wrapper.get('.cfg-toggle__label .provenance')
        expect(badge.text()).toBe('Project')
        expect(badge.classes()).toContain('source-project')
    })

    it('keeps the control column separate from wrapping label text', () => {
        expect(configFormStyle).toMatch(/\.cfg-toggle\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)\s+auto/)
    })
})
