import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { html5DragAndDrop, withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

interface Envelope<T> {
    data: T;
}

describe.concurrent('UI releases smoke scenarios (DEV-393)', () => {
    it('creates a release, adds a ticket by drag and drop and marks it released', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();

        try {
            const task = await workspace.addTask('Release smoke ticket', { args: ['--project', 'WEB'] });
            const server = await startLotarServer(workspace);

            const releases = async () => {
                const response = await fetch(`${server.url}/api/releases/list`);
                const body = (await response.json()) as Envelope<{
                    releases: Array<{ id: number; version: string; state: string; task_count: number }>;
                }>;
                return body.data.releases;
            };
            const taskReleases = async () => {
                const response = await fetch(`${server.url}/api/tasks/get?id=${encodeURIComponent(task.id)}`);
                const body = (await response.json()) as Envelope<{ releases: number[] }>;
                return body.data.releases;
            };

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.click('a[href="/releases"]');
                    await page.waitForSelector('h1:has-text("Releases")', { timeout: 15_000 });
                    await page.waitForSelector('text=No releases yet', { timeout: 15_000 });

                    // Create a release through the dialog.
                    await page.click('[data-testid="release-create"]');
                    await page.fill('[data-testid="release-form-version"]', 'v0.15.0');
                    await page.fill('[data-testid="release-form-planned"]', '2026-12-01');
                    await page.click('[data-testid="release-form-submit"]');
                    await page.waitForSelector('[data-testid="release-group"] h2:has-text("0.15.0")', {
                        timeout: 10_000,
                    });
                    await expect
                        .poll(releases, { timeout: 10_000 })
                        .toEqual([expect.objectContaining({ version: '0.15.0', state: 'planned', task_count: 0 })]);
                    const [created] = await releases();
                    const releaseSelector = `[data-release-id="${created!.id}"]`;

                    // Backlog → release adds the ticket.
                    const sourceSelector = `[data-release-id="backlog"] tr[data-task-id="${task.id}"]`;
                    await page.waitForSelector(sourceSelector, { timeout: 10_000 });
                    await html5DragAndDrop(page, sourceSelector, releaseSelector);
                    await page.waitForSelector(`${releaseSelector} tr[data-task-id="${task.id}"]`, { timeout: 10_000 });
                    await expect.poll(taskReleases, { timeout: 10_000 }).toEqual([created!.id]);
                    await expect
                        .poll(() => page.textContent(`${releaseSelector} [data-testid="release-counts"]`), {
                            timeout: 10_000,
                        })
                        .toContain('1 open');
                    expect(await page.$(sourceSelector)).toBeNull();

                    // Mark it released.
                    await page.click(`${releaseSelector} [data-testid="release-mark-released"]`);
                    await page.waitForSelector(`${releaseSelector} [data-testid="release-reopen"]`, {
                        timeout: 10_000,
                    });
                    expect((await page.textContent(`${releaseSelector} [data-testid="release-state"]`))?.trim()).toBe(
                        'released',
                    );
                    expect(await page.textContent(releaseSelector)).toContain('Released');
                    await expect
                        .poll(releases, { timeout: 10_000 })
                        .toEqual([expect.objectContaining({ version: '0.15.0', state: 'released', task_count: 1 })]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
