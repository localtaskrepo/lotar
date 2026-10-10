#!/usr/bin/env node
// Regenerate the README/docs screenshots, the hero recording, the terminal recording,
// and the social preview image from a freshly seeded demo workspace.
//
// Usage:
//   npm run screenshots                          # build lotar + UI, then capture everything
//   node scripts/screenshots.mjs                 # reuse an existing binary (target/smoke or target/release)
//   node scripts/screenshots.mjs --only board,agents --skip-hero --skip-terminal
//   node scripts/screenshots.mjs --keep          # keep the demo workspace for inspection
//   node scripts/screenshots.mjs --no-git        # refresh UI only; preserve Git-dependent media
//
// Output (docs/assets/screenshots/ by default):
//   <shot>-light.webp / <shot>-dark.webp   1440x900 viewport at 2x, both themes
//   hero-agent-loop.webp                   animated agent loop (needs ffmpeg + img2webp, else GIF via ffmpeg)
//   terminal.gif                           CLI tour recorded with vhs (skipped when vhs is missing)
//   ../social-preview.png                  1280x640 GitHub social preview
//
// The agent in the demo is a deterministic stub (scripts/demo/stub-agent.mjs) that speaks
// the Claude Code stream-json protocol; no model is called.
// Set PLAYWRIGHT_BROWSERS_PATH if your Chromium lives outside Playwright's default cache.

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';
import { demoEnvironment, removeDemoWorkspace, seedDemoWorkspace } from './demo/workspace.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const BIN_NAME = process.platform === 'win32' ? 'lotar.exe' : 'lotar';
const VIEWPORT = { width: 1440, height: 900 };
const SCALE = 2;
const WEBP_QUALITY = 0.82;
const THEMES = ['light', 'dark'];
// Stable rendering of dates and times regardless of the host machine.
const PAGE_DEFAULTS = { locale: 'en-US', timezoneId: 'UTC' };

function parseArgs(argv) {
    const opts = { out: path.join(ROOT, 'docs', 'assets', 'screenshots'), only: null, hero: true, terminal: true, social: true, keep: false, gitHistory: true };
    for (let i = 0; i < argv.length; i += 1) {
        const a = argv[i];
        if (a === '--out') opts.out = path.resolve(argv[++i]);
        else if (a === '--only') opts.only = new Set(argv[++i].split(',').map((s) => s.trim()).filter(Boolean));
        else if (a === '--skip-hero') opts.hero = false;
        else if (a === '--skip-terminal') opts.terminal = false;
        else if (a === '--skip-social') opts.social = false;
        else if (a === '--keep') opts.keep = true;
        else if (a === '--no-git') opts.gitHistory = false;
        else if (a === '-h' || a === '--help') {
            console.log(readFileSync(fileURLToPath(import.meta.url), 'utf8').split('\n').slice(1, 20).map((l) => l.replace(/^\/\/ ?/, '')).join('\n'));
            process.exit(0);
        } else {
            throw new Error(`Unknown argument: ${a}`);
        }
    }
    return opts;
}

function resolveBinary() {
    const explicit = process.env.LOTAR_BINARY_PATH || process.env.LOTAR_BIN;
    if (explicit) return explicit;
    for (const candidate of [path.join(ROOT, 'target', 'smoke', BIN_NAME), path.join(ROOT, 'target', 'release', BIN_NAME)]) {
        if (existsSync(candidate)) return candidate;
    }
    throw new Error('No lotar binary found. Run `npm run build:smoke` (or use `npm run screenshots`, which builds first) or set LOTAR_BINARY_PATH.');
}

function hasTool(name) {
    const probe = spawnSync(name, ['-version'], { stdio: 'ignore' });
    if (!probe.error) return true;
    return !spawnSync(name, ['--version'], { stdio: 'ignore' }).error;
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

async function startServer(bin, dir) {
    const child = spawn(bin, ['serve', '--port', '0', '--host', '127.0.0.1'], {
        cwd: dir,
        // The browser user is "priya", so actors and "(you)" labels never show the host user.
        env: {
            ...demoEnvironment(dir),
            LOTAR_WEB_UI_EMBEDDED: '1',
            LOTAR_DEFAULT_REPORTER: 'priya',
        },
        stdio: ['ignore', 'pipe', 'pipe'],
    });
    let output = '';
    child.stderr.on('data', (d) => { output += d; });
    const base = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`lotar serve did not start:\n${output}`)), 20_000);
        child.stdout.on('data', (d) => {
            output += d;
            const match = /URL: (http:\/\/\S+:\d+)/.exec(output);
            if (match) {
                clearTimeout(timer);
                resolve(match[1]);
            }
        });
        child.once('exit', (code) => reject(new Error(`lotar serve exited (${code}):\n${output}`)));
    });
    return { base, stop: () => child.kill() };
}

