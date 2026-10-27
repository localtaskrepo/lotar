import { beforeEach, describe, expect, it } from 'vitest'
import {
    applyThemeName,
    applyThemePreference,
    effectiveThemeName,
    initializeThemeFromStorage,
    readThemeNamePreference,
    setRepositoryTheme,
    storeThemeNamePreference,
} from '../utils/theme'

// DEV-218: a person's theme choice wins over the repository default, and the
// light/dark mode is always resolved onto <html data-mode>.
describe('theme selection', () => {
    beforeEach(() => {
        localStorage.clear()
        setRepositoryTheme(null)
        delete document.documentElement.dataset.themeName
        delete document.documentElement.dataset.mode
    })

    it('follows the repository default until the person picks a theme', () => {
        setRepositoryTheme('forest')
        expect(document.documentElement.dataset.themeName).toBe('forest')
        expect(localStorage.getItem('lotar.repositoryTheme')).toBe('forest')

        storeThemeNamePreference('rose')
        applyThemeName(readThemeNamePreference())
        expect(document.documentElement.dataset.themeName).toBe('rose')

        // A new repository default does not override the personal choice.
        setRepositoryTheme('ocean')
        expect(document.documentElement.dataset.themeName).toBe('rose')

        storeThemeNamePreference(null)
        applyThemeName(null)
        expect(document.documentElement.dataset.themeName).toBe('ocean')
    })

    it('ignores unknown themes', () => {
        localStorage.setItem('lotar.themeName', 'neon')
        expect(readThemeNamePreference()).toBeNull()
        setRepositoryTheme('neon')
        expect(effectiveThemeName()).toBe('lotar')
    })

    it('resolves the mode, following the system when asked', () => {
        const listeners: Array<() => void> = []
        let dark = false
        // jsdom has no matchMedia; provide a controllable one.
        Object.defineProperty(window, 'matchMedia', {
            configurable: true,
            value: () =>
                ({
                    get matches() {
                        return dark
                    },
                    addEventListener: (_: string, listener: () => void) => listeners.push(listener),
                }) as unknown as MediaQueryList,
        })
        applyThemePreference('dark')
        expect(document.documentElement.dataset.mode).toBe('dark')
        applyThemePreference('light')
        expect(document.documentElement.dataset.mode).toBe('light')

        localStorage.setItem('lotar.theme', 'system')
        initializeThemeFromStorage()
        expect(document.documentElement.dataset.mode).toBe('light')
        dark = true
        listeners.forEach((listener) => listener())
        expect(document.documentElement.dataset.mode).toBe('dark')
        expect(document.documentElement.dataset.theme).toBeUndefined()
    })
})
