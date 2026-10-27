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

const SCROLL_TOP = 400;
// Mirrors view/utils/routerScroll.ts: on Back the router waits at most this
// long for the page to grow tall enough before restoring the saved position.
const RESTORE_BUDGET_MS = 1500;

type ReachProbe = { reachedAfter: number | null };

/**
 * Starts an in-page clock just before Back. It records how long it takes for
 * the board to render tall enough to scroll to `SCROLL_TOP`. The clock starts
 * before the router's own wait does, so the measured time is an upper bound.
 */
async function startReachProbe(page: Page) {
    await page.evaluate((top) => {
        const start = performance.now();
        const probe: ReachProbe = { reachedAfter: null };
        (window as unknown as { __boardReach: ReachProbe }).__boardReach = probe;
        const check = () => {
            const elapsed = performance.now() - start;
            const onBoard = location.pathname.startsWith('/boards') && document.querySelector('article.task');
            if (onBoard && document.documentElement.scrollHeight - window.innerHeight >= top) {
                probe.reachedAfter = elapsed;
            } else if (elapsed < 60_000) {
                window.setTimeout(check, 10);
            }
        };
        check();
    }, SCROLL_TOP);
}

function reachedAfter(page: Page) {
    return page.evaluate(() => (window as unknown as { __boardReach?: ReachProbe }).__boardReach?.reachedAfter ?? null);
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

                    // The unfiltered board also shows 30 cards, and the filtered
                    // query starts after a debounce and swaps the grid for a
                    // loader while it is new. Wait for it to land so later
                    // scrolling happens on the final, full-height board.
                    const filtered = page.waitForResponse((response) => {
                        const url = new URL(response.url());
                        return url.pathname === '/api/tasks/list' && url.searchParams.get('q') === 'Card';
                    });
                    await search.fill('Card');
                    await search.press('Enter');
                    expect((await filtered).status()).toBe(200);
                    await expect.poll(() => page.url()).toContain('project=PRB');
                    await expect.poll(cards).toBe(30);
                    await page.locator('.show-more-btn').first().click();
                    await expect.poll(cards).toBe(35);

                    const scrollY = () => page.evaluate(() => window.scrollY);
                    const scrollDown = async () => {
                        await page.evaluate((top) => window.scrollTo(0, top), SCROLL_TOP);
                        await expect.poll(scrollY).toBe(SCROLL_TOP);
                    };
                    const expectKeptState = async () => {
                        await expect.poll(() => search.inputValue()).toBe('Card');
                        await expect.poll(() => page.getByTestId('board-sortby').inputValue()).toBe('id');
                        await expect.poll(cards).toBe(35);
                    };

                    await scrollDown();
                    await leaveAndReturn(page, 'nav');
                    await expectKeptState();

                    // Back restores the scroll position. The router only waits
                    // RESTORE_BUDGET_MS for the board to render tall enough, so
                    // under heavy load the restore can legitimately give up. A
                    // round in which the board rendered within the budget must
                    // restore exactly; a slower round is retried, and the test
                    // fails if no round is fast enough.
                    const slowRounds: number[] = [];
                    let restored = false;
                    for (let round = 0; round < 3 && !restored; round += 1) {
                        await scrollDown();
                        const nav = page.getByRole('navigation');
                        await nav.getByRole('link', { name: 'Tasks', exact: true }).click();
                        await page.waitForURL((url) => !url.pathname.startsWith('/boards'));
                        await startReachProbe(page);
                        await page.goBack();
                        await page.waitForSelector('article.task');
                        await expectKeptState();
                        await expect.poll(() => reachedAfter(page)).not.toBeNull();
                        const elapsed = (await reachedAfter(page))!;
                        if (elapsed < RESTORE_BUDGET_MS) {
                            await expect.poll(scrollY).toBe(SCROLL_TOP);
                            restored = true;
                        } else {
                            slowRounds.push(Math.round(elapsed));
                        }
                    }
                    expect(
                        restored,
                        `the board never rendered within the ${RESTORE_BUDGET_MS} ms restore budget (took ${slowRounds.join(', ')} ms)`,
                    ).toBe(true);

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
