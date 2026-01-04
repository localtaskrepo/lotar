import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import MarkdownContent from '../components/MarkdownContent.vue'

describe('MarkdownContent', () => {
    it('renders basic markdown', () => {
        const wrapper = mount(MarkdownContent, {
            props: {
                source: 'Hello **world**',
            },
        })

        const strong = wrapper.find('strong')
        expect(strong.exists()).toBe(true)
        expect(strong.text()).toBe('world')
    })

    it('sanitizes unsafe HTML', () => {
        const wrapper = mount(MarkdownContent, {
            props: {
                source: 'Hi <script>alert(1)</script> there',
            },
        })

        expect(wrapper.html()).not.toContain('<script')
        expect(wrapper.text()).toContain('Hi')
        expect(wrapper.text()).toContain('there')
    })
})
