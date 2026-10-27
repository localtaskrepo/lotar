import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import {
    checkThemes,
    colorStops,
    parseThemes,
    REQUIRED_TOKENS,
    resolveColor,
} from '../../scripts/check-theme-contrast.mjs'
import { THEMES } from '../utils/themes'

const __dirname = dirname(fileURLToPath(import.meta.url))
const themesCss = readFileSync(resolve(__dirname, '../themes.css'), 'utf8')
const stylesCss = readFileSync(resolve(__dirname, '../styles.css'), 'utf8')
const brandingRs = readFileSync(resolve(__dirname, '../../src/services/branding_service.rs'), 'utf8')

/** Ids in `UI_THEMES` (server-side validation of `ui.theme`). */
function rustThemeIds(): string[] {
    const list = brandingRs.match(/pub const UI_THEMES: &\[&str\] = &\[([\s\S]*?)\];/)
    expect(list, 'UI_THEMES in branding_service.rs').not.toBeNull()
    return [...(list?.[1] ?? '').matchAll(/"([^"]+)"/g)].map((m) => m[1] ?? '')
}

function declarationsOf(css: string, selector: string): Map<string, string> {
    const start = css.indexOf(`${selector} {`)
    expect(start, `styles.css block ${selector}`).toBeGreaterThanOrEqual(0)
    const body = css.slice(css.indexOf('{', start) + 1, css.indexOf('}', start))
    const tokens = new Map<string, string>()
    for (const declaration of body.replace(/\/\*[\s\S]*?\*\//g, '').split(';')) {
        const colon = declaration.indexOf(':')
        if (colon === -1) continue
        tokens.set(declaration.slice(0, colon).trim(), declaration.slice(colon + 1).trim())
    }
    return tokens
}

describe('colour themes (themes.css)', () => {
    const { themes, errors: parseErrors } = parseThemes(themesCss)

    it('parses without unexpected selectors or duplicate blocks', () => {
        expect(parseErrors).toEqual([])
    })

    it('defines a light and a dark block for every theme in THEMES, and nothing else', () => {
        const ids = THEMES.map((theme) => theme.id)
        expect(Object.keys(themes).sort()).toEqual([...ids].sort())
        for (const id of ids) {
            expect(themes[id]?.light, `${id} light`).toBeInstanceOf(Map)
            expect(themes[id]?.dark, `${id} dark`).toBeInstanceOf(Map)
        }
    })

    it('matches UI_THEMES in branding_service.rs, in the same order', () => {
        expect(rustThemeIds()).toEqual(THEMES.map((theme) => theme.id))
    })

    it('lists the classic themes first, then the playful ones', () => {
        const groups = THEMES.map((theme) => theme.group)
        expect(groups.every((group) => group === 'classic' || group === 'playful')).toBe(true)
        expect(groups.lastIndexOf('classic')).toBeLessThan(groups.indexOf('playful'))
    })

    it('passes the completeness and contrast checks', () => {
        const { errors } = checkThemes(themes)
        expect(errors).toEqual([])
    })

    it('keeps the lotar theme identical to the styles.css fallback palette', () => {
        const light = declarationsOf(stylesCss, ':where(:root)')
        const dark = declarationsOf(stylesCss, ':where(:root[data-mode="dark"])')
        const darkMedia = declarationsOf(stylesCss, ':where(:root:not([data-mode]))')
        expect(Object.fromEntries(themes.lotar?.light ?? [])).toEqual(Object.fromEntries(light))
        expect(Object.fromEntries(themes.lotar?.dark ?? [])).toEqual(Object.fromEntries(dark))
        expect(Object.fromEntries(darkMedia)).toEqual(Object.fromEntries(dark))
    })

    it('reports missing tokens, mismatched sets and contrast failures', () => {
        const css = `
            [data-theme-name="broken"] { color-scheme: light; --color-bg: #ffffff; --color-fg: #eeeeee; --color-extra: #000; --radius-md: 0; }
            [data-theme-name="broken"][data-mode="dark"] { color-scheme: dark; --color-bg: #000000; --color-fg: #ffffff; --text-glow: none; }
            [data-theme-name="lonely"] { color-scheme: light; }
            .stray { color: red; }
        `
        const parsed = parseThemes(css)
        expect(parsed.errors).toEqual(['unexpected selector: .stray'])
        const { errors } = checkThemes(parsed.themes)
        // Colour tokens must be in both blocks; character tokens may live in
        // the light (base) block only, and the dark block may only override.
        expect(errors).toContain('broken: only in light block: --color-extra')
        expect(errors).toContain('broken: only in dark block: --text-glow')
        expect(errors.some((e: string) => e.includes('--radius-md'))).toBe(false)
        expect(errors).toContain('lonely: missing dark block')
        expect(errors.some((e: string) => e.startsWith('broken light: missing') && e.includes('--color-accent'))).toBe(
            true,
        )
        expect(errors.some((e: string) => e.startsWith('broken light fg/bg:'))).toBe(true)
        expect(REQUIRED_TOKENS).toContain('--color-chart-8')
        expect(REQUIRED_TOKENS).toContain('--shadow-lg')
    })

    it('checks fg against every colour stop of patterned backgrounds, in both modes', () => {
        const base = REQUIRED_TOKENS.filter((t) => t !== 'color-scheme')
            .map((t) => `${t}: #808080;`)
            .join(' ')
        const css = `
            [data-theme-name="striped"] { ${base} color-scheme: light; --color-bg: #ffffff; --color-fg: #000000;
                --page-background: repeating-linear-gradient(90deg, #222222 0 1px, transparent 1px 8px), var(--color-bg); }
            [data-theme-name="striped"][data-mode="dark"] { ${base} color-scheme: dark; --color-bg: #000000; --color-fg: #ffffff; }
        `
        const { errors, notes } = checkThemes(parseThemes(css).themes)
        // The dark block inherits the light pattern and is checked with its own
        // palette: the dark stripe fails under black text, passes under white.
        expect(errors.some((e: string) => e.startsWith('striped light fg on --page-background'))).toBe(true)
        expect(errors.some((e: string) => e.startsWith('striped dark fg on --page-background'))).toBe(false)
        expect(notes.some((n: string) => n.includes('--page-background'))).toBe(false)
    })

    it('checks header text tokens against every header stop, defaulting to the page colours', () => {
        const base = REQUIRED_TOKENS.filter((t) => t !== 'color-scheme')
            .map((t) => `${t}: #000000;`)
            .join(' ')
        const css = `
            [data-theme-name="navy"] { ${base} color-scheme: light; --color-bg: #ffffff; --color-accent: #0000aa;
                --topbar-background: linear-gradient(90deg, #000080, #1660b0); --topbar-fg: #ffffff; }
            [data-theme-name="navy"][data-mode="dark"] { ${base} color-scheme: dark; --color-bg: #ffffff; --color-accent: #0000aa; }
        `
        const { rows, errors } = checkThemes(parseThemes(css).themes)
        // Light text passes on navy; muted and accent default to the (dark)
        // page colours and fail there, the accent also under its own tint.
        expect(errors.some((e: string) => e.startsWith('navy light topbar-fg'))).toBe(false)
        expect(errors.some((e: string) => e.startsWith('navy light topbar-muted'))).toBe(true)
        expect(errors.some((e: string) => e.startsWith('navy light topbar-accent'))).toBe(true)
        expect(rows.find((row) => row.mode === 'light')?.results['topbar-fg']?.ratio).toBeGreaterThan(4.5)
    })

    it('parses colour stops, var() references and color-mix()', () => {
        const tokens = new Map([
            ['--color-bg', '#ffffff'],
            ['--color-accent', '#ff0000'],
        ])
        expect(
            colorStops(
                'radial-gradient(circle, color-mix(in oklab, var(--color-accent) 10%, transparent) 0 3px, transparent 4px) 0 0 / 8px 8px, #123456, var(--color-bg)',
                tokens,
            ),
        ).toEqual([
            'color-mix(in oklab, var(--color-accent) 10%, transparent)',
            'transparent',
            '#123456',
            'var(--color-bg)',
        ])
        const mixed = resolveColor('color-mix(in oklab, var(--color-accent) 10%, transparent)', tokens)
        expect(mixed?.slice(0, 3).map(Math.round)).toEqual([255, 0, 0])
        expect(mixed?.[3]).toBeCloseTo(0.1)
        expect(resolveColor('color-mix(in srgb, #000000, #ffffff)', tokens)?.slice(0, 3)).toEqual([127.5, 127.5, 127.5])
        expect(resolveColor('hsl(0 0% 0%)', tokens)).toBeNull()
    })
})
