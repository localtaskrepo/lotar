#!/usr/bin/env node
// Capture a small, cropped screenshot of a UI element and attach it to a ticket,
// so the task panel shows what a UI change looks like (DEV-224).
//
// Usage:
//   node scripts/ticket-screenshot.mjs --ticket DEV-123 --url http://localhost:8080/boards \
//       --selector '.app-page-header' [--name board-header] [--width 1280] [--theme dark]
//
// Options:
//   --ticket ID      ticket to attach to (omit with --no-attach)
//   --url URL        page of a running `lotar serve` (use scenario data, never the DEV backlog)
//   --selector CSS   element to crop to (required; snippets, not full pages)
//   --name NAME      file name without extension (default: derived from the selector)
//   --width N        viewport width (default 1280; use 390 for phone layouts)
//   --height N       viewport height (default 900)
//   --theme T        light | dark (default light)
//   --scale N        device scale factor (default 1; 2 for crisp text, saved as <name>-2x)
//   --pad N          pixels around the element (default 8)
//   --wait-for CSS   wait for this element before capturing (default: the selector)
//   --setup FILE     module whose default export `async ({ page }) => {}` prepares the page
//   --quality Q      WebP quality 0-1 (default 0.8)
//   --out DIR        also keep the image here (default: temporary, removed after attaching)
//   --tasks-dir DIR  tasks directory passed to lotar (default .tasks)
//   --lotar BIN      lotar binary (default target/debug/lotar, then lotar on PATH)
//   --no-attach      only write the image (requires --out)

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// Snippets above this size are probably too big a crop; they still attach.
const SIZE_WARNING_BYTES = 150 * 1024;

const DEFAULTS = {
    width: 1280,
    height: 900,
    theme: 'light',
    scale: 1,
    pad: 8,
    quality: 0.8,
    tasksDir: '.tasks',
    attach: true,
};

/** Parse argv into options; throws with a readable message on bad input. */
export function parseArgs(argv) {
    const opts = { ...DEFAULTS };
    const take = (flag, value) => {
        if (value === undefined || value.startsWith('--')) throw new Error(`${flag} needs a value`);
        return value;
    };
    const number = (flag, value, min, max) => {
        const parsed = Number(take(flag, value));
        if (!Number.isFinite(parsed) || parsed < min || parsed > max) {
            throw new Error(`${flag} must be a number from ${min} to ${max}`);
        }
        return parsed;
    };
    for (let i = 0; i < argv.length; i += 1) {
        const flag = argv[i];
        const value = argv[i + 1];
        switch (flag) {
            case '--ticket':
                opts.ticket = take(flag, value);
                i += 1;
                break;
            case '--url':
                opts.url = take(flag, value);
                i += 1;
                break;
            case '--selector':
                opts.selector = take(flag, value);
                i += 1;
                break;
            case '--name':
                opts.name = take(flag, value);
                i += 1;
                break;
            case '--width':
                opts.width = number(flag, value, 200, 4000);
                i += 1;
                break;
            case '--height':
                opts.height = number(flag, value, 200, 4000);
                i += 1;
                break;
            case '--theme':
                opts.theme = take(flag, value);
                if (!['light', 'dark'].includes(opts.theme)) throw new Error('--theme must be light or dark');
                i += 1;
                break;
            case '--scale':
                opts.scale = number(flag, value, 1, 3);
                i += 1;
                break;
            case '--pad':
                opts.pad = number(flag, value, 0, 200);
                i += 1;
                break;
            case '--quality':
                opts.quality = number(flag, value, 0.1, 1);
                i += 1;
                break;
            case '--wait-for':
                opts.waitFor = take(flag, value);
                i += 1;
                break;
            case '--setup':
                opts.setup = take(flag, value);
                i += 1;
                break;
            case '--out':
                opts.out = take(flag, value);
                i += 1;
                break;
            case '--tasks-dir':
                opts.tasksDir = take(flag, value);
                i += 1;
                break;
            case '--lotar':
                opts.lotar = take(flag, value);
                i += 1;
                break;
            case '--no-attach':
                opts.attach = false;
                break;
            case '--help':
            case '-h':
                opts.help = true;
                break;
            default:
                throw new Error(`Unknown option ${flag}`);
        }
    }
    if (opts.help) return opts;
    if (!opts.url) throw new Error('--url is required');
    if (!opts.selector) throw new Error('--selector is required: attach a cropped snippet, not a full page');
    if (opts.attach && !opts.ticket) throw new Error('--ticket is required (or pass --no-attach with --out)');
    if (!opts.attach && !opts.out) throw new Error('--no-attach needs --out, or the image is lost');
    // Scale-2 captures are marked `-2x` so pages can show them at half size.
    const scaleSuffix = Number.isInteger(opts.scale) && opts.scale > 1 ? `-${opts.scale}x` : '';
    opts.name = `${fileName(opts.name ?? opts.selector)}${scaleSuffix}`;
    return opts;
}

