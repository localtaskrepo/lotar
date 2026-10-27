#!/usr/bin/env node
// Contrast and completeness check for the themes in view/themes.css
// (DEV-218 – DEV-220). A theme is a skin plus a colour palette:
//
//   [data-theme-name="<skin>"]                                   skin, light
//   [data-theme-name="<skin>"][data-mode="dark"]                 skin, dark
//   [data-theme-name="<skin>"][data-theme-palette="<p>"]         palette, light
//   [data-theme-name="<skin>"][data-theme-palette="<p>"][data-mode="dark"]
//
// The skin blocks hold the skin's character and its default palette.
// - Mode tokens (`color-scheme`, `--color-*`, `--shadow-*`) are not reset on
//   themed elements by styles.css, so both skin blocks must define the
//   complete set; otherwise a theme preview would inherit the page's colours.
// - Character tokens (fonts, radii, page/card/header backgrounds, control
//   bevels, glows) have defaults on every themed element in styles.css. The
//   skin's light block is its base (its selector also matches in dark mode)
//   and may set them once; the dark block may only override what the light
//   block sets, so the dark look is always "light block + dark overrides".
// - Palette blocks override the skin's blocks. A palette light block has the
//   same specificity as the skin's dark block and comes after it, so it wins
//   in dark mode too: it must follow the skin's dark block, and every mode
//   token it sets, and every token the skin's dark block sets, must be set
//   again in the palette's dark block.
//
// Every skin × palette × mode is checked on its effective tokens (the blocks
// that match, in cascade order). Contrast uses WCAG 2 relative luminance.
// Background tokens that may hold gradients or patterns are checked colour
// stop by colour stop (translucent stops are composited over --color-bg;
// stops the parser cannot evaluate are skipped with a note), and the header
// text tokens against every header stop. The default palette of `classic`
// reproduces the historical LoTaR palette and is reported but never fails.
//
// Usage: node scripts/check-theme-contrast.mjs [path/to/themes.css]
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

/** Skins whose default palette is frozen; failures are reported, not fatal. */
export const EXEMPT_THEMES = new Set(['classic']);

export const REQUIRED_TOKENS = [
    'color-scheme',
    '--color-bg',
    '--color-surface',
    '--color-surface-contrast',
    '--color-border',
    '--color-border-strong',
    '--color-fg',
    '--color-muted',
    '--color-accent',
    '--color-accent-strong',
    '--color-accent-contrast',
    '--color-danger',
    '--color-danger-contrast',
    '--color-success',
    '--color-warning',
    '--color-chart-4',
    '--color-chart-5',
    '--color-chart-6',
    '--color-chart-7',
    '--color-chart-8',
    '--shadow-xs',
    '--shadow-sm',
    '--shadow-md',
    '--shadow-lg',
];

/**
 * Tokens that depend on the mode and must appear in both blocks.
 * @param {string} name
 */
export function isModeToken(name) {
    return name === 'color-scheme' || name.startsWith('--color-') || name.startsWith('--shadow-');
}

/**
 * Contrast pairs: [label, foreground token, background token, minimum].
 * Primary buttons put accent-contrast text on accent and, on hover, on
 * accent-strong; danger buttons put danger-contrast text on danger.
 */
export const CHECKS = [
    ['fg/bg', '--color-fg', '--color-bg', 7],
    ['fg/surface', '--color-fg', '--color-surface', 7],
    ['muted/bg', '--color-muted', '--color-bg', 4.5],
    ['muted/surface', '--color-muted', '--color-surface', 4.5],
    ['accent-contrast/accent', '--color-accent-contrast', '--color-accent', 4.5],
    ['accent-contrast/accent-strong', '--color-accent-contrast', '--color-accent-strong', 4.5],
    ['accent-strong/bg', '--color-accent-strong', '--color-bg', 4.5],
    ['danger/bg', '--color-danger', '--color-bg', 3],
    ['danger-contrast/danger', '--color-danger-contrast', '--color-danger', 4.5],
    ['border/bg', '--color-border', '--color-bg', 1],
    ['border-strong/bg', '--color-border-strong', '--color-bg', 1.5],
];

