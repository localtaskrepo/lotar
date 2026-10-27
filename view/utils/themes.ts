/**
 * Colour themes for the web UI (DEV-218). Each theme has a light and a dark
 * token set in `view/themes.css`, selected by attributes on an element:
 *
 *   data-theme-name="<id>"   which theme
 *   data-mode="light|dark"   which of its two sets
 *
 * The app sets both on <html>; theme previews set them on their own element,
 * so a preview shows its theme regardless of the page's.
 *
 * Keep the ids in sync with `UI_THEMES` in src/config (validation of the
 * repository-wide `ui.theme` setting).
 */
export interface ThemeInfo {
    id: ThemeId
    name: string
    description: string
}

export const THEMES = [
    { id: 'lotar', name: 'LoTaR', description: 'Sky blue on cool slate; the default.' },
    { id: 'ocean', name: 'Ocean', description: 'Calm blue-grey, soft contrast.' },
    { id: 'forest', name: 'Forest', description: 'Greens on warm neutrals.' },
    { id: 'ember', name: 'Ember', description: 'Warm amber and terracotta.' },
    { id: 'violet', name: 'Violet', description: 'Purple accents on cool greys.' },
    { id: 'rose', name: 'Rose', description: 'Rose pink on soft blush neutrals.' },
    { id: 'contrast', name: 'High contrast', description: 'Maximum legibility: black, white and strong borders.' },
] as const satisfies ReadonlyArray<{ id: string; name: string; description: string }>

export type ThemeId = (typeof THEMES)[number]['id']

export const DEFAULT_THEME: ThemeId = 'lotar'

export function isThemeId(value: unknown): value is ThemeId {
    return typeof value === 'string' && THEMES.some((theme) => theme.id === value)
}
