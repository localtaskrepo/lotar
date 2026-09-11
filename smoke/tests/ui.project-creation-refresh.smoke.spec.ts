import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-25 durable coverage: creating a project in ConfigView must refresh
// already-mounted UI surfaces without a browser reload. The task panel host
// is always mounted and caches its project snapshot after the first open
// (ensureProjectsLoaded only fetches when empty), so the snapshot is warmed
// before creation. Afterwards the reopened panel must offer the new project
// and create a task in it, using SPA navigation only — verified by a window
// marker that a reload would clear.
describe('UI project creation refresh', () => {
    it('propagates a created project to the mounted task panel without reload', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const seed = await workspace.addTask('DEV25 seed task', { args: ['--project=BASE'] });
            expect(seed.project).toBe('BASE');

            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async page => {
                    // Completion tracking for list hydrations: a pending
                    // /api/tasks/list response is not a finished hydration.
                    // The store replaces its map when a clear-hydrate
                    // completes, so a list request still in flight at create
                    // time could drop the optimistic upsert of the created
                    // task and the row wait below would never pass.
                    const pendingLists = new Set<unknown>();
                    let seenLists = 0;
                    page.on('request', request => {
                        if (new URL(request.url()).pathname === '/api/tasks/list') {
                            seenLists += 1;
                            pendingLists.add(request);
                        }
                    });
                    const trackResponse = (response: any) => {
                        response
                            .finished()
                            .catch(() => {})
                            .finally(() => pendingLists.delete(response.request()));
                    };
                    page.on('response', response => {
                        if (new URL(response.url()).pathname === '/api/tasks/list') {
                            trackResponse(response);
                        }
                    });
                    page.on('requestfailed', request => pendingLists.delete(request));

                    await page.waitForSelector('text=DEV25 seed task', { timeout: 20_000 });

                    // Warm the always-mounted task panel's project snapshot.
                    await page.getByRole('button', { name: 'Add task' }).click();
                    const panel = page.locator('.task-panel');
                    await panel.waitFor({ state: 'visible', timeout: 10_000 });
                    const projectSelect = panel.locator('select').first();
                    await expect
                        .poll(() => projectSelect.locator('option[value="BASE"]').count(), { timeout: 10_000 })
                        .toBeGreaterThan(0);
                    await page.getByRole('button', { name: 'Close panel' }).click();
                    await panel.waitFor({ state: 'hidden', timeout: 10_000 });

                    // Marker proving no browser reload happens for the rest of the test.
                    await page.evaluate(() => {
                        (window as any).__dev25NoReload = true;
                    });

                    // Create the project through the config page (SPA navigation).
                    await page.locator('nav a.nav__link', { hasText: 'Config' }).click();
                    await page.getByRole('button', { name: 'New project' }).click();
                    await page.locator('input[placeholder="Marketing website"]').fill('Dev25 Fresh Project');
                    await page.locator('input[placeholder="AUTO"]').fill('FRESH');
                    await page.getByRole('button', { name: 'Create project' }).click();

                    const scopeSelect = page.locator('select.scope-select');
                    await expect
                        .poll(() => scopeSelect.locator('option[value="FRESH"]').count(), { timeout: 10_000 })
                        .toBeGreaterThan(0);
                    await expect.poll(() => scopeSelect.inputValue(), { timeout: 10_000 }).toBe('FRESH');

                    // Reopen the task panel without reloading the page.
                    await page.locator('nav a.nav__link', { hasText: 'Tasks' }).click();
                    await page.getByRole('button', { name: 'Add task' }).click();
                    await panel.waitFor({ state: 'visible', timeout: 10_000 });

                    const reopenedSelect = panel.locator('select').first();
                    await expect
                        .poll(() => reopenedSelect.locator('option[value="FRESH"]').count(), { timeout: 10_000 })
                        .toBeGreaterThan(0);

                    await reopenedSelect.selectOption('FRESH');
                    const create = panel.getByRole('button', { name: 'Create task', exact: true });
                    await expect.poll(() => create.isEnabled(), { timeout: 10_000 }).toBe(true);
                    await panel.locator('input[placeholder="Title"]').fill('DEV25 task in fresh project');
                    // Readiness gate: no list hydration may still be in
                    // flight when the create commits.
                    await expect
                        .poll(() => pendingLists.size === 0 && seenLists > 0, { timeout: 10_000 })
                        .toBe(true);
                    await create.click();
                    await panel.waitFor({ state: 'hidden', timeout: 10_000 });

                    // The task list is scoped to the default project (BASE);
                    // observing the created task through the FRESH project
                    // filter waits on that refetch's completed state instead
                    // of the default view, where the clearing hydrate races
                    // SSE redelivery of the created task (reported separately).
                    await page.click('[data-testid="filter-toggle"]');
                    const projectFilter = page.locator('[data-testid="filter-project"]');
                    await projectFilter.waitFor({ state: 'visible', timeout: 10_000 });
                    await expect
                        .poll(() => projectFilter.locator('option[value="FRESH"]').count(), { timeout: 10_000 })
                        .toBeGreaterThan(0);
                    await projectFilter.selectOption('FRESH');
                    await page.waitForSelector('text=DEV25 task in fresh project', { timeout: 15_000 });
                    expect(await page.evaluate(() => (window as any).__dev25NoReload)).toBe(true);

                    await expect
                        .poll(async () => (await workspace.listTaskFiles()).some(file => file.includes('FRESH')), { timeout: 10_000 })
                        .toBe(true);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