/** Character tokens that paint behind body text; every colour stop is checked. */
export const BACKGROUND_TOKENS = ['--page-background', '--card-background', '--field-background'];
export const BACKGROUND_LABEL = 'fg/background stops';
const BACKGROUND_FG_MIN = 4.5;

/**
 * The sticky header has its own text tokens. Each is checked against every
 * colour stop of --topbar-background (composited over --color-bg); the
 * accent also against those stops under the active link's 18% accent tint.
 * Defaults mirror the `:where(:root, [data-theme-name])` block in styles.css.
 */
export const HEADER_DEFAULTS = {
    '--topbar-background': 'color-mix(in oklab, var(--color-bg) 88%, transparent)',
    '--topbar-fg': 'var(--color-fg)',
    '--topbar-muted': 'var(--color-muted)',
    '--topbar-accent': 'var(--color-accent-strong)',
};
export const HEADER_CHECKS = [
    ['topbar-fg', '--topbar-fg', 4.5],
    ['topbar-muted', '--topbar-muted', 4.5],
    ['topbar-accent', '--topbar-accent', 4.5],
];

/** Themes that must keep clearly visible borders: both border tokens ≥ 3:1. */
export const HIGH_CONTRAST_THEMES = new Set(['contrast']);
const HIGH_CONTRAST_BORDER_MIN = 3;

/**
 * @typedef {{ light?: Map<string, string>, dark?: Map<string, string>, order: { light?: number, dark?: number } }} BlockPair
 * @typedef {BlockPair & { palettes: Record<string, BlockPair> }} SkinBlocks
 */

/**
 * Parse themes.css into `{ skin: { light, dark, palettes: { id: { light,
 * dark } } } }` (token maps, plus each block's position in `order`) and a
 * list of structural errors (unknown selectors, duplicate blocks).
 * @param {string} css
 */
export function parseThemes(css) {
    const source = css.replace(/\/\*[\s\S]*?\*\//g, '');
    /** @type {Record<string, SkinBlocks>} */
    const themes = {};
    /** @type {string[]} */
    const errors = [];
    const blockPattern = /([^{}]+)\{([^{}]*)\}/g;
    let match;
    let index = 0;
    while ((match = blockPattern.exec(source)) !== null) {
        const selector = match[1].trim().replace(/\s+/g, ' ');
        const selectorMatch = selector.match(
            /^\[data-theme-name=(["'])([a-z0-9-]+)\1\](?:\[data-theme-palette=(["'])([a-z0-9-]+)\3\])?(\[data-mode=(["'])dark\6\])?$/,
        );
        if (!selectorMatch) {
            errors.push(`unexpected selector: ${selector}`);
            continue;
        }
        const id = selectorMatch[2];
        const palette = selectorMatch[4];
        const mode = selectorMatch[5] ? 'dark' : 'light';
        const skin = (themes[id] ??= { order: {}, palettes: {} });
        const entry = palette ? (skin.palettes[palette] ??= { order: {} }) : skin;
        const label = palette ? `${id}:${palette}` : id;
        if (entry[mode]) {
            errors.push(`${label}: duplicate ${mode} block`);
            continue;
        }
        const tokens = new Map();
        for (const declaration of splitTopLevel(match[2], ';')) {
            const colon = declaration.indexOf(':');
            if (colon === -1) continue;
            const name = declaration.slice(0, colon).trim();
            const value = declaration
                .slice(colon + 1)
                .trim()
                .replace(/\s+/g, ' ');
            if (name) tokens.set(name, value);
        }
        entry[mode] = tokens;
        entry.order[mode] = index++;
    }
    return { themes, errors };
}

/**
 * Split on a separator outside parentheses.
 * @param {string} text
 * @param {string} separator
 */
function splitTopLevel(text, separator) {
    const parts = [];
    let depth = 0;
    let start = 0;
    for (let i = 0; i < text.length; i++) {
        const c = text[i];
        if (c === '(') depth++;
        else if (c === ')') depth = Math.max(0, depth - 1);
        else if (c === separator && depth === 0) {
            parts.push(text.slice(start, i));
            start = i + 1;
        }
    }
    parts.push(text.slice(start));
    return parts;
}

