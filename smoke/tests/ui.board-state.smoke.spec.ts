import { describe, it } from 'vitest';
import type { Page } from 'playwright';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-180 follow-up: board view state survives leaving the board and coming
// back through the nav link or browser Back, and ID sorting is natural.
function seed(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': 'default.project: PRB\n',
        '.tasks/PRB/config.yml': stringify({
            issue: { states: ['Todo', 'Done'], priorities: ['Low', 'High'], done_states: ['Done'] },
        }),
    };
    for (let number = 1; number <= 35; number += 1) {
        files[`.tasks/PRB/${number}.yml`] = stringify({
            title: `Card ${number}`,
            status: 'Todo',
            priority: number % 2 ? 'Low' : 'High',
            type: 'Feature',
            created: '2026-10-01T00:00:00Z',
            modified: '2026-10-01T00:00:00Z',
        });
    }
    return files;
}

async function leaveAndReturn(page: Page, via: 'nav' | 'back') {
    const nav = page.getByRole('navigation');
    await nav.getByRole('link', { name: 'Tasks', exact: true }).click();
    await page.waitForURL((url) => !url.pathname.startsWith('/boards'));
    if (via === 'nav') await nav.getByRole('link', { name: 'Boards', exact: true }).click();
    else await page.goBack();
    await page.waitForSelector('article.task');
}

describe.concurrent('Board view state (DEV-180)', () => {
    it('keeps filter, sort, grouping, collapsed lanes, expanded columns and scroll', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/boards?project=PRB`, async (page) => {
                    await page.setViewportSize({ width: 1280, height: 900 });
                    const search = page.getByTestId('filter-search');
                    const cards = () => page.locator('article.task').count();
                    const ids = () => page.locator('article.task .id').allTextContents();
                    await page.waitForSelector('article.task');

                    // Natural ID order: PRB-2 follows PRB-1, not PRB-10.
                    await page.getByTestId('board-sortby').selectOption('id');
                    await expect.poll(async () => (await ids()).slice(0, 3)).toEqual(['PRB-1', 'PRB-2', 'PRB-3']);

                    await search.fill('Card');
                    await search.press('Enter');
                    await expect.poll(() => page.url()).toContain('project=PRB');
                    await expect.poll(cards).toBe(30);
                    await page.locator('.show-more-btn').first().click();
                    await expect.poll(cards).toBe(35);

                    for (const via of ['nav', 'back'] as const) {
                        await page.evaluate(() => window.scrollTo(0, 400));
                        await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(400);
                        await leaveAndReturn(page, via);
                        await expect.poll(() => search.inputValue()).toBe('Card');
                        await expect.poll(() => page.getByTestId('board-sortby').inputValue()).toBe('id');
                        await expect.poll(cards).toBe(35);
                        if (via === 'back') await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(400);
                    }

                    await page.getByTestId('board-groupby').selectOption('priority');
                    await page.locator('.swimlane-header').filter({ hasText: 'Low' }).click();
                    await expect.poll(cards).toBe(17);
                    await leaveAndReturn(page, 'nav');
                    await expect
                        .poll(() => page.locator('.swimlane-header.collapsed .swimlane-label').allTextContents())
                        .toEqual(['Low']);
                    await expect.poll(() => page.getByTestId('board-groupby').inputValue()).toBe('priority');
                    expect(await cards()).toBe(17);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
