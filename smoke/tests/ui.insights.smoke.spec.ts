import fs from 'fs-extra';
import path from 'node:path';
import type { Page, Route } from '@playwright/test';
import { describe, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser, withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-91 / DEV-187: Insights is computed from one live, project-scoped task
// query. It stays scoped to the selected project, updates over SSE after REST
// changes, remembers the chosen view and period, and never pulls the user back
// after they leave while it is still loading.
interface SeedSpec {
    readonly project: string;
    readonly title: string;
    readonly status?: string;
}

async function seedTask(workspace: SmokeWorkspace, spec: SeedSpec) {
    const created = await workspace.addTask(spec.title, { args: [`--project=${spec.project}`] });
    if (spec.status) {
        const yaml = parse(await fs.readFile(created.filePath, 'utf8')) as Record<string, unknown>;
        yaml.status = spec.status;
        await fs.writeFile(created.filePath, stringify(yaml));
    }
    return created;
}

async function apiMutate(server: { url: string }, route: string, body: Record<string, unknown>) {
    const res = await fetch(`${server.url}${route}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        body: JSON.stringify(body),
    });
    if (!res.ok) throw new Error(`POST ${route} failed: ${res.status} ${await res.text()}`);
}

function tileValue(page: Page, label: string) {
    return page
        .locator('.stat-tile')
        .filter({
            has: page.locator('.stat-tile__label', {
                hasText: new RegExp(`^\\s*${label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*$`),
            }),
        })
        .locator('.stat-tile__value')
        .textContent()
        .then((text) => text?.trim() ?? '');
}

function trackLists(page: Page) {
    const urls: string[] = [];
    page.on('request', (request) => {
        if (new URL(request.url()).pathname === '/api/tasks/list') urls.push(request.url());
    });
    return {
        urls,
        withProject: (value: string | null) =>
            urls.filter((url) => new URL(url).searchParams.get('project') === value).length,
    };
}

describe.concurrent('UI Insights (DEV-91)', () => {
    it('stays project-scoped and only widens when the user picks all projects', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();
        try {
            await seedTask(workspace, { project: 'BASE', title: 'Insights base todo' });
            await seedTask(workspace, { project: 'FRESH', title: 'Insights fresh todo' });
            const server = await startLotarServer(workspace);
            try {
                await withBrowser({}, async (context) => {
                    // Track before navigating so the very first query is seen.
                    const page = await context.newPage();
                    const lists = trackLists(page);
                    await page.goto(`${server.url}/insights?project=BASE`, { waitUntil: 'domcontentloaded' });
                    await expect.poll(() => tileValue(page, 'Open')).toBe('1');
                    expect(lists.urls.length).toBeGreaterThan(0);
                    expect(lists.withProject('BASE')).toBe(lists.urls.length);

                    await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                    await page.getByTestId('filter-project').selectOption('');
                    await expect.poll(() => tileValue(page, 'Open')).toBe('2');
                    expect(lists.withProject(null)).toBeGreaterThan(0);
                    expect(lists.withProject('FRESH')).toBe(0);
                    await page.close();
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('updates live after a REST change, across every view', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();
        try {
            const first = await seedTask(workspace, { project: 'LIVE', title: 'Insights live one' });
            await seedTask(workspace, { project: 'LIVE', title: 'Insights live two' });
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/insights?project=LIVE`, async (page) => {
                    await expect.poll(() => tileValue(page, 'Open')).toBe('2');
                    expect(await tileValue(page, 'Completed')).toBe('0');

                    await apiMutate(server, '/api/tasks/update', { id: first.id, status: 'Done' });
                    await expect.poll(() => tileValue(page, 'Completed')).toBe('1');
                    expect(await tileValue(page, 'Open')).toBe('1');

                    await page.getByTestId('insights-tab-delivery').click();
                    await expect.poll(() => tileValue(page, 'Lead time (median)')).not.toBe('—');
                    for (const lens of ['team', 'planning', 'quality', 'mine']) {
                        await page.getByTestId(`insights-tab-${lens}`).click();
                        await page.locator(`#insights-panel-${lens}`).waitFor();
                    }
                    const artifacts = path.resolve('target/smoke-artifacts/insights');
                    await fs.ensureDir(artifacts);
                    await page.getByTestId('insights-tab-overview').click();
                    await page.screenshot({ path: path.join(artifacts, 'overview.png'), fullPage: true });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('remembers the chosen view and period', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();
        try {
            await seedTask(workspace, { project: 'KEEP', title: 'Insights keep' });
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/insights?project=KEEP`, async (page) => {
                    await expect.poll(() => tileValue(page, 'Open')).toBe('1');
                    await page.getByTestId('insights-tab-planning').click();
                    await page.getByTestId('insights-period').selectOption('90d');
                    await expect.poll(() => new URL(page.url()).searchParams.get('view')).toBe('planning');
                    expect(new URL(page.url()).searchParams.get('period')).toBe('90d');

                    const nav = page.getByRole('navigation');
                    await nav.getByRole('link', { name: 'Tasks', exact: true }).click();
                    await page.waitForURL((url) => url.pathname === '/');
                    await nav.getByRole('link', { name: 'Insights', exact: true }).click();
                    await expect
                        .poll(() => page.getByTestId('insights-tab-planning').getAttribute('aria-selected'))
                        .toBe('true');
                    expect(await page.getByTestId('insights-period').inputValue()).toBe('90d');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('never pulls the user back after they leave during the first load (DEV-187)', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();
        try {
            await seedTask(workspace, { project: 'SLOW', title: 'Insights slow' });
            const server = await startLotarServer(workspace);
            try {
                await withBrowser({}, async (context) => {
                    let release!: () => void;
                    const held = new Promise<void>((resolve) => (release = resolve));
                    let heldOnce = false;
                    await context.route('**/api/tasks/list**', async (route: Route) => {
                        // Hold Insights' first task query (the first one the page makes).
                        if (!heldOnce) {
                            heldOnce = true;
                            await held;
                        }
                        await route.continue();
                    });
                    const page = await context.newPage();
                    await page.goto(`${server.url}/insights?project=SLOW`, { waitUntil: 'domcontentloaded' });
                    await page.locator('.app-page-header__title').waitFor();
                    await expect.poll(() => heldOnce).toBe(true);

                    // Leave while Insights is still waiting for its data.
                    await page.getByRole('navigation').getByRole('link', { name: 'Sprints', exact: true }).click();
                    await page.waitForURL((url) => url.pathname === '/sprints');
                    const finished = page.waitForResponse((response) => response.url().includes('/api/tasks/list'));
                    release();
                    await finished;
                    await page.waitForLoadState('networkidle');
                    expect(new URL(page.url()).pathname).toBe('/sprints');
                    await page.close();
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