/** A safe file stem: letters, digits, dots, dashes; never empty. */
export function fileName(raw) {
    const stem = String(raw)
        .toLowerCase()
        .replace(/[^a-z0-9.-]+/g, '-')
        .replace(/^[-.]+|[-.]+$/g, '')
        .slice(0, 60);
    return stem || 'screenshot';
}

/** Grow `box` by `pad` on every side, kept inside the page. */
export function paddedClip(box, pad, page) {
    const x = Math.max(0, Math.floor(box.x - pad));
    const y = Math.max(0, Math.floor(box.y - pad));
    const right = Math.min(page.width, Math.ceil(box.x + box.width + pad));
    const bottom = Math.min(page.height, Math.ceil(box.y + box.height + pad));
    return { x, y, width: Math.max(1, right - x), height: Math.max(1, bottom - y) };
}

function lotarBinary(opts) {
    if (opts.lotar) return opts.lotar;
    const local = path.join(ROOT, 'target', 'debug', process.platform === 'win32' ? 'lotar.exe' : 'lotar');
    return existsSync(local) ? local : 'lotar';
}

async function capture(opts) {
    const { chromium } = await import('@playwright/test');
    const browser = await chromium.launch();
    try {
        const context = await browser.newContext({
            viewport: { width: opts.width, height: opts.height },
            deviceScaleFactor: opts.scale,
            colorScheme: opts.theme,
            locale: 'en-US',
        });
        const page = await context.newPage();
        await page.goto(opts.url, { waitUntil: 'domcontentloaded' });
        if (opts.setup) {
            const setup = await import(pathToFileURL(path.resolve(opts.setup)).href);
            await setup.default({ page });
        }
        const target = page.locator(opts.waitFor ?? opts.selector).first();
        await target.waitFor({ state: 'visible', timeout: 15_000 });
        await page.evaluate(() => document.fonts?.ready);
        // Let transitions and lazy images finish.
        await page.waitForTimeout(400);

        const element = page.locator(opts.selector).first();
        await element.scrollIntoViewIfNeeded();
        const box = await element.boundingBox();
        if (!box) throw new Error(`${opts.selector} has no visible box`);
        const scroll = await page.evaluate(() => ({
            x: window.scrollX,
            y: window.scrollY,
            width: document.documentElement.scrollWidth,
            height: document.documentElement.scrollHeight,
        }));
        const clip = paddedClip({ ...box, x: box.x + scroll.x, y: box.y + scroll.y }, opts.pad, scroll);
        const png = await page.screenshot({ clip, fullPage: true });

        const dataUrl = await page.evaluate(
            async ({ b64, quality }) => {
                const img = new Image();
                img.src = `data:image/png;base64,${b64}`;
                await img.decode();
                const canvas = document.createElement('canvas');
                canvas.width = img.naturalWidth;
                canvas.height = img.naturalHeight;
                canvas.getContext('2d').drawImage(img, 0, 0);
                return canvas.toDataURL('image/webp', quality);
            },
            { b64: png.toString('base64'), quality: opts.quality },
        );
        return Buffer.from(dataUrl.slice(dataUrl.indexOf(',') + 1), 'base64');
    } finally {
        await browser.close();
    }
}

async function main() {
    let opts;
    try {
        opts = parseArgs(process.argv.slice(2));
    } catch (error) {
        console.error(`ticket-screenshot: ${error.message}`);
        process.exit(2);
    }
    if (opts.help) {
        console.log('See the usage notes at the top of scripts/ticket-screenshot.mjs.');
        return;
    }

    const image = await capture(opts);
    const dir = opts.out ? path.resolve(opts.out) : mkdtempSync(path.join(os.tmpdir(), 'lotar-shot-'));
    mkdirSync(dir, { recursive: true });
    const file = path.join(dir, `${opts.name}.webp`);
    writeFileSync(file, image);
    const size = `${Math.round(image.length / 1024)} KB`;
    console.log(`Captured ${opts.selector} at ${opts.width}px (${opts.theme}): ${file} (${size})`);
    if (image.length > SIZE_WARNING_BYTES) {
        console.warn(`ticket-screenshot: ${size} is large for a snippet; crop to a smaller element.`);
    }

    try {
        if (!opts.attach) return;
        const result = spawnSync(
            lotarBinary(opts),
            ['--tasks-dir', opts.tasksDir, 'task', 'reference', 'add', 'upload', opts.ticket, file],
            { stdio: 'inherit' },
        );
        if (result.error) throw result.error;
        if (result.status !== 0) process.exitCode = result.status ?? 1;
    } finally {
        if (!opts.out) rmSync(dir, { recursive: true, force: true });
    }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
    main().catch((error) => {
        console.error(`ticket-screenshot: ${error.message}`);
        process.exit(1);
    });
}
