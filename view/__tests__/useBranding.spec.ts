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

// DEV-345: the tab names the repository and shows its icon.
describe('applyBranding tab', () => {
    let icon: HTMLLinkElement

    beforeEach(() => {
        vi.resetModules()
        icon = document.createElement('link')
        icon.rel = 'icon'
        icon.type = 'image/svg+xml'
        icon.setAttribute('href', '/assets/branding/lotar-app-icon.svg')
        document.head.append(icon)
    })

    afterEach(() => {
        icon.remove()
    })

    it('titles the tab with the repository without a configured name', async () => {
        const { applyBranding } = await import('../composables/useBranding')
        applyBranding({ repository: 'my-app' })
        expect(document.title).toBe('my-app · LoTaR')
        applyBranding({ repository: 'my-app', name: 'Acme' })
        expect(document.title).toBe('Acme')
        applyBranding({})
        expect(document.title).toBe('LoTaR')
    })

    it('switches the tab icon to the repository icon and back', async () => {
        const { applyBranding } = await import('../composables/useBranding')
        applyBranding({ icon: '/api/branding/icon?v=abc', detected_icon: 'public/favicon.svg' })
        expect(icon.getAttribute('href')).toBe('/api/branding/icon?v=abc')
        expect(icon.hasAttribute('type')).toBe(false)
        applyBranding({})
        expect(icon.href).toMatch(/\/assets\/branding\/lotar-app-icon\.svg$/)
        expect(icon.type).toBe('image/svg+xml')
    })
})