/** Index of the parenthesis closing the one at `open`, or -1. */
function closingParen(text, open) {
    let depth = 0;
    for (let i = open; i < text.length; i++) {
        if (text[i] === '(') depth++;
        else if (text[i] === ')' && --depth === 0) return i;
    }
    return -1;
}

/**
 * Parse a hex colour (#rgb, #rgba, #rrggbb, #rrggbbaa) into [r, g, b, a]
 * with channels 0-255 and alpha 0-1, or null when the value is not hex.
 * @param {string | undefined} value
 */
export function parseHex(value) {
    const match = value?.trim().match(/^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i);
    if (!match) return null;
    let hex = match[1];
    if (hex.length <= 4) hex = [...hex].map((c) => c + c).join('');
    const channels = [0, 2, 4, 6].filter((i) => i < hex.length).map((i) => parseInt(hex.slice(i, i + 2), 16));
    return [channels[0], channels[1], channels[2], channels.length === 4 ? channels[3] / 255 : 1];
}

const NAMED_COLORS = {
    transparent: [0, 0, 0, 0],
    black: [0, 0, 0, 1],
    white: [255, 255, 255, 1],
};

/** sRGB channel (0-255) → linear 0-1. */
const toLinear = (c) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
};
/** linear 0-1 → sRGB channel 0-255 (clamped). */
const fromLinear = (l) => {
    const v = l <= 0.0031308 ? l * 12.92 : 1.055 * Math.max(l, 0) ** (1 / 2.4) - 0.055;
    return Math.min(255, Math.max(0, v * 255));
};

function rgbToOklab([r, g, b]) {
    const [lr, lg, lb] = [r, g, b].map(toLinear);
    const l = Math.cbrt(0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb);
    const m = Math.cbrt(0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb);
    const s = Math.cbrt(0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb);
    return [
        0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
    ];
}

function oklabToRgb([L, a, b]) {
    const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
    const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
    const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
    return [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
    ].map(fromLinear);
}

/**
 * `color-mix(in srgb|oklab, A [p%], B [q%])` with premultiplied alpha, as
 * browsers compute it. Returns null for other colour spaces.
 */
function colorMix(args, tokens, depth) {
    const parts = splitTopLevel(args, ',').map((p) => p.trim());
    if (parts.length !== 3) return null;
    const space = parts[0].match(/^in\s+(srgb|oklab)$/i)?.[1].toLowerCase();
    if (!space) return null;
    const items = parts.slice(1).map((part) => {
        const pct = part.match(/\s(\d+(?:\.\d+)?)%$/);
        return { color: pct ? part.slice(0, pct.index).trim() : part, pct: pct ? Number(pct[1]) : null };
    });
    let [p1, p2] = items.map((item) => item.pct);
    if (p1 === null && p2 === null) [p1, p2] = [50, 50];
    else if (p1 === null) p1 = 100 - p2;
    else if (p2 === null) p2 = 100 - p1;
    const sum = p1 + p2;
    if (sum <= 0) return null;
    const c1 = resolveColor(items[0].color, tokens, depth + 1);
    const c2 = resolveColor(items[1].color, tokens, depth + 1);
    if (!c1 || !c2) return null;
    const [w1, w2] = [p1 / sum, p2 / sum];
    const convert = space === 'oklab' ? rgbToOklab : (c) => c.slice(0, 3);
    const back = space === 'oklab' ? oklabToRgb : (c) => c;
    const alpha = c1[3] * w1 + c2[3] * w2;
    const scale = Math.min(1, sum / 100);
    if (alpha === 0) return [0, 0, 0, 0];
    const v1 = convert(c1);
    const v2 = convert(c2);
    const mixed = [0, 1, 2].map((i) => (v1[i] * c1[3] * w1 + v2[i] * c2[3] * w2) / alpha);
    return [...back(mixed), alpha * scale];
}

