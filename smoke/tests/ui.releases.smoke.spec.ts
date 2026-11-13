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

    it('offers Add to release in the row menu once a release appears (DEV-464)', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();

        try {
            const task = await workspace.addTask('Row menu release ticket', { args: ['--project', 'WEB'] });
            const server = await startLotarServer(workspace);
            const taskReleases = async () => {
                const response = await fetch(`${server.url}/api/tasks/get?id=${encodeURIComponent(task.id)}`);
                const body = (await response.json()) as Envelope<{ releases: number[] }>;
                return body.data.releases;
            };

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=Row menu release ticket', { timeout: 15_000 });
                    await page.click('button[aria-label="Row actions"]');
                    const item = page.locator('.menu-popover .menu-item', { hasText: 'Add to release' });
                    await item.waitFor({ timeout: 10_000 });
                    expect(await item.isDisabled()).toBe(true);

                    // A release created elsewhere reaches the open page through release_changed.
                    const created = await fetch(`${server.url}/api/releases/create`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ version: '0.16.0' }),
                    });
                    expect(created.ok).toBe(true);
                    const { data } = (await created.json()) as Envelope<{ release: { id: number } }>;
                    await expect.poll(() => item.isEnabled(), { timeout: 15_000 }).toBe(true);

                    await item.click();
                    const dialog = page.locator('form', { hasText: 'Add 1 task to release' });
                    await dialog.waitFor({ timeout: 10_000 });
                    expect(await dialog.locator('select').inputValue()).toBe(String(data.release.id));
                    await dialog.locator('button[type="submit"]').click();
                    await expect.poll(taskReleases, { timeout: 10_000 }).toEqual([data.release.id]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('narrows the Releases page to one project (DEV-464)', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();

        try {
            const web = await workspace.addTask('Web release ticket', { args: ['--project', 'WEB'] });
            const api = await workspace.addTask('Api release ticket', { args: ['--project', 'API'] });
            const apiBacklog = await workspace.addTask('Api backlog ticket', { args: ['--project', 'API'] });
            const server = await startLotarServer(workspace);

            try {
                const created = await fetch(`${server.url}/api/releases/create`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ version: '0.17.0' }),
                });
                const { data } = (await created.json()) as Envelope<{ release: { id: number } }>;
                const added = await fetch(`${server.url}/api/releases/add`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ release: data.release.id, tasks: [web.id, api.id] }),
                });
                expect(added.ok).toBe(true);
                const releaseSelector = `[data-release-id="${data.release.id}"]`;

                await withPage(server.url, async (page) => {
                    await page.goto(`${server.url}/releases`);
                    await page.waitForSelector(`${releaseSelector} tr[data-task-id="${web.id}"]`, { timeout: 15_000 });
                    await page.waitForSelector(`${releaseSelector} tr[data-task-id="${api.id}"]`);

                    await page.selectOption('[data-testid="release-project"]', 'API');
                    await page.waitForSelector(`${releaseSelector} tr[data-task-id="${web.id}"]`, {
                        state: 'detached',
                        timeout: 10_000,
                    });
                    expect(new URL(page.url()).searchParams.get('project')).toBe('API');
                    expect(await page.$(`${releaseSelector} tr[data-task-id="${api.id}"]`)).not.toBeNull();
                    expect(await page.textContent(`${releaseSelector} [data-testid="release-counts"]`)).toContain(
                        '1 open',
                    );
                    await page.waitForSelector(`[data-release-id="backlog"] tr[data-task-id="${apiBacklog.id}"]`);
                    expect(await page.$$('[data-release-id="backlog"] tbody tr')).toHaveLength(1);

                    await page.selectOption('[data-testid="release-project"]', '');
                    await page.waitForSelector(`${releaseSelector} tr[data-task-id="${web.id}"]`, { timeout: 10_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
