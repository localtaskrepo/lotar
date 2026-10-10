import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

function seed(): Record<string, string> {
    const today = new Date().toISOString().slice(0, 10);
    const files: Record<string, string> = {
        '.tasks/config.yml': stringify({
            issue: { states: ['Todo', 'Done'], priorities: ['Low', 'Medium', 'High'], types: ['Feature', 'Bug', 'Chore'] },
            custom_fields: ['iteration', 'scope', 'sprint'],
            default: { reporter: 'Fixture' }, auto: { set_reporter: false },
        }),
        '.tasks/FIL/config.yml': 'project.name: Filter fixture\n',
        '.tasks/ALT/config.yml': 'project.name: Alternate fixture\n',
        '.tasks/@sprints/1.yml': stringify({
            created: `${today}T00:00:00Z`, plan_length: '1w',
            plan: { label: 'Fixture sprint', starts_at: `${today}T00:00:00Z` },
            actual: { started_at: `${today}T00:00:00Z` }, tasks: [{ id: 'FIL-1' }, { id: 'FIL-2' }],
        }),
    };
    for (const [id, title, iteration, deleted] of [
        ['FIL-1', 'Alpha task', 'alpha', false],
        ['FIL-2', 'Beta task', 'beta release', false],
        ['FIL-3', 'Deleted beta task', 'beta release', true],
        ['ALT-1', 'Foreign task', 'beta release', false],
    ] as const) {
        const [project, number] = id.split('-');
        files[`.tasks/${project}/${number}.yml`] = stringify({
            title, status: 'Todo', priority: 'High', type: 'Feature', reporter: 'Fixture',
            due_date: today, created: `${today}T00:00:00Z`, modified: `${today}T00:00:00Z`,
            custom_fields: { iteration, scope: 'north', sprint: id === 'FIL-2' ? 'inc-2' : 'inc-1' },
            tags: id === 'FIL-2' ? ['ui', 'api'] : ['other'],
            ...(['FIL-1', 'FIL-2'].includes(id) ? { sprints: [1] } : {}),
            ...(deleted ? { deleted_at: `${today}T00:00:00Z` } : {}),
        });
    }
    return files;
}

