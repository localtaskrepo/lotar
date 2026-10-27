import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import { checkThemes, parseThemes, REQUIRED_TOKENS } from '../../scripts/check-theme-contrast.mjs'
import { THEMES } from '../utils/themes'

const __dirname = dirname(fileURLToPath(import.meta.url))
const themesCss = readFileSync(resolve(__dirname, '../themes.css'), 'utf8')
const stylesCss = readFileSync(resolve(__dirname, '../styles.css'), 'utf8')

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
            [data-theme-name="broken"] { color-scheme: light; --color-bg: #ffffff; --color-fg: #eeeeee; --extra: 1px; }
            [data-theme-name="broken"][data-mode="dark"] { color-scheme: dark; --color-bg: #000000; --color-fg: #ffffff; }
            [data-theme-name="lonely"] { color-scheme: light; }
            .stray { color: red; }
        `
        const parsed = parseThemes(css)
        expect(parsed.errors).toEqual(['unexpected selector: .stray'])
        const { errors } = checkThemes(parsed.themes)
        expect(errors).toContain('broken: only in light block: --extra')
        expect(errors).toContain('lonely: missing dark block')
        expect(errors.some((e: string) => e.startsWith('broken light: missing') && e.includes('--color-accent'))).toBe(
            true,
        )
        expect(errors.some((e: string) => e.startsWith('broken light fg/bg:'))).toBe(true)
        expect(REQUIRED_TOKENS).toContain('--color-chart-8')
    })
})
