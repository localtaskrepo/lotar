import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// DEV-220: index.html loads the repository skin once; applyBranding reloads it
// only when a later response names a different skin.
describe('applyBranding skin stylesheet', () => {
    let link: HTMLLinkElement

    beforeEach(() => {
        vi.resetModules()
        link = document.createElement('link')
        link.rel = 'stylesheet'
        link.setAttribute('href', '/api/branding/theme.css')
        document.head.append(link)
    })

    afterEach(() => {
        link.remove()
    })

    it('keeps the stylesheet index.html loaded on the first response', async () => {
        const { applyBranding } = await import('../composables/useBranding')
        applyBranding({ theme: 'custom', theme_file: '.tasks/skin.css' })
        expect(link.getAttribute('href')).toBe('/api/branding/theme.css')
    })

    it('reloads the stylesheet when a later response names another skin', async () => {
        const { applyBranding } = await import('../composables/useBranding')
        applyBranding({ theme: 'custom', theme_file: '.tasks/skin.css' })
        applyBranding({ theme: 'custom', theme_file: '.tasks/other.css' })
        expect(link.getAttribute('href')).toMatch(/^\/api\/branding\/theme\.css\?v=\d+$/)
    })
})
