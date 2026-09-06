import { storageGet, storageRemove, storageSet } from './storage'
import { darken, getContrastingColor, normalizeHex, withAlpha } from './color'

export type ThemePreference = 'system' | 'light' | 'dark'

const THEME_KEY = 'lotar.theme'
const ACCENT_KEY = 'lotar.accent'

export const DEFAULT_ACCENT = '#0ea5e9'

const LIGHT_CHROME_COLOR = '#f8fafc'
const DARK_CHROME_COLOR = '#111315'

let currentThemePreference: ThemePreference = 'system'
let prefersDarkQuery: MediaQueryList | null = null
let systemThemeChangeHandler: (() => void) | null = null

export function readThemePreference(): ThemePreference {
    const stored = storageGet(THEME_KEY) as ThemePreference | null
    if (stored === 'light' || stored === 'dark' || stored === 'system') {
        return stored
    }
    return 'system'
}

export function applyThemePreference(theme: ThemePreference) {
    currentThemePreference = theme
    ensureSystemThemeListener()
    if (typeof document === 'undefined') return
    const root = document.documentElement
    if (theme === 'system') {
        delete (root as any).dataset.theme
    } else {
        (root as any).dataset.theme = theme
    }
    updateChromeThemeColor()
}

export function storeThemePreference(theme: ThemePreference) {
    storageSet(THEME_KEY, theme)
}

export function normalizeAccent(value: string): string | null {
    return normalizeHex(value)
}

export function readAccentPreference(): string | null {
    const stored = storageGet(ACCENT_KEY)
    return normalizeHex(stored ?? undefined)
}

export function applyAccentPreference(accent: string | null) {
    if (typeof document === 'undefined') return
    const root = document.documentElement
    if (!accent) {
        root.style.removeProperty('--color-accent')
        root.style.removeProperty('--color-accent-strong')
        root.style.removeProperty('--color-accent-contrast')
        root.style.removeProperty('--focus-ring')
        return
    }
    const normalized = normalizeHex(accent)
    if (!normalized) return
    const strong = darken(normalized, 0.2)
    const contrast = getContrastingColor(normalized)
    root.style.setProperty('--color-accent', normalized)
    root.style.setProperty('--color-accent-strong', strong)
    root.style.setProperty('--color-accent-contrast', contrast)
    root.style.setProperty('--focus-ring', `0 0 0 1px ${withAlpha(normalized, 0.35)}, 0 0 0 4px ${withAlpha(normalized, 0.2)}`)
}

export function storeAccentPreference(accent: string | null) {
    if (!accent) {
        storageRemove(ACCENT_KEY)
        return
    }
    const normalized = normalizeHex(accent)
    if (!normalized) return
    storageSet(ACCENT_KEY, normalized)
}

export function initializeThemeFromStorage() {
    ensureSystemThemeListener()
    const theme = readThemePreference()
    applyThemePreference(theme)
    const accent = readAccentPreference()
    applyAccentPreference(accent)
}

function ensureSystemThemeListener() {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return
    if (systemThemeChangeHandler) return
    prefersDarkQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const listener = () => {
        if (currentThemePreference === 'system') {
            updateChromeThemeColor()
        }
    }
    if (typeof prefersDarkQuery.addEventListener === 'function') {
        prefersDarkQuery.addEventListener('change', listener)
    } else if (typeof prefersDarkQuery.addListener === 'function') {
        prefersDarkQuery.addListener(listener)
    }
    systemThemeChangeHandler = listener
}

function getSystemTheme(): 'light' | 'dark' {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return 'light'
    if (!prefersDarkQuery) {
        prefersDarkQuery = window.matchMedia('(prefers-color-scheme: dark)')
    }
    return prefersDarkQuery.matches ? 'dark' : 'light'
}

function getEffectiveTheme(): 'light' | 'dark' {
    if (currentThemePreference === 'system') {
        return getSystemTheme()
    }
    return currentThemePreference
}

function updateChromeThemeColor() {
    if (typeof document === 'undefined') return
    const metaTags = document.querySelectorAll('meta[name="theme-color"]') as NodeListOf<HTMLMetaElement>
    if (!metaTags.length) return
    const color = getEffectiveTheme() === 'dark' ? DARK_CHROME_COLOR : LIGHT_CHROME_COLOR
    metaTags.forEach((meta) => {
        meta.setAttribute('content', color)
    })
}
