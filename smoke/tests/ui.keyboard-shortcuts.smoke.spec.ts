import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-158: command palette (Ctrl/Cmd+K) and single-key shortcuts.
function seed(): Record<string, string> {
    return {
        '.tasks/config.yml': stringify({
            issue: { states: ['Todo', 'Done'], priorities: ['Low', 'Medium', 'High'], types: ['Feature'] },
            default: { reporter: 'Fixture', project: 'KEY' },
            auto: { set_reporter: false },
        }),
        '.tasks/KEY/config.yml': 'project.name: Keyboard fixture\n',
        '.tasks/KEY/7.yml': stringify({
            title: 'Keyboard reachable task',
            status: 'Todo',
            priority: 'Medium',
            type: 'Feature',
            reporter: 'Fixture',
            created: '2026-01-01T00:00:00Z',
            modified: '2026-01-01T00:00:00Z',
        }),
    };
}

describe('Keyboard shortcuts and command palette (DEV-158)', () => {
    it('navigates, creates and finds tasks from the keyboard', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
        const server = await startLotarServer(workspace);
        try {
            await withPage(`${server.url}/`, async (page) => {
                await page.getByTestId('filter-search').waitFor({ state: 'visible' });
                const isMac = await page.evaluate(() => /mac/i.test(navigator.platform));
                const openPalette = () => page.keyboard.press(isMac ? 'Meta+k' : 'Control+k');
                const palette = page.getByRole('dialog', { name: 'Command palette', exact: true });
                const input = page.getByTestId('command-palette-input');

                // Jump to a page through the palette.
                await openPalette();
                await input.waitFor({ state: 'visible' });
                expect(await input.evaluate((el) => el === document.activeElement)).toBe(true);
                await input.fill('boards');
                await input.press('Enter');
                await expect.poll(() => new URL(page.url()).pathname).toBe('/boards');
                await palette.waitFor({ state: 'detached' });

                // g then a letter, from the page body.
                await page.locator('body').click({ position: { x: 5, y: 5 } });
                await page.keyboard.press('g');
                await page.keyboard.press('c');
                await expect.poll(() => new URL(page.url()).pathname).toBe('/calendar');

                // ? shows the reference; Esc closes it.
                await page.keyboard.press('Shift+?');
                const help = page.getByTestId('keyboard-shortcuts');
                await help.waitFor({ state: 'visible' });
                expect(await help.textContent()).toContain('Command palette');
                await page.keyboard.press('Escape');
                await help.waitFor({ state: 'detached' });

                // c opens the new task editor.
                await page.keyboard.press('c');
                const editor = page.getByRole('dialog', { name: 'Create task', exact: true });
                await editor.waitFor({ state: 'visible' });
                await page.keyboard.press('Escape');
                await editor.waitFor({ state: 'detached' });

                // Find a task by ID and open it.
                await openPalette();
                await input.fill('KEY-7');
                await expect.poll(() => page.getByTestId('command-palette-option-task-KEY-7').count()).toBe(1);
                await input.press('Enter');
                await page.locator('.task-panel').waitFor({ state: 'visible' });
                await expect.poll(() => page.locator('.task-panel').textContent()).toContain('Keyboard reachable task');
                await page.keyboard.press('Escape');
                await page.locator('.task-panel').waitFor({ state: 'detached' });

                // With single-key shortcuts off, g sequences do nothing, but the palette works.
                await page.goto(`${server.url}/preferences`);
                const toggle = page.getByTestId('preferences-single-key-shortcuts');
                await toggle.waitFor({ state: 'visible' });
                await toggle.uncheck();
                await page.locator('body').click({ position: { x: 5, y: 5 } });
                await page.keyboard.press('g');
                await page.keyboard.press('t');
                await page.waitForTimeout(300);
                expect(new URL(page.url()).pathname).toBe('/preferences');
                await openPalette();
                await input.waitFor({ state: 'visible' });
            });
        } finally {
            await server.stop();
            await workspace.dispose();
        }
    });
    it('acts on the open task, lists recent tasks, applies quick filters and moves with j/k (DEV-198)', async ({
        expect,
    }) => {
        const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
        const server = await startLotarServer(workspace);
        try {
            await withPage(`${server.url}/`, async (page) => {
                const listQueries: string[] = [];
                page.on('request', (request) => {
                    const url = new URL(request.url());
                    if (url.pathname === '/api/tasks/list') listQueries.push(url.search);
                });
                await page.getByTestId('filter-search').waitFor({ state: 'visible' });
                await page.locator('[data-task-row]').first().waitFor({ state: 'visible' });
                const isMac = await page.evaluate(() => /mac/i.test(navigator.platform));
                const openPalette = () => page.keyboard.press(isMac ? 'Meta+k' : 'Control+k');
                const input = page.getByTestId('command-palette-input');
                const panel = page.locator('.task-panel');

                // j focuses the first row, Enter opens it.
                await page.locator('body').click({ position: { x: 5, y: 5 } });
                await page.keyboard.press('j');
                expect(await page.evaluate(() => document.activeElement?.hasAttribute('data-task-row'))).toBe(true);
                await page.keyboard.press('Enter');
                await expect.poll(() => panel.textContent()).toContain('Keyboard reachable task');

                // The open task's actions run through the panel and save the file.
                await openPalette();
                await input.fill('status done');
                await page.getByTestId('command-palette-option-task-status-Done').waitFor({ state: 'visible' });
                await input.press('Enter');
                await expect.poll(() => workspace.readTaskYaml('KEY-7')).toContain('status: Done');
                await page.keyboard.press('Escape');
                await panel.waitFor({ state: 'detached' });

                // An empty palette lists the task as recent.
                await openPalette();
                await page.getByTestId('command-palette-option-recent-KEY-7').waitFor({ state: 'visible' });

                // Quick filters apply to the current page.
                await input.fill('overdue');
                await page.getByTestId('command-palette-option-filter-overdue').waitFor({ state: 'visible' });
                await input.press('Enter');
                await expect
                    .poll(() => listQueries.some((query) => new URLSearchParams(query).get('due') === 'overdue'))
                    .toBe(true);

                // j moves through board cards as well.
                await page.goto(`${server.url}/boards`);
                await page.locator('[data-board-card]').first().waitFor({ state: 'visible' });
                await page.locator('body').click({ position: { x: 5, y: 5 } });
                await page.keyboard.press('j');
                expect(await page.evaluate(() => document.activeElement?.hasAttribute('data-board-card'))).toBe(true);
            });
        } finally {
            await server.stop();
            await workspace.dispose();
        }
    });
});