/**
 * Evaluate a CSS colour expression to [r, g, b, a], resolving `var()`
 * against the theme's tokens. Supports hex, rgb()/rgba(), a few named
 * colours and color-mix() in srgb/oklab; anything else returns null.
 * @param {string} expression
 * @param {Map<string, string>} tokens
 */
export function resolveColor(expression, tokens, depth = 0) {
    if (depth > 8) return null;
    const value = expression.trim();
    const hex = parseHex(value);
    if (hex) return hex;
    const named = NAMED_COLORS[value.toLowerCase()];
    if (named) return [...named];
    const fn = value.match(/^([a-z-]+)\(([\s\S]*)\)$/i);
    if (!fn) return null;
    const [, name, args] = fn;
    switch (name.toLowerCase()) {
        case 'var': {
            const [ref, fallback] = splitTopLevel(args, ',').map((p) => p.trim());
            const resolved = tokens.get(ref);
            if (resolved !== undefined) return resolveColor(resolved, tokens, depth + 1);
            return fallback ? resolveColor(fallback, tokens, depth + 1) : null;
        }
        case 'rgb':
        case 'rgba': {
            const nums = args
                .split(/[\s,/]+/)
                .filter(Boolean)
                .map((n) => (n.endsWith('%') ? Number(n.slice(0, -1)) / 100 : Number(n)));
            if (nums.length < 3 || nums.some(Number.isNaN)) return null;
            return [nums[0], nums[1], nums[2], nums[3] ?? 1];
        }
        case 'color-mix':
            return colorMix(args, tokens, depth);
        default:
            return null;
    }
}

/**
 * Pull colour expressions out of a `background` value: hex, named colours
 * and colour functions; `var()` references that are not colours are expanded
 * and scanned in turn. Gradient keywords, lengths and positions are ignored.
 * @param {string} value
 * @param {Map<string, string>} tokens
 * @returns {string[]}
 */
