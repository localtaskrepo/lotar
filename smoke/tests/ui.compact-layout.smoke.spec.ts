import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-419: on a phone the board shows one column at a time and the task list
// shows rows; Preferences can switch back to the classic wide layout.
function seed(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': 'default.project: CMP\n',
        '.tasks/CMP/config.yml': stringify({
            issue: { states: ['Todo', 'InProgress', 'Done'], priorities: ['Low', 'High'], done_states: ['Done'] },
        }),
    };
    const tasks = [
        { title: 'Charlie card with a title long enough to wrap on a phone', status: 'Todo', priority: 'High' },
        { title: 'Alpha card', status: 'Todo', priority: 'Low' },
        { title: 'Bravo card', status: 'InProgress', priority: 'Low' },
    ];
    tasks.forEach((task, index) => {
        files[`.tasks/CMP/${index + 1}.yml`] = stringify({
            ...task,
            type: 'Feature',
            reporter: 'Fixture',
            assignee: 'robin',
            tags: ['phone', 'layout'],
            created: '2026-10-01T00:00:00Z',
            modified: `2026-10-0${index + 1}T00:00:00Z`,
        });
    });
    return files;
}

describe.concurrent('Compact phone layout (DEV-419)', () => {
    it('shows one board column at a time, moves cards from a sheet, and lists tasks as rows', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
        const artifacts = path.resolve('target/smoke-artifacts/compact-layout');
        await fs.ensureDir(artifacts);
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/boards?project=CMP`, async (page) => {
                    await page.setViewportSize({ width: 390, height: 844 });
                    const ids = () => page.locator('article.task .id').allTextContents();
                    const tabs = page.getByRole('tab');
                    const pageWidth = () => page.evaluate(() => document.documentElement.scrollWidth);

                    await expect.poll(() => tabs.allTextContents()).toEqual(['Todo2', 'InProgress1', 'Done0']);
                    await expect.poll(ids).toEqual(['CMP-2', 'CMP-1']);
                    expect(await pageWidth()).toBeLessThanOrEqual(390);
                    await page.screenshot({ path: path.join(artifacts, 'board.png') });

                    // A tab, the arrow keys and a sideways swipe each change the column.
                    await tabs.filter({ hasText: 'InProgress' }).click();
                    await expect.poll(ids).toEqual(['CMP-3']);
                    await page.keyboard.press('ArrowLeft');
                    await expect.poll(ids).toEqual(['CMP-2', 'CMP-1']);
                    await page.getByRole('tabpanel').evaluate((panel) => {
                        const touch = (x: number) =>
                            new Touch({ identifier: 1, target: panel, clientX: x, clientY: 400 });
                        panel.dispatchEvent(new TouchEvent('touchstart', { touches: [touch(300)], bubbles: true }));
                        panel.dispatchEvent(
                            new TouchEvent('touchend', { changedTouches: [touch(100)], bubbles: true }),
                        );
                    });
                    await expect.poll(ids).toEqual(['CMP-3']);
                    await tabs.filter({ hasText: 'Todo' }).click();

                    // The card's ⋯ sheet moves it without dragging.
                    await page
                        .locator('[data-board-card]', { hasText: 'CMP-1' })
                        .getByTestId('board-card-actions')
                        .click();
                    const sheet = page.getByTestId('board-card-actions-dialog');
                    await sheet.waitFor({ state: 'visible' });
                    await page.screenshot({ path: path.join(artifacts, 'card-actions.png') });
                    await sheet.getByRole('button', { name: 'Done' }).click();
                    await expect.poll(() => tabs.allTextContents()).toEqual(['Todo1', 'InProgress1', 'Done1']);
                    await expect.poll(ids).toEqual(['CMP-2']);

                    // Task list: rows without the header, sorted from the sort bar.
                    await page.goto(`${server.url}/?project=CMP`);
                    const titles = () => page.locator('[data-task-row] .task-table__title-text').allTextContents();
                    await expect.poll(titles).toHaveLength(3);
                    expect(await page.locator('.task-table--compact thead').isVisible()).toBe(false);
                    await page.getByTestId('task-table-sort').selectOption('title');
                    await expect
                        .poll(titles)
                        .toEqual([
                            'Alpha card',
                            'Bravo card',
                            'Charlie card with a title long enough to wrap on a phone',
                        ]);
                    await page.getByTestId('task-table-sortdir').click();
                    await expect
                        .poll(titles)
                        .toEqual([
                            'Charlie card with a title long enough to wrap on a phone',
                            'Bravo card',
                            'Alpha card',
                        ]);
                    expect(await pageWidth()).toBeLessThanOrEqual(390);
                    await page.screenshot({ path: path.join(artifacts, 'tasks.png') });

                    // Classic keeps the wide layouts on a phone.
                    await page.evaluate(() => localStorage.setItem('lotar.preferences.layout.narrow', 'classic'));
                    await page.reload();
                    await expect.poll(titles).toHaveLength(3);
                    expect(await page.locator('.task-table--compact').count()).toBe(0);
                    expect(await page.locator('thead').first().isVisible()).toBe(true);
                    await page.goto(`${server.url}/boards?project=CMP`);
                    await page.locator('.board.grid').waitFor({ state: 'visible' });
                    expect(await page.getByTestId('board-compact').count()).toBe(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
