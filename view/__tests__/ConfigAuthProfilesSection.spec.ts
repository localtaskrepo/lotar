import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import ConfigAuthProfilesSection from '../components/ConfigAuthProfilesSection.vue'

const stubs = {
    ConfigGroup: {
        template: '<section><slot /></section>',
        props: ['title', 'description'],
    },
}

describe('ConfigAuthProfilesSection', () => {
    it('hides auth profiles in the UI', () => {
        const wrapper = mount(ConfigAuthProfilesSection, {
            global: { stubs },
        })

        expect(wrapper.text()).toContain('Auth profiles are not displayed in the UI.')
    })
})
