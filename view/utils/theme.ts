import { storageGet, storageRemove, storageSet } from './storage'
import { darken, getContrastingColor, normalizeHex, withAlpha } from './color'
import { DEFAULT_THEME, isThemeId, type ThemeId } from './themes'

/** Light/dark mode choice (kept under its historic name and storage key). */
export type ThemePreference = 'system' | 'light' | 'dark'

const THEME_KEY = 'lotar.theme'
const ACCENT_KEY = 'lotar.accent'
/** The user's colour theme; absent means "use the repository default". */
const THEME_NAME_KEY = 'lotar.themeName'
/**
 * Last repository default seen, so the inline script in index.html can apply
 * the right theme before the app has asked the server (no flash).
 */
const REPOSITORY_THEME_KEY = 'lotar.repositoryTheme'

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
    applyMode()
}

/** Put the resolved light/dark mode on <html> as `data-mode`. */
function applyMode() {
    if (typeof document === 'undefined') return
    const root = document.documentElement
    root.dataset.mode = getEffectiveTheme()
    updateChromeThemeColor()
}

let repositoryTheme: ThemeId | null = readCachedRepositoryTheme()

function readCachedRepositoryTheme(): ThemeId | null {
    const cached = storageGet(REPOSITORY_THEME_KEY)
    return isThemeId(cached) ? cached : null
}

/** The user's own theme choice, or null to follow the repository default. */
export function readThemeNamePreference(): ThemeId | null {
    const stored = storageGet(THEME_NAME_KEY)
    return isThemeId(stored) ? stored : null
}

export function storeThemeNamePreference(theme: ThemeId | null) {
    if (theme) storageSet(THEME_NAME_KEY, theme)
    else storageRemove(THEME_NAME_KEY)
}

/** The repository's default theme (`ui.theme`), as last reported by the server. */
export function getRepositoryTheme(): ThemeId {
    return repositoryTheme ?? DEFAULT_THEME
}

/** Record the repository default and re-apply when the user follows it. */
export function setRepositoryTheme(theme: string | null | undefined) {
    repositoryTheme = isThemeId(theme) ? theme : null
    if (repositoryTheme) storageSet(REPOSITORY_THEME_KEY, repositoryTheme)
    else storageRemove(REPOSITORY_THEME_KEY)
    applyThemeName(readThemeNamePreference())
}

/** Theme in effect: the user's choice, else the repository default. */
export function effectiveThemeName(choice: ThemeId | null = readThemeNamePreference()): ThemeId {
    return choice ?? getRepositoryTheme()
}

/** Apply a theme choice (null = repository default) to <html>. */
export function applyThemeName(choice: ThemeId | null) {
    if (typeof document === 'undefined') return
    document.documentElement.dataset.themeName = effectiveThemeName(choice)
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
    root.style.setProperty(
        '--focus-ring',
        `0 0 0 1px ${withAlpha(normalized, 0.35)}, 0 0 0 4px ${withAlpha(normalized, 0.2)}`,
    )
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
    // The old explicit-mode attribute; `data-mode` replaced it.
    if (typeof document !== 'undefined') delete document.documentElement.dataset.theme
    applyThemeName(readThemeNamePreference())
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
            applyMode()
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
    // The browser chrome matches the theme's page background.
    const themed =
        typeof getComputedStyle === 'function'
            ? getComputedStyle(document.documentElement).getPropertyValue('--color-bg').trim()
            : ''
    const color = themed || (getEffectiveTheme() === 'dark' ? DARK_CHROME_COLOR : LIGHT_CHROME_COLOR)
    metaTags.forEach((meta) => {
        meta.setAttribute('content', color)
    })
}