export function colorStops(value, tokens, depth = 0) {
    /** @type {string[]} */
    const stops = [];
    if (depth > 8) return stops;
    let i = 0;
    while (i < value.length) {
        const rest = value.slice(i);
        const fn = rest.match(/^(color-mix|rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|var)\(/i);
        if (fn) {
            const end = closingParen(value, i + fn[1].length);
            if (end === -1) break;
            const expression = value.slice(i, end + 1);
            if (fn[1].toLowerCase() === 'var' && !resolveColor(expression, tokens)) {
                const ref = splitTopLevel(expression.slice(4, -1), ',')[0].trim();
                const resolved = tokens.get(ref);
                if (resolved !== undefined) stops.push(...colorStops(resolved, tokens, depth + 1));
                else stops.push(expression);
            } else {
                stops.push(expression);
            }
            i = end + 1;
            continue;
        }
        const hex = rest.match(/^#[0-9a-f]+\b/i);
        if (hex) {
            stops.push(hex[0]);
            i += hex[0].length;
            continue;
        }
        const word = rest.match(/^[a-z][a-z-]*/i);
        if (word) {
            if (NAMED_COLORS[word[0].toLowerCase()]) stops.push(word[0]);
            i += word[0].length;
            continue;
        }
        i++;
    }
    return stops;
}

/** @param {number[]} rgb */
function luminance([r, g, b]) {
    return 0.2126 * toLinear(r) + 0.7152 * toLinear(g) + 0.0722 * toLinear(b);
}

/**
 * WCAG contrast ratio between two parsed colours. A translucent foreground
 * is composited over the (opaque) background first.
 * @param {number[]} f
 * @param {number[]} b
 */
function contrastRgba(f, b) {
    const a = f[3];
    const composited = [0, 1, 2].map((i) => f[i] * a + b[i] * (1 - a));
    const l1 = luminance(composited);
    const l2 = luminance(b);
    return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
}

/**
 * WCAG contrast ratio between two hex colours. A translucent foreground is
 * composited over the (opaque) background first.
 * @param {string} fg
 * @param {string} bg
 */
export function contrast(fg, bg) {
    const f = parseHex(fg);
    const b = parseHex(bg);
    if (!f || !b) return null;
    return contrastRgba(f, b);
}

/** Composite a possibly translucent colour over an opaque one. */
function over(top, base) {
    const a = top[3];
    return [0, 1, 2].map((i) => top[i] * a + base[i] * (1 - a)).concat(1);
}

/**
 * Lowest fg contrast across every colour stop of the background tokens.
 * Fully transparent stops show the layer below and are not counted.
 * @param {Map<string, string>} tokens effective tokens for the mode
 */
export function backgroundStopContrast(tokens) {
    const fg = resolveColor(tokens.get('--color-fg') ?? '', tokens);
    const bg = resolveColor(tokens.get('--color-bg') ?? '', tokens);
    /** @type {{ token: string, stop: string, ratio: number }[]} */
    const results = [];
    /** @type {string[]} */
    const skipped = [];
    if (!fg || !bg) return { results, skipped };
    for (const token of BACKGROUND_TOKENS) {
        const value = tokens.get(token);
        if (value === undefined) continue;
        for (const stop of colorStops(value, tokens)) {
            const color = resolveColor(stop, tokens);
            if (!color) {
                skipped.push(`${token} ${stop}`);
                continue;
            }
            if (color[3] === 0) continue;
            results.push({ token, stop, ratio: contrastRgba(fg, over(color, over(bg, [255, 255, 255, 1]))) });
        }
    }
    return { results, skipped };
}

/**
 * Lowest contrast of each header text token over the header's colour stops.
 * @param {Map<string, string>} themeTokens effective tokens for the mode
 */
export function headerContrast(themeTokens) {
    const tokens = new Map([...Object.entries(HEADER_DEFAULTS), ...themeTokens]);
    const bg = resolveColor(tokens.get('--color-bg') ?? '', tokens);
    /** @type {Record<string, { ratio: number | null, stop: string | null }>} */
    const results = {};
    /** @type {string[]} */
    const skipped = [];
    const stops = [];
    if (bg) {
        const opaqueBg = over(bg, [255, 255, 255, 1]);
        for (const stop of colorStops(tokens.get('--topbar-background') ?? '', tokens)) {
            const color = resolveColor(stop, tokens);
            if (!color) skipped.push(`--topbar-background ${stop}`);
            else if (color[3] > 0) stops.push({ stop, color: over(color, opaqueBg) });
        }
    }
    for (const [label, token] of HEADER_CHECKS) {
        const fg = resolveColor(tokens.get(token) ?? '', tokens);
        results[label] = { ratio: null, stop: null };
        if (!fg) {
            skipped.push(`${token} ${tokens.get(token)}`);
            continue;
        }
        for (const { stop, color } of stops) {
            const backgrounds = [{ stop, color }];
            if (token === '--topbar-accent') {
                backgrounds.push({ stop: `${stop} + 18% accent tint`, color: over([...fg.slice(0, 3), 0.18], color) });
            }
            for (const candidate of backgrounds) {
                const ratio = contrastRgba(fg, candidate.color);
                const current = results[label];
                if (current.ratio === null || ratio < current.ratio) results[label] = { ratio, stop: candidate.stop };
            }
        }
    }
    return { results, skipped };
}

/**
 * Contrast checks for one skin × palette × mode on its effective tokens.
 * @param {Map<string, string>} tokens
 * @param {{ highContrast: boolean, label: string, mode: string, fail: (message: string) => void, notes: string[] }} context
 */
function checkVariant(tokens, { highContrast, label: id, mode, fail, notes }) {
    /** @type {Record<string, { ratio: number | null, min: number, pass: boolean }>} */
    const results = {};
    for (const [label, fgToken, bgToken, baseMin] of CHECKS) {
        const min = label.startsWith('border') && highContrast ? HIGH_CONTRAST_BORDER_MIN : baseMin;
        const ratio = contrast(tokens.get(fgToken) ?? '', tokens.get(bgToken) ?? '');
        if (ratio === null) {
            notes.push(`${id} ${mode} ${label}: skipped (non-hex value)`);
            results[label] = { ratio, min, pass: true };
            continue;
        }
        const pass = ratio >= min;
        results[label] = { ratio, min, pass };
        if (!pass) fail(`${id} ${mode} ${label}: ${ratio.toFixed(2)} < ${min}`);
    }
    const stops = backgroundStopContrast(tokens);
    for (const skip of new Set(stops.skipped)) notes.push(`${id} ${mode} ${skip}: skipped (unsupported colour)`);
    let lowest = null;
    for (const { token, stop, ratio } of stops.results) {
        lowest = lowest === null ? ratio : Math.min(lowest, ratio);
        if (ratio < BACKGROUND_FG_MIN) {
            fail(`${id} ${mode} fg on ${token} stop ${stop}: ${ratio.toFixed(2)} < ${BACKGROUND_FG_MIN}`);
        }
    }
    results[BACKGROUND_LABEL] = {
        ratio: lowest,
        min: BACKGROUND_FG_MIN,
        pass: lowest === null || lowest >= BACKGROUND_FG_MIN,
    };
    const header = headerContrast(tokens);
    for (const skip of new Set(header.skipped)) notes.push(`${id} ${mode} ${skip}: skipped (unsupported colour)`);
    for (const [label, , min] of HEADER_CHECKS) {
        const { ratio, stop } = header.results[label];
        const pass = ratio === null || ratio >= min;
        results[label] = { ratio, min, pass };
        if (!pass) fail(`${id} ${mode} ${label} on --topbar-background stop ${stop}: ${ratio.toFixed(2)} < ${min}`);
    }
    return results;
}

/** Later maps win, like later matching rules of equal or higher specificity. */
function cascade(...maps) {
    return new Map(maps.filter(Boolean).flatMap((map) => [...map]));
}

/**
 * Run every check. Returns table rows (one per skin × palette × mode), hard
 * errors (fail the run) and notes (reported only).
 * @param {ReturnType<typeof parseThemes>['themes']} themes
 */
export function checkThemes(themes) {
    /** @type {{ theme: string, mode: string, results: Record<string, { ratio: number | null, min: number, pass: boolean }> }[]} */
    const rows = [];
    /** @type {string[]} */
    const errors = [];
    /** @type {string[]} */
    const notes = [];
    for (const [id, skin] of Object.entries(themes)) {
        for (const mode of ['light', 'dark']) {
            if (!skin[mode]) errors.push(`${id}: missing ${mode} block`);
        }
        if (skin.light && skin.dark) {
            const onlyLight = [...skin.light.keys()].filter((k) => isModeToken(k) && !skin.dark.has(k)).sort();
            const onlyDark = [...skin.dark.keys()].filter((k) => !skin.light.has(k)).sort();
            if (onlyLight.length) errors.push(`${id}: only in light block: ${onlyLight.join(', ')}`);
            if (onlyDark.length) errors.push(`${id}: only in dark block: ${onlyDark.join(', ')}`);
        }
        for (const mode of ['light', 'dark']) {
            const own = skin[mode];
            if (!own) continue;
            const missing = REQUIRED_TOKENS.filter((t) => !own.has(t));
            if (missing.length) errors.push(`${id} ${mode}: missing ${missing.join(', ')}`);
            const scheme = own.get('color-scheme');
            if (scheme && scheme !== mode) errors.push(`${id} ${mode}: color-scheme is "${scheme}"`);
        }
        /** @type {{ label: string, exempt: boolean, light?: Map<string, string>, dark?: Map<string, string> }[]} */
        const variants = [
            {
                label: id,
                exempt: EXEMPT_THEMES.has(id),
                light: skin.light,
                dark: skin.dark && cascade(skin.light, skin.dark),
            },
        ];
        for (const [paletteId, palette] of Object.entries(skin.palettes ?? {})) {
            const label = `${id}:${paletteId}`;
            for (const mode of ['light', 'dark']) {
                if (!palette[mode]) errors.push(`${label}: missing ${mode} block`);
                const scheme = palette[mode]?.get('color-scheme');
                if (scheme && scheme !== mode) errors.push(`${label} ${mode}: color-scheme is "${scheme}"`);
            }
            if (palette.light && palette.dark) {
                const needDark = [...palette.light.keys()]
                    .filter((k) => (isModeToken(k) || skin.dark?.has(k)) && !palette.dark.has(k))
                    .sort();
                if (needDark.length) {
                    errors.push(
                        `${label}: only in light block (would also apply in dark mode): ${needDark.join(', ')}`,
                    );
                }
            }
            if (palette.order.light !== undefined && (skin.order.dark ?? Infinity) > palette.order.light) {
                errors.push(`${label}: palette blocks must come after the ${id} dark block`);
            }
            variants.push({
                label,
                exempt: false,
                light: palette.light && cascade(skin.light, palette.light),
                dark: palette.dark && cascade(skin.light, skin.dark, palette.light, palette.dark),
            });
        }
        for (const variant of variants) {
            const fail = (message) => {
                if (variant.exempt) notes.push(`${message} (frozen default palette)`);
                else errors.push(message);
            };
            for (const mode of ['light', 'dark']) {
                const tokens = variant[mode];
                if (!tokens) continue;
                const results = checkVariant(tokens, {
                    highContrast: HIGH_CONTRAST_THEMES.has(id),
                    label: variant.label,
                    mode,
                    fail,
                    notes,
                });
                rows.push({ theme: variant.label, mode, results });
            }
        }
    }
    return { rows, errors, notes };
}

/** @param {ReturnType<typeof checkThemes>['rows']} rows */
export function formatTable(rows) {
    const labels = [...CHECKS.map(([label]) => label), BACKGROUND_LABEL, ...HEADER_CHECKS.map(([label]) => label)];
    const header = ['theme', 'mode', ...labels];
    const body = rows.map((row) => [
        row.theme,
        row.mode,
        ...labels.map((label) => {
            const r = row.results[label];
            if (!r || r.ratio === null) return label === BACKGROUND_LABEL ? '-' : 'skip';
            return `${r.ratio.toFixed(2)}${r.pass ? '' : ' ✗'}`;
        }),
    ]);
    const widths = header.map((h, i) => Math.max(h.length, ...body.map((cells) => cells[i].length)));
    const line = (cells) => cells.map((c, i) => c.padEnd(widths[i])).join('  ');
    return [line(header), line(widths.map((w) => '-'.repeat(w))), ...body.map(line)].join('\n');
}

function main() {
    const file = resolve(process.argv[2] ?? fileURLToPath(new URL('../view/themes.css', import.meta.url)));
    const { themes, errors: parseErrors } = parseThemes(readFileSync(file, 'utf8'));
    const { rows, errors, notes } = checkThemes(themes);
    console.log(formatTable(rows));
    const minimums = CHECKS.filter(([, , , min]) => min > 1)
        .map(([label, , , min]) => `${label} ≥ ${min}`)
        .join(', ');
    console.log(
        `\nMinimums: ${minimums}, ${BACKGROUND_LABEL} ≥ ${BACKGROUND_FG_MIN} (lowest fg contrast over every ` +
            `colour stop of ${BACKGROUND_TOKENS.join(', ')}; "-" = none set); border/bg is informational except ` +
            `in high-contrast themes, where both borders need ≥ ${HIGH_CONTRAST_BORDER_MIN}. ` +
            `${HEADER_CHECKS.map(([label, , min]) => `${label} ≥ ${min}`).join(', ')}: lowest contrast over every ` +
            `colour stop of --topbar-background (accent also under the active link's 18% tint).`,
    );
    for (const note of notes) console.log(`note: ${note}`);
    const allErrors = [...parseErrors, ...errors];
    for (const error of allErrors) console.error(`error: ${error}`);
    if (allErrors.length) {
        console.error(`\n${allErrors.length} theme check(s) failed.`);
        process.exit(1);
    }
    console.log(`\nAll ${rows.length / 2} skin × palette combinations pass in light and dark.`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
    main();
}