async function api(base, route, body) {
    const res = await fetch(`${base}${route}`, body
        ? { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) }
        : undefined);
    if (!res.ok) throw new Error(`${route} -> ${res.status} ${await res.text()}`);
    return (await res.json()).data;
}

async function waitFor(check, { timeout = 30_000, interval = 250, label = 'condition' } = {}) {
    const deadline = Date.now() + timeout;
    for (;;) {
        const value = await check();
        if (value) return value;
        if (Date.now() > deadline) throw new Error(`Timed out waiting for ${label}`);
        await sleep(interval);
    }
}

// ---------------------------------------------------------------------------
// Page helpers
// ---------------------------------------------------------------------------

const pendingRequests = new WeakMap();

async function newCapturePage(context) {
    const page = await context.newPage();
    const requests = new Set();
    pendingRequests.set(page, requests);
    page.on('request', (request) => {
        if (request.resourceType() !== 'eventsource') requests.add(request);
    });
    page.on('requestfinished', (request) => requests.delete(request));
    page.on('requestfailed', (request) => requests.delete(request));
    return page;
}

async function settle(page, ms = 600) {
    // Live EventSource streams never reach network idle.
    await page.waitForLoadState('domcontentloaded');
    await page.evaluate(() => document.fonts?.ready);
    await sleep(ms);
    await waitFor(() => pendingRequests.get(page).size === 0, { label: 'page requests excluding live events' });
}

async function openTaskFromBoard(page, title) {
    const card = page.locator('article.card.task').filter({ hasText: title }).first();
    await card.waitFor({ state: 'visible' });
    await card.scrollIntoViewIfNeeded();
    await card.dblclick();
    await page.locator('.task-panel').first().waitFor({ state: 'visible' });
    await settle(page, 500);
}

async function showActivityTab(page, label) {
    const tab = page.locator('.task-panel__tab', { hasText: label });
    await tab.scrollIntoViewIfNeeded();
    await tab.click();
    await settle(page, 400);
}

