import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it } from 'vitest'
import ConfigGroup from '../components/ConfigGroup.vue'
import ConfigGroupSource from '../components/ConfigGroup.vue?raw'

function mountGroup(source: 'project' | 'global' | 'built_in' | undefined = 'project') {
    return mount(ConfigGroup, {
        attachTo: document.body,
        props: {
            title: 'Sample group',
            description: 'Details about this group',
            source,
        },
        slots: {
            default: '<div class="sentinel">Content</div>',
        },
    })
}

function extractStyle(source: string): string {
    const match = source.match(/<style[^>]*>([\s\S]*?)<\/style>/)
    return match ? match[1] : ''
}

describe('ConfigGroup styles', () => {
    afterEach(() => {
        document.body.innerHTML = ''
    })

    it('applies flex-based layout to the wrapper and content', () => {
        const wrapper = mountGroup()
        const styleContent = extractStyle(ConfigGroupSource)

        expect(styleContent).toMatch(/\.config-group[^{}]*\{[^}]*display:\s*flex/)
        expect(styleContent).toMatch(/\.config-group[^{}]*\{[^}]*flex-direction:\s*column/)
        expect(styleContent).toMatch(/\.config-group[^{}]*\{[^}]*padding:\s*16px\s*20px/)
        expect(styleContent).toMatch(/\.config-group__content[^{}]*\{[^}]*display:\s*flex/)
        expect(styleContent).toMatch(/\.config-group__content[^{}]*\{[^}]*gap:\s*10px/)

        wrapper.unmount()
    })

    it('renders a styled source badge for scoped configurations', () => {
        const wrapper = mountGroup('project')
        const styleContent = extractStyle(ConfigGroupSource)

        expect(styleContent).toMatch(/\.config-group__source\.source-project[^{}]*\{[^}]*background:\s*rgba\(0,\s*180,\s*120,\s*0\.25\)/)
        expect(styleContent).toMatch(/\.config-group__source[^{}]*\{[^}]*border-radius:\s*999px/)
        expect(styleContent).toMatch(/\.config-group__source[^{}]*\{[^}]*text-transform:\s*uppercase/)

        wrapper.unmount()
    })
})
