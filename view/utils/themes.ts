/**
 * Themes for the web UI (DEV-218 – DEV-220). A theme is a skin (fonts,
 * corners, backgrounds, effects) plus a colour palette. Each skin × palette
 * has a light and a dark version in `view/themes.css`, selected by
 * attributes on an element:
 *
 *   data-theme-name="<skin>"        which skin
 *   data-theme-palette="<palette>"  which of its palettes (absent = its default)
 *   data-mode="light|dark"          which version
 *
 * The app sets these on <html>; theme previews set them on their own element.
 *
 * A choice is written `skin` or `skin:palette` (e.g. `terminal:red`) in
 * `ui.theme` and in the browser. The first palette is the skin's default.
 * Keep skins and palettes in sync with `UI_THEMES` in
 * src/services/branding_service.rs; the theme spec checks this.
 */
export type ThemeGroup = 'classic' | 'playful'

export interface PaletteInfo {
    id: string
    name: string
    /** Accent colour shown on the palette's swatch. */
    swatch: string
}

export interface ThemeInfo {
    id: string
    name: string
    description: string
    group: ThemeGroup
    /** Colour palettes; the first is the default. Absent: one fixed palette. */
    palettes?: readonly PaletteInfo[]
}

export const THEMES = [
    {
        id: 'classic',
        name: 'Classic',
        description: 'Clean and calm; pick a colour palette.',
        group: 'classic',
        palettes: [
            { id: 'lotar', name: 'LoTaR', swatch: '#0ea5e9' },
            { id: 'ocean', name: 'Ocean', swatch: '#3b6a96' },
            { id: 'forest', name: 'Forest', swatch: '#2f7d4f' },
            { id: 'ember', name: 'Ember', swatch: '#b4532a' },
            { id: 'violet', name: 'Violet', swatch: '#7047d6' },
            { id: 'rose', name: 'Rose', swatch: '#c03a6b' },
        ],
    },
    {
        id: 'contrast',
        name: 'High contrast',
        description: 'Maximum legibility: black, white and strong borders.',
        group: 'classic',
    },
    {
        id: 'terminal',
        name: 'Terminal',
        description: 'Monospace console: printout paper by day, phosphor glow by night.',
        group: 'playful',
        palettes: [
            { id: 'amber', name: 'Amber', swatch: '#ffb000' },
            { id: 'green', name: 'Green', swatch: '#33ff66' },
            { id: 'red', name: 'Red', swatch: '#ff4d4d' },
            { id: 'cyan', name: 'Cyan', swatch: '#3ee6ff' },
            { id: 'white', name: 'White', swatch: '#e8e8e8' },
        ],
    },
    {
        id: 'matrix',
        name: 'Matrix',
        description: 'Glowing code rain on black.',
        group: 'playful',
        palettes: [
            { id: 'green', name: 'Green', swatch: '#00ff41' },
            { id: 'red', name: 'Red pill', swatch: '#ff3b3b' },
            { id: 'blue', name: 'Blue pill', swatch: '#3b9dff' },
        ],
    },
    {
        id: 'synthwave',
        name: 'Synthwave',
        description: '80s neon and a sunset grid.',
        group: 'playful',
        palettes: [
            { id: 'sunset', name: 'Sunset', swatch: '#ff3fb4' },
            { id: 'outrun', name: 'Outrun', swatch: '#ff8c1a' },
            { id: 'ice', name: 'Ice', swatch: '#3ee6ff' },
        ],
    },
    {
        id: 'blueprint',
        name: 'Blueprint',
        description: 'Technical drawing: grid paper and ink.',
        group: 'playful',
        palettes: [
            { id: 'blue', name: 'Blue', swatch: '#1b5fbf' },
            { id: 'green', name: 'Engineering green', swatch: '#1f7a55' },
            { id: 'sepia', name: 'Sepia', swatch: '#8a5a2b' },
        ],
    },
    {
        id: 'paper',
        name: 'Paper',
        description: 'Serif notebook on warm ruled paper.',
        group: 'playful',
        palettes: [
            { id: 'blue', name: 'Blue ink', swatch: '#2c4f8a' },
            { id: 'black', name: 'Black ink', swatch: '#2b2b2b' },
            { id: 'red', name: 'Red ink', swatch: '#9e2a3a' },
            { id: 'green', name: 'Green ink', swatch: '#2f6b3f' },
        ],
    },
    {
        id: 'retro',
        name: 'Retro',
        description: '90s desktop: grey chrome and bevels.',
        group: 'playful',
        palettes: [
            { id: 'teal', name: 'Teal', swatch: '#008080' },
            { id: 'plum', name: 'Plum', swatch: '#800080' },
            { id: 'slate', name: 'Slate', swatch: '#3c5a78' },
            { id: 'desert', name: 'Desert', swatch: '#a0794b' },
        ],
    },
    {
        id: 'candy',
        name: 'Candy',
        description: 'Pastels, round and bouncy.',
        group: 'playful',
        palettes: [
            { id: 'bubblegum', name: 'Bubblegum', swatch: '#c92a75' },
            { id: 'mint', name: 'Mint', swatch: '#11785c' },
            { id: 'grape', name: 'Grape', swatch: '#7a3fd1' },
        ],
    },
] as const satisfies readonly ThemeInfo[]