// Shots: each prepares a page that is already at the right size and theme.
function defineShots(demo) {
    return [
        { name: 'tasks', path: `/?project=${demo.project}` },
        { name: 'board', path: `/boards?project=${demo.project}` },
        {
            name: 'sprints', path: `/sprints?project=${demo.project}`,
            async prepare(page) {
                // Open the active sprint's insights to show the burndown.
                const insights = page.getByRole('button', { name: 'Insights' }).first();
                await insights.click();
                await page.getByRole('tab', { name: 'Burndown' }).or(page.getByRole('button', { name: 'Burndown' })).first().click();
                await page.locator('canvas').first().waitFor({ state: 'visible', timeout: 10_000 }).catch(() => {});
                await settle(page, 900);
            },
        },
        { name: 'insights', path: `/insights?project=${demo.project}`, settle: 1200 },
        { name: 'calendar', path: `/calendar?project=${demo.project}` },
        { name: 'automations', path: '/automations' },
        {
            name: 'sync', path: '/sync',
            async prepare(page) {
                const scope = page.locator('select').first();
                await scope.selectOption({ label: /ATLS/ }).catch(async () => {
                    const values = await scope.locator('option').allTextContents();
                    const match = values.find((v) => v.includes(demo.project));
                    if (match) await scope.selectOption({ label: match });
                });
                await settle(page, 800);
            },
        },
        {
            name: 'scan', path: '/scan',
            async prepare(page) {
                await page.getByRole('button', { name: /Dry run/ }).click();
                await page.getByText(/Toolbar\.tsx|EditorView\.swift|notes\.rs/).first().waitFor({ timeout: 15_000 });
                await settle(page, 600);
            },
        },
        {
            name: 'task-details', path: `/boards?project=${demo.project}`,
            async prepare(page) {
                await openTaskFromBoard(page, demo.byKey.offline.title);
                await showActivityTab(page, 'Comments');
                await page.locator('.task-panel__activity').evaluate((el) => el.scrollIntoView({ block: 'end' }));
                await settle(page, 400);
            },
        },
        {
            name: 'task-commits', path: `/boards?project=${demo.project}`,
            async prepare(page) {
                await openTaskFromBoard(page, demo.byKey.offline.title);
                await showActivityTab(page, 'Commits');
                await page.locator('.task-panel__activity').evaluate((el) => el.scrollIntoView({ block: 'end' }));
                await settle(page, 600);
            },
        },
        { name: 'config', path: '/config' },
    ];
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

async function toWebp(encoderPage, png, quality = WEBP_QUALITY) {
    const dataUrl = await encoderPage.evaluate(async ({ b64, q }) => {
        const img = new Image();
        img.src = `data:image/png;base64,${b64}`;
        await img.decode();
        const canvas = document.createElement('canvas');
        canvas.width = img.naturalWidth;
        canvas.height = img.naturalHeight;
        canvas.getContext('2d').drawImage(img, 0, 0);
        return canvas.toDataURL('image/webp', q);
    }, { b64: png.toString('base64'), q: quality });
    return Buffer.from(dataUrl.slice(dataUrl.indexOf(',') + 1), 'base64');
}

function kb(file) {
    return `${Math.round(statSync(file).size / 1024)} KB`;
}

// ---------------------------------------------------------------------------
// Captures
// ---------------------------------------------------------------------------

async function captureShots(browser, server, demo, opts) {
    const shots = defineShots(demo).filter((s) => (!opts.only || opts.only.has(s.name)) && (opts.gitHistory || s.name !== 'task-commits'));
    const context = await browser.newContext({ ...PAGE_DEFAULTS, viewport: VIEWPORT, deviceScaleFactor: SCALE, colorScheme: THEMES[0], reducedMotion: 'reduce' });
    await context.addInitScript(() => {
        if (location.protocol === 'http:' || location.protocol === 'https:') {
            localStorage.clear();
            sessionStorage.clear();
        }
    });
    const page = await newCapturePage(context);
    for (const theme of THEMES) {
        await page.emulateMedia({ colorScheme: theme });
        for (const shot of shots) {
            await page.goto(`${server.base}${shot.path}`);
            await settle(page, shot.settle ?? 700);
            if (shot.prepare) await shot.prepare(page);
            const png = await page.screenshot({ type: 'png' });
            const file = path.join(opts.out, `${shot.name}-${theme}.webp`);
            writeFileSync(file, await toWebp(page, png));
            console.log(`  ${path.relative(ROOT, file)} (${kb(file)})`);
        }
    }
    return page;
}

async function assignToAgent(server, id) {
    await api(server.base, '/api/tasks/update', { id, assignee: '@claude' });
}

async function captureAgents(browser, server, demo, opts) {
    // Hold the stub mid-run so both themes catch a live, streaming job.
    writeFileSync(demo.holdFile, 'hold');
    try {
        await assignToAgent(server, demo.ids.encrypt);
        await waitFor(async () => {
            const jobs = await api(server.base, '/api/jobs').catch(() => null);
            const list = Array.isArray(jobs) ? jobs : jobs?.jobs ?? [];
            return list.some((j) => j.ticket_id === demo.ids.encrypt && j.status === 'running');
        }, { label: 'running agent job' });
        await sleep(5_500);
        for (const theme of THEMES) {
            const context = await browser.newContext({ ...PAGE_DEFAULTS, viewport: VIEWPORT, deviceScaleFactor: SCALE, colorScheme: theme, reducedMotion: 'reduce' });
            const page = await newCapturePage(context);
            await page.goto(`${server.base}/agents`);
            await settle(page, 600);
            const running = page.locator('.job-card.job-running').first();
            await running.getByRole('button', { name: 'Show logs' }).click();
            await page.locator('.job-card.job-running .log-row').nth(4).waitFor({ timeout: 15_000 });
            await settle(page, 500);
            const file = path.join(opts.out, `agents-${theme}.webp`);
            writeFileSync(file, await toWebp(page, await page.screenshot({ type: 'png' })));
            console.log(`  ${path.relative(ROOT, file)} (${kb(file)})`);
            await context.close();
        }
    } finally {
        if (existsSync(demo.holdFile)) unlinkSync(demo.holdFile);
    }
}

function backoffTitle(demo) {
    const file = path.join(demo.dir, '.tasks', demo.project, `${demo.ids.backoff.split('-')[1]}.yml`);
    return readFileSync(file, 'utf8').match(/^title:\s*(.+)$/m)[1].trim();
}

async function recordHero(browser, server, demo, opts, tmp) {
    const videoDir = path.join(tmp, 'video');
    const context = await browser.newContext({ ...PAGE_DEFAULTS, viewport: VIEWPORT, deviceScaleFactor: 1, colorScheme: 'light', recordVideo: { dir: videoDir, size: VIEWPORT } });
    const page = await newCapturePage(context);
    const t0 = Date.now();
    await page.goto(`${server.base}/boards?project=${demo.project}`);
    await settle(page, 300);
    const startAt = (Date.now() - t0) / 1000;

    const title = backoffTitle(demo);
    await sleep(900);
    await openTaskFromBoard(page, title);
    await sleep(700);
    const select = page.locator('#task-panel-assignee-select');
    await select.scrollIntoViewIfNeeded();
    await sleep(500);
    const options = await select.locator('option').allTextContents();
    const agentOption = options.find((o) => o.includes('claude'));
    if (agentOption) {
        await select.selectOption({ label: agentOption });
    } else {
        await select.selectOption('__custom');
        const input = page.getByPlaceholder('Type assignee');
        await input.fill('@claude');
        await input.press('Enter');
    }
    await sleep(1200);
    await page.keyboard.press('Escape');
    await page.getByRole('link', { name: 'Agents' }).click();
    await settle(page, 300);
    const card = page.locator('.job-card').first();
    await card.waitFor();
    await card.getByRole('button', { name: 'Show logs' }).click();
    await waitFor(async () => (await page.locator('.job-card.job-completed').count()) > 0, { timeout: 40_000, label: 'hero job completion' });
    await sleep(1500);
    await page.goto(`${server.base}/boards?project=${demo.project}`);
    await settle(page, 600);
    await sleep(800);
    await openTaskFromBoard(page, title);
    await showActivityTab(page, 'History');
    await page.locator('.task-panel__activity').evaluate((el) => el.scrollIntoView({ block: 'center' }));
    await sleep(2600);
    const endAt = (Date.now() - t0) / 1000;
    const video = await page.video().path();
    await context.close();
    return { video, startAt, endAt };
}

function encodeAnimation(video, startAt, endAt, outBase, tmp) {
    const duration = Math.max(1, endAt - startAt);
    const fps = 10;
    const width = 1200;
    if (hasTool('ffmpeg') && hasTool('img2webp')) {
        const frames = path.join(tmp, 'frames');
        mkdirSync(frames, { recursive: true });
        const ff = spawnSync('ffmpeg', ['-y', '-loglevel', 'error', '-ss', startAt.toFixed(2), '-t', duration.toFixed(2), '-i', video,
            '-vf', `fps=${fps},scale=${width}:-1:flags=lanczos`, path.join(frames, 'f%04d.png')], { stdio: 'inherit' });
        if (ff.status !== 0) throw new Error('ffmpeg frame extraction failed');
        const list = readdirSync(frames).filter((f) => f.endsWith('.png')).sort().map((f) => path.join(frames, f));
        const out = `${outBase}.webp`;
        const enc = spawnSync('img2webp', ['-loop', '0', '-lossy', '-q', '55', '-m', '4', '-d', String(Math.round(1000 / fps)), ...list, '-o', out], { stdio: ['ignore', 'ignore', 'inherit'] });
        if (enc.status !== 0) throw new Error('img2webp failed');
        return out;
    }
    if (hasTool('ffmpeg')) {
        const out = `${outBase}.gif`;
        const ff = spawnSync('ffmpeg', ['-y', '-loglevel', 'error', '-ss', startAt.toFixed(2), '-t', duration.toFixed(2), '-i', video,
            '-vf', `fps=${fps},scale=${width}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128[p];[b][p]paletteuse=dither=bayer`, out], { stdio: 'inherit' });
        if (ff.status !== 0) throw new Error('ffmpeg GIF encoding failed');
        return out;
    }
    console.warn('  ffmpeg not found: keeping the raw Playwright video only');
    return null;
}

async function captureSocial(page, opts, heroShot) {
    await page.setViewportSize({ width: 1280, height: 640 });
    const img = readFileSync(heroShot).toString('base64');
    await page.setContent(`<!doctype html><html><head><style>
        * { box-sizing: border-box; margin: 0; }
        body { width: 1280px; height: 640px; overflow: hidden; background: #0b1220; color: #e5edf7;
               font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, Roboto, sans-serif; position: relative; }
        .glow { position: absolute; inset: 0; background: radial-gradient(900px 500px at 15% 10%, rgba(14,165,233,.28), transparent 60%); }
        .text { position: absolute; left: 72px; top: 92px; width: 520px; }
        h1 { font-size: 88px; letter-spacing: -2px; font-weight: 800; }
        h1 span { color: #38bdf8; }
        p { margin-top: 22px; font-size: 30px; line-height: 1.3; color: #b6c4d6; }
        .pills { margin-top: 34px; display: flex; flex-wrap: wrap; gap: 10px; }
        .pills span { font-size: 18px; padding: 7px 14px; border-radius: 999px; border: 1px solid #27405f; color: #9fb6d1; }
        .shot { position: absolute; left: 640px; top: 70px; width: 820px; border-radius: 14px; border: 1px solid #22344d;
                box-shadow: 0 30px 80px rgba(0,0,0,.55); }
    </style></head><body><div class="glow"></div>
    <div class="text"><h1>Lo<span>TaR</span></h1>
      <p>A git-native issue tracker that lives in your repo, and can run your coding agents.</p>
      <div class="pills"><span>CLI</span><span>Web UI</span><span>MCP</span><span>Agents</span><span>Sprints</span></div></div>
    <img class="shot" src="data:image/webp;base64,${img}"></body></html>`);
    await page.locator('img.shot').evaluate((el) => el.decode());
    const file = path.join(path.dirname(opts.out), 'social-preview.png');
    await page.screenshot({ path: file, type: 'png', scale: 'css' });
    console.log(`  ${path.relative(ROOT, file)} (${kb(file)})`);
}

function recordTerminal(bin, demo, opts) {
    if (!hasTool('vhs')) {
        console.warn('  vhs not found (https://github.com/charmbracelet/vhs): skipping terminal.gif');
        return;
    }
    const tape = path.join(ROOT, 'scripts', 'demo', 'terminal.tape');
    const res = spawnSync('vhs', [tape], {
        cwd: demo.dir,
        stdio: 'inherit',
        env: {
            ...demoEnvironment(demo.dir),
            PATH: `${path.dirname(bin)}${path.delimiter}${process.env.PATH}`,
            LOTAR_DEFAULT_REPORTER: 'priya',
        },
    });
    if (res.status !== 0) throw new Error('vhs failed');
    const produced = path.join(demo.dir, 'terminal.gif');
    const file = path.join(opts.out, 'terminal.gif');
    writeFileSync(file, readFileSync(produced));
    console.log(`  ${path.relative(ROOT, file)} (${kb(file)})`);
}

// ---------------------------------------------------------------------------

async function main() {
    const opts = parseArgs(process.argv.slice(2));
    const bin = resolveBinary();
    mkdirSync(opts.out, { recursive: true });
    const tmp = mkdtempSync(path.join(os.tmpdir(), 'lotar-screens-'));
    // Own a unique workspace; never replace another capture's or user's demo.
    const demoRoot = realpathSync(mkdtempSync(path.join(os.tmpdir(), 'lotar-demo-')));
    const dir = path.join(demoRoot, 'atlas');

    console.log(`Seeding demo workspace in ${dir}`);
    const demo = seedDemoWorkspace({ dir, bin, gitHistory: opts.gitHistory });
    if (!opts.gitHistory) console.warn('Git-free capture: preserving existing agent, hero, commit-history and terminal media.');

    // The terminal recording mutates tasks; run it on its own copy of the workspace.
    const server = await startServer(bin, dir);
    console.log(`Serving ${server.base}`);
    const browser = await chromium.launch({ args: (process.env.LOTAR_SCREENSHOT_CHROMIUM_ARGS || '').split(',').filter(Boolean) });
    try {
        console.log('Screenshots');
        const page = await captureShots(browser, server, demo, opts);
        if (opts.gitHistory && opts.hero && (!opts.only || opts.only.has('hero'))) {
            console.log('Hero recording');
            const { video, startAt, endAt } = await recordHero(browser, server, demo, opts, tmp);
            const out = encodeAnimation(video, startAt, endAt, path.join(opts.out, 'hero-agent-loop'), tmp);
            if (out) console.log(`  ${path.relative(ROOT, out)} (${kb(out)})`);
        }
        if (opts.gitHistory && (!opts.only || opts.only.has('agents'))) {
            console.log('Agent jobs');
            await captureAgents(browser, server, demo, opts);
        }
        if (opts.social && (!opts.only || opts.only.has('social'))) {
            const source = path.join(opts.out, 'board-dark.webp');
            if (existsSync(source)) {
                console.log('Social preview');
                await captureSocial(page, opts, source);
            }
        }
    } finally {
        await browser.close();
        server.stop();
    }

    if (opts.gitHistory && opts.terminal && (!opts.only || opts.only.has('terminal'))) {
        console.log('Terminal recording');
        const termDir = path.join(demoRoot, 'atlas-terminal');
        const termDemo = seedDemoWorkspace({ dir: termDir, bin });
        recordTerminal(bin, termDemo, opts);
        if (!opts.keep) removeDemoWorkspace(termDir);
    }

    if (opts.keep) {
        console.log(`Kept demo workspace: ${dir}`);
    } else {
        removeDemoWorkspace(dir);
        rmSync(demoRoot, { recursive: true, force: true });
        rmSync(tmp, { recursive: true, force: true });
    }
}

main().catch((err) => {
    console.error(err);
    process.exit(1);
});
