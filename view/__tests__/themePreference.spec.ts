import { beforeEach, describe, expect, it } from 'vitest'
import { computed, nextTick } from 'vue'
import {
    applyThemeName,
    applyThemePreference,
    effectiveThemeName,
    getRepositoryTheme,
    previewConfiguredTheme,
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

    it('follows the configured default until the person picks a theme', () => {
        // Legacy ids from before palettes still work.
        setRepositoryTheme('forest')
        expect(document.documentElement.dataset.themeName).toBe('classic')
        expect(document.documentElement.dataset.themePalette).toBe('forest')
        expect(localStorage.getItem('lotar.repositoryTheme')).toBe('classic:forest')

        storeThemeNamePreference('terminal:red')
        applyThemeName(readThemeNamePreference())
        expect(document.documentElement.dataset.themeName).toBe('terminal')
        expect(document.documentElement.dataset.themePalette).toBe('red')

        // A new configured default does not override the personal choice.
        setRepositoryTheme('matrix')
        expect(document.documentElement.dataset.themeName).toBe('terminal')

        storeThemeNamePreference(null)
        applyThemeName(null)
        expect(document.documentElement.dataset.themeName).toBe('matrix')
        // The default palette is applied by id too.
        expect(document.documentElement.dataset.themePalette).toBe('green')
    })

    it('previews unsaved configured themes only for people following the configuration', () => {
        const name = () => document.documentElement.dataset.themeName
        setRepositoryTheme('classic')
        previewConfiguredTheme('matrix:red')
        expect(name()).toBe('matrix')
        expect(document.documentElement.dataset.themePalette).toBe('red')
        previewConfiguredTheme(null) // "Not set": the built-in default
        expect(name()).toBe('classic')
        previewConfiguredTheme(undefined)
        expect(name()).toBe('classic')

        // A personal choice other than the saved configured theme is kept.
        storeThemeNamePreference('terminal:red')
        previewConfiguredTheme('matrix')
        expect(name()).toBe('terminal')
        // A stored choice equal to the configured theme still follows it.
        localStorage.setItem('lotar.themeName', 'classic')
        previewConfiguredTheme('paper')
        expect(name()).toBe('paper')
        previewConfiguredTheme(undefined)
        expect(name()).toBe('classic')
    })

    it('exposes the configured theme reactively', async () => {
        const configured = computed(() => getRepositoryTheme())
        setRepositoryTheme('candy:mint')
        expect(configured.value).toBe('candy:mint')
        setRepositoryTheme('ocean')
        await nextTick()
        expect(configured.value).toBe('classic:ocean')
    })

    it('ignores unknown themes and palettes', () => {
        localStorage.setItem('lotar.themeName', 'neon')
        expect(readThemeNamePreference()).toBeNull()
        storeThemeNamePreference('terminal:plaid')
        expect(readThemeNamePreference()).toBeNull()
        setRepositoryTheme('neon')
        expect(effectiveThemeName()).toBe('classic')
        // The default palette is stored without a suffix.
        storeThemeNamePreference('terminal:amber')
        expect(readThemeNamePreference()).toBe('terminal')
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