export type ThemeId = (typeof THEMES)[number]['id']

/** The repository's own skin (`ui.theme_file`), when one is configured. */
export const CUSTOM_THEME = 'custom'

export const DEFAULT_THEME: ThemeId = 'classic'

/** Ids from DEV-218/219, before palettes: still accepted everywhere. */
export const LEGACY_THEME_ALIASES: Readonly<Record<string, string>> = {
    lotar: 'classic',
    ocean: 'classic:ocean',
    forest: 'classic:forest',
    ember: 'classic:ember',
    violet: 'classic:violet',
    rose: 'classic:rose',
}

export interface ThemeChoice {
    theme: ThemeId | typeof CUSTOM_THEME
    /** Palette id; null = the skin's default palette. */
    palette: string | null
}

export function findTheme(id: string): ThemeInfo | undefined {
    return (THEMES as readonly ThemeInfo[]).find((theme) => theme.id === id)
}

export function isThemeId(value: unknown): value is ThemeId {
    return typeof value === 'string' && findTheme(value) !== undefined
}

/**
 * Read `skin` or `skin:palette` (or a legacy id). Returns null for anything
 * unknown. `allowCustom` accepts the repository skin.
 */
export function parseThemeValue(raw: unknown, allowCustom = false): ThemeChoice | null {
    if (typeof raw !== 'string' || !raw.trim()) return null
    const value = LEGACY_THEME_ALIASES[raw.trim()] ?? raw.trim()
    const [themeId, palette = null, ...rest] = value.split(':')
    if (rest.length) return null
    if (themeId === CUSTOM_THEME) return allowCustom && palette === null ? { theme: CUSTOM_THEME, palette: null } : null
    const theme = findTheme(themeId ?? '')
    if (!theme) return null
    if (palette === null) return { theme: theme.id as ThemeId, palette: null }
    if (!theme.palettes?.some((option) => option.id === palette)) return null
    // The default palette is written without a suffix.
    return { theme: theme.id as ThemeId, palette: theme.palettes[0]!.id === palette ? null : palette }
}

/** `skin` or `skin:palette`, omitting the default palette. */
export function formatThemeValue(choice: ThemeChoice): string {
    const theme = findTheme(choice.theme)
    const isDefault = !choice.palette || theme?.palettes?.[0]?.id === choice.palette
    return isDefault ? choice.theme : `${choice.theme}:${choice.palette}`
}

/** The palette actually shown: the chosen one or the skin's default. */
export function effectivePalette(choice: ThemeChoice): string | null {
    return choice.palette ?? findTheme(choice.theme)?.palettes?.[0]?.id ?? null
}

/** "Terminal · Red", "Classic · Ocean", "High contrast". */
export function themeLabel(choice: ThemeChoice, customName = 'Repository theme'): string {
    if (choice.theme === CUSTOM_THEME) return customName
    const theme = findTheme(choice.theme)
    if (!theme) return choice.theme
    const paletteId = effectivePalette(choice)
    const palette = theme.palettes?.find((option) => option.id === paletteId)
    return palette && theme.palettes!.length > 1 ? `${theme.name} · ${palette.name}` : theme.name
}
