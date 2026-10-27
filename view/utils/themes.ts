/**
 * Themes for the web UI (DEV-218, DEV-219). Each theme has a light and a dark
 * token set in `view/themes.css`, selected by attributes on an element:
 *
 *   data-theme-name="<id>"   which theme
 *   data-mode="light|dark"   which of its two sets
 *
 * The app sets both on <html>; theme previews set them on their own element,
 * so a preview shows its theme regardless of the page's.
 *
 * Keep the ids in sync with `UI_THEMES` in src/services/branding_service.rs
 * (validation of the repository-wide `ui.theme` setting); the theme spec
 * checks this.
 */
export type ThemeGroup = 'classic' | 'playful'

export interface ThemeInfo {
    id: ThemeId
    name: string
    description: string
    /** Calm everyday palettes vs. themes with a strong character. */
    group: ThemeGroup
}

export const THEMES = [
    { id: 'lotar', name: 'LoTaR', description: 'Sky blue on cool slate; the default.', group: 'classic' },
    { id: 'ocean', name: 'Ocean', description: 'Calm blue-grey, soft contrast.', group: 'classic' },
    { id: 'forest', name: 'Forest', description: 'Greens on warm neutrals.', group: 'classic' },
    { id: 'ember', name: 'Ember', description: 'Warm amber and terracotta.', group: 'classic' },
    { id: 'violet', name: 'Violet', description: 'Purple accents on cool greys.', group: 'classic' },
    { id: 'rose', name: 'Rose', description: 'Rose pink on soft blush neutrals.', group: 'classic' },
    {
        id: 'contrast',
        name: 'High contrast',
        description: 'Maximum legibility: black, white and strong borders.',
        group: 'classic',
    },
    {
        id: 'terminal',
        name: 'Terminal',
        description: 'Monospace console: greenbar paper by day, amber phosphor by night.',
        group: 'playful',
    },
    { id: 'matrix', name: 'Matrix', description: 'Glowing green code rain on black.', group: 'playful' },
    { id: 'synthwave', name: 'Synthwave', description: '80s neon: magenta, cyan and a sunset grid.', group: 'playful' },
    {
        id: 'blueprint',
        name: 'Blueprint',
        description: 'Technical drawing: grid paper and blue ink.',
        group: 'playful',
    },
    { id: 'paper', name: 'Paper', description: 'Serif notebook on warm ruled paper.', group: 'playful' },
    { id: 'retro', name: 'Retro', description: '90s desktop: grey chrome, bevels and teal.', group: 'playful' },
    { id: 'candy', name: 'Candy', description: 'Bubblegum pastels, round and bouncy.', group: 'playful' },
] as const satisfies ReadonlyArray<{ id: string; name: string; description: string; group: ThemeGroup }>

export type ThemeId = (typeof THEMES)[number]['id']

export const DEFAULT_THEME: ThemeId = 'lotar'

export function isThemeId(value: unknown): value is ThemeId {
    return typeof value === 'string' && THEMES.some((theme) => theme.id === value)
}