describe.concurrent('Unified filter search and panel (DEV-98)', () => {
    for (const viewport of [
        { label: 'desktop', width: 1280, height: 900 },
        { label: 'mobile', width: 390, height: 844 },
    ]) {
        it(`uses one search input and preserves scoped filters on ${viewport.label}`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
            const before = await workspace.readTaskYaml('FIL-2');
            const artifacts = path.resolve('target/smoke-artifacts/filters');
            await fs.ensureDir(artifacts);
            try {
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/`, async page => {
                        await page.setViewportSize(viewport);
                        const mutations: string[] = [];
                        const listQueries: string[] = [];
                        page.on('request', request => {
                            const url = new URL(request.url());
                            if (['POST', 'PUT', 'DELETE'].includes(request.method())) mutations.push(url.pathname);
                            if (url.pathname === '/api/tasks/list') listQueries.push(url.search);
                        });
                        const search = page.getByTestId('filter-search');
                        const toggle = page.getByTestId('filter-toggle');
                        const panel = page.getByRole('region', { name: 'Filters', exact: true });
                        await toggle.click();
                        await page.getByTestId('filter-project').selectOption('FIL');
                        await expect.poll(() => page.getByText('Alpha task', { exact: true }).count()).toBe(1);
                        expect(await page.getByLabel('Custom filters', { exact: true }).count()).toBe(0);
                        expect(await panel.getByPlaceholder('Tags', { exact: true }).count()).toBe(0);
                        expect(await panel.getByRole('heading', { name: 'Quick picks', exact: true }).count()).toBe(1);
                        expect(await panel.getByRole('heading', { name: 'Filter fields', exact: true }).count()).toBe(1);
                        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
                        const searchBounds = await search.boundingBox();
                        const toggleBounds = await toggle.boundingBox();
                        expect(Math.abs(searchBounds!.y - toggleBounds!.y)).toBeLessThanOrEqual(1);
                        expect(await page.locator('.filter-bar__search-hint').evaluate(el => getComputedStyle(el).clipPath)).toBe('inset(50%)');
                        const status = page.getByTestId('filter-status');
                        await status.click();
                        const statusOptions = page.getByRole('group', { name: 'Status choices', exact: true });
                        const popup = await statusOptions.boundingBox();
                        expect(popup).not.toBeNull();
                        expect(popup!.x).toBeGreaterThanOrEqual(0);
                        expect(popup!.x + popup!.width).toBeLessThanOrEqual(viewport.width);
                        await page.keyboard.press('Escape');
                        await statusOptions.waitFor({ state: 'detached' });
                        expect(await status.evaluate(el => el === document.activeElement)).toBe(true);

                        await panel.getByRole('button', { name: 'iteration', exact: true }).click();
                        expect(await search.inputValue()).toBe('field:iteration=');
                        expect(await search.evaluate(el => el === document.activeElement)).toBe(true);
                        await expect.poll(() => page.getByText('Alpha task', { exact: true }).count()).toBe(1);
                        await search.fill('task field:iteration="beta release"');
                        await search.press('Enter');
                        await expect.poll(() => page.getByText('Alpha task', { exact: true }).count()).toBe(0);
                        await expect.poll(() => page.getByText('Beta task', { exact: true }).count()).toBe(1);
                        expect(await page.getByText('Foreign task', { exact: true }).count()).toBe(0);
                        expect(await page.getByText('Deleted beta task', { exact: true }).count()).toBe(0);
                        expect(listQueries.some(query => new URLSearchParams(query).get('field:iteration') === 'beta release')).toBe(true);
                        expect(await page.getByTestId('filter-chips').textContent()).toContain('beta release');
                        const clearSearch = page.getByRole('button', { name: 'Clear search', exact: true });
                        await clearSearch.click();
                        expect(await search.inputValue()).toBe('');
                        expect(await clearSearch.count()).toBe(0);
                        expect(await search.evaluate(el => el === document.activeElement)).toBe(true);
                        expect(await page.getByTestId('filter-chips').textContent()).toContain('beta release');
                        expect(await page.getByText('Beta task', { exact: true }).count()).toBe(1);
                        await search.fill('tags:ui');
                        await search.press('Enter');
                        await expect.poll(() => listQueries.some(query => new URLSearchParams(query).get('tags') === 'ui')).toBe(true);
                        await page.getByRole('button', { name: 'Remove filter Tag ui', exact: true }).click();
                        await panel.getByRole('button', { name: 'sprint', exact: true }).click();
                        expect(await search.inputValue()).toBe('field:"sprint"=');
                        await search.fill('task field:"sprint"=inc-2 ');
                        await search.press('Enter');
                        await expect.poll(() => listQueries.some(query => new URLSearchParams(query).get('field:sprint') === 'inc-2')).toBe(true);
                        expect(listQueries.some(query => new URLSearchParams(query).get('sprints') === 'inc-2')).toBe(false);
                        expect(await page.getByText('Beta task', { exact: true }).count()).toBe(1);
                        await page.getByRole('button', { name: 'Remove filter sprint inc-2', exact: true }).click();
                        await expect.poll(() => new URL(page.url()).searchParams.has('field:sprint')).toBe(false);
                        await search.press('Escape');
                        if (!(await panel.isVisible())) await toggle.click();
                        await page.screenshot({ path: path.join(artifacts, `tasks-${viewport.label}.png`), animations: 'disabled' });

                        await search.fill('status:Done field:iteration=');
                        await search.press('Enter');
                        await page.locator('.filter-bar__search-error').waitFor({ state: 'visible' });
                        expect(await search.getAttribute('aria-invalid')).toBe('true');
                        expect(await page.getByText('Beta task', { exact: true }).count()).toBe(1);
                        expect(await page.getByText('Alpha task', { exact: true }).count()).toBe(0);
                        expect(listQueries.some(query => new URLSearchParams(query).get('q')?.includes('field:iteration='))).toBe(false);
                        await search.fill('task');
                        await search.press('Enter');
                        await page.reload();
                        await expect.poll(() => page.getByText('Beta task', { exact: true }).count()).toBe(1);
                        expect(await page.getByText('Alpha task', { exact: true }).count()).toBe(0);
                        await page.getByRole('button', { name: 'Remove filter iteration beta release', exact: true }).click();
                        const snapshot = await page.evaluate(() => localStorage.getItem('lotar.tasks.filter'));
                        await expect.poll(() => page.getByText('Alpha task', { exact: true }).count(), { message: `${snapshot} ${listQueries.slice(-3).join(' ')}` }).toBe(1);

                        // Each consumer receives the same structured field key, not a free-text q.
                        for (const route of ['/boards?project=FIL', '/calendar?project=FIL', '/sprints?project=FIL']) {
                            await page.goto(`${server.url}${route}`);
                            const titles = page.locator(route.startsWith('/boards') ? 'article.task .title'
                                : route.startsWith('/calendar') ? '.task-item .task-inline > .title' : 'tr.task-row .task-title');
                            const alpha = titles.filter({ hasText: /^\s*Alpha task\s*$/ });
                            const beta = titles.filter({ hasText: /^\s*Beta task\s*$/ });
                            await expect.poll(() => alpha.count(), { message: route }).toBe(1);
                            await search.fill('field:iteration="beta release"');
                            await search.press('Enter');
                            await expect.poll(() => alpha.count()).toBe(0);
                            await expect.poll(() => beta.count()).toBe(1);
                            await toggle.click();
                            expect(await panel.getByLabel('Custom filters', { exact: true }).count()).toBe(0);
                            expect(await panel.getByPlaceholder('Tags', { exact: true }).count()).toBe(0);
                            if (route.startsWith('/sprints')) expect(await panel.getByRole('heading', { name: 'View options', exact: true }).count()).toBe(1);
                            await panel.getByRole('button', { name: 'Clear conditions', exact: true }).click();
                            await expect.poll(() => alpha.count()).toBe(1);
                            expect(await page.getByTestId('filter-project').inputValue()).toBe('FIL');
                            await panel.getByRole('button', { name: 'Close filters', exact: true }).click();
                            await panel.waitFor({ state: 'detached' });
                            expect(await toggle.evaluate(el => el === document.activeElement)).toBe(true);
                        }
                        expect(mutations).toEqual([]);
                        expect(await workspace.readTaskYaml('FIL-2')).toBe(before);
                    });
                } finally {
                    await server.stop();
                }
            } finally {
                await workspace.dispose();
            }
        });
    }
});
