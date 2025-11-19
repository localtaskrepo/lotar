import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import SmartListChips from '../components/SmartListChips.vue'

describe('SmartListChips', () => {
    it('emits preset events for custom field shortcuts', async () => {
        const wrapper = mount(SmartListChips, {
            props: {
                value: {},
                statuses: [],
                priorities: [],
                customPresets: [{ label: 'Iteration', expression: 'field:iteration=' }],
            },
        })

        const button = wrapper.findAll('button').find((btn) => btn.text() === 'Iteration')
        expect(button).toBeTruthy()
        await button!.trigger('click')

        const emitted = wrapper.emitted('preset') || []
        expect(emitted[0]).toEqual(['field:iteration='])
    })
})
