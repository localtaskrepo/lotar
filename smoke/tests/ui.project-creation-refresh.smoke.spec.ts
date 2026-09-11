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
                    await create.click();
                    await panel.waitFor({ state: 'hidden', timeout: 10_000 });

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
