#!/usr/bin/env node
// Contrast and completeness check for the colour themes in view/themes.css
// (DEV-218). Every theme has a light block `[data-theme-name="<id>"]` and a
// dark block `[data-theme-name="<id>"][data-mode="dark"]`; both must define
// the same, complete token set so nothing leaks in from another theme or
// mode. Contrast uses WCAG 2 relative luminance. The default `lotar` theme
// reproduces the historical palette and is reported but never fails the run.
//
// Usage: node scripts/check-theme-contrast.mjs [path/to/themes.css]
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

/** Themes whose palette is frozen; failures are reported, not fatal. */
export const EXEMPT_THEMES = new Set(['lotar']);

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
];

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

/** Themes that must keep clearly visible borders: both border tokens ≥ 3:1. */
export const HIGH_CONTRAST_THEMES = new Set(['contrast']);
const HIGH_CONTRAST_BORDER_MIN = 3;

/**
 * Parse themes.css into `{ id: { light: Map, dark: Map } }` plus a list of
 * structural errors (unknown selectors, duplicate blocks).
 * @param {string} css
 */
export function parseThemes(css) {
    const source = css.replace(/\/\*[\s\S]*?\*\//g, '');
    /** @type {Record<string, { light?: Map<string, string>, dark?: Map<string, string> }>} */
    const themes = {};
    /** @type {string[]} */
    const errors = [];
    const blockPattern = /([^{}]+)\{([^{}]*)\}/g;
    let match;
    while ((match = blockPattern.exec(source)) !== null) {
        const selector = match[1].trim().replace(/\s+/g, ' ');
        const selectorMatch = selector.match(/^\[data-theme-name=(["'])([a-z0-9-]+)\1\](\[data-mode=(["'])dark\4\])?$/);
        if (!selectorMatch) {
            errors.push(`unexpected selector: ${selector}`);
            continue;
        }
        const id = selectorMatch[2];
        const mode = selectorMatch[3] ? 'dark' : 'light';
        const entry = (themes[id] ??= {});
        if (entry[mode]) {
            errors.push(`${id}: duplicate ${mode} block`);
            continue;
        }
        const tokens = new Map();
        for (const declaration of match[2].split(';')) {
            const colon = declaration.indexOf(':');
            if (colon === -1) continue;
            const name = declaration.slice(0, colon).trim();
            const value = declaration.slice(colon + 1).trim();
            if (name) tokens.set(name, value);
        }
        entry[mode] = tokens;
    }
    return { themes, errors };
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

/** @param {number[]} rgb */
function luminance([r, g, b]) {
    const lin = (c) => {
        const s = c / 255;
        return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
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
    const a = f[3];
    const composited = [0, 1, 2].map((i) => f[i] * a + b[i] * (1 - a));
    const l1 = luminance(composited);
    const l2 = luminance(b);
    return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
}

/**
 * Run every check. Returns table rows, hard errors (fail the run) and
 * notes (reported only).
 * @param {ReturnType<typeof parseThemes>['themes']} themes
 */
export function checkThemes(themes) {
    /** @type {{ theme: string, mode: string, results: Record<string, { ratio: number | null, min: number, pass: boolean }> }[]} */
    const rows = [];
    /** @type {string[]} */
    const errors = [];
    /** @type {string[]} */
    const notes = [];
    for (const [id, entry] of Object.entries(themes)) {
        const exempt = EXEMPT_THEMES.has(id);
        for (const mode of ['light', 'dark']) {
            if (!entry[mode]) errors.push(`${id}: missing ${mode} block`);
        }
        if (entry.light && entry.dark) {
            const lightKeys = [...entry.light.keys()].sort();
            const darkKeys = [...entry.dark.keys()].sort();
            const onlyLight = lightKeys.filter((k) => !entry.dark.has(k));
            const onlyDark = darkKeys.filter((k) => !entry.light.has(k));
            if (onlyLight.length) errors.push(`${id}: only in light block: ${onlyLight.join(', ')}`);
            if (onlyDark.length) errors.push(`${id}: only in dark block: ${onlyDark.join(', ')}`);
        }
        for (const mode of ['light', 'dark']) {
            const tokens = entry[mode];
            if (!tokens) continue;
            const missing = REQUIRED_TOKENS.filter((t) => !tokens.has(t));
            if (missing.length) errors.push(`${id} ${mode}: missing ${missing.join(', ')}`);
            const scheme = tokens.get('color-scheme');
            if (scheme && scheme !== mode) errors.push(`${id} ${mode}: color-scheme is "${scheme}"`);
            /** @type {Record<string, { ratio: number | null, min: number, pass: boolean }>} */
            const results = {};
            for (const [label, fgToken, bgToken, baseMin] of CHECKS) {
                const min =
                    label.startsWith('border') && HIGH_CONTRAST_THEMES.has(id) ? HIGH_CONTRAST_BORDER_MIN : baseMin;
                const ratio = contrast(tokens.get(fgToken) ?? '', tokens.get(bgToken) ?? '');
                if (ratio === null) {
                    notes.push(`${id} ${mode} ${label}: skipped (non-hex value)`);
                    results[label] = { ratio, min, pass: true };
                    continue;
                }
                const pass = ratio >= min;
                results[label] = { ratio, min, pass };
                if (!pass) {
                    const message = `${id} ${mode} ${label}: ${ratio.toFixed(2)} < ${min}`;
                    if (exempt) notes.push(`${message} (frozen default palette)`);
                    else errors.push(message);
                }
            }
            rows.push({ theme: id, mode, results });
        }
    }
    return { rows, errors, notes };
}

/** @param {ReturnType<typeof checkThemes>['rows']} rows */
export function formatTable(rows) {
    const labels = CHECKS.map(([label]) => label);
    const header = ['theme', 'mode', ...labels];
    const body = rows.map((row) => [
        row.theme,
        row.mode,
        ...labels.map((label) => {
            const r = row.results[label];
            if (!r || r.ratio === null) return 'skip';
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
        `\nMinimums: ${minimums}; border/bg is informational except in high-contrast themes, ` +
            `where both borders need ≥ ${HIGH_CONTRAST_BORDER_MIN}.`,
    );
    for (const note of notes) console.log(`note: ${note}`);
    const allErrors = [...parseErrors, ...errors];
    for (const error of allErrors) console.error(`error: ${error}`);
    if (allErrors.length) {
        console.error(`\n${allErrors.length} theme check(s) failed.`);
        process.exit(1);
    }
    console.log(`\nAll ${Object.keys(themes).length} themes pass.`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
    main();
}
