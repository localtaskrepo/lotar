import path from 'node:path';
import type { Page } from '@playwright/test';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const ARTIFACTS = path.join(process.cwd(), 'target/smoke-artifacts/task-view');

async function panelBox(page: Page) {
    const box = await page.locator('aside.task-panel').boundingBox();
    if (!box) throw new Error('task panel is not visible');
    return box;
}

describe('DEV-239 large task view (actual backend/browser)', () => {
    it('switches between the side panel and the large view, and opens in the preferred view', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            name: 'task-view-ui-',
            env: { LOTAR_DEFAULT_REPORTER: 'Smoke' },
        });
        try {
            const task = await workspace.addTask('Offline edits are lost when reconnecting', {
                args: [
                    '--project=VIEW',
                    '--priority=High',
                    '--reporter=Smoke',
                    '--description=Edits made offline disappear after the sync queue reconnects mid-flush.',
                ],
            });
            const server = await startLotarServer(workspace, { env: { LOTAR_SSE_READY: '1' } });
            try {
                await withBrowser({}, async (context) => {
                    const page = await context.newPage();
                    try {
                        await page.setViewportSize({ width: 1440, height: 900 });
                        await page.goto(`${server.url}/?project=VIEW&taskPanel=${task.id}`, {
                            waitUntil: 'domcontentloaded',
                        });
                        const panel = page.locator('aside.task-panel');
                        const toggle = page.getByTestId('task-view-toggle');
                        await expect.poll(() => toggle.isVisible()).toBe(true);
                        expect(await toggle.getAttribute('aria-pressed')).toBe('false');
                        const side = await panelBox(page);
                        expect(side.width).toBeLessThanOrEqual(660);
                        expect(side.x + side.width).toBeGreaterThan(1430);

                        // Large view: wide, centered, with people and fields in a right-hand column.
                        await toggle.click();
                        await expect.poll(async () => (await panelBox(page)).width).toBeGreaterThan(1200);
                        expect(await toggle.getAttribute('aria-pressed')).toBe('true');
                        const large = await panelBox(page);
                        expect(Math.abs(large.x - (1440 - large.x - large.width))).toBeLessThan(4);
                        await expect.poll(() => panel.getAttribute('class')).toContain('task-panel--split');
                        const details = await panel
                            .locator('.task-panel__group legend', { hasText: 'Details' })
                            .boundingBox();
                        const people = await panel.locator('.task-panel__side').boundingBox();
                        expect(details && people).toBeTruthy();
                        expect(people!.x).toBeGreaterThan(details!.x + details!.width / 2);
                        expect(people!.y).toBeLessThan(details!.y + 200);
                        // The title is edited in place as the heading; properties, people and tags
                        // sit labelled in the right column.
                        expect(await panel.locator('.task-panel__title-input').inputValue()).toBe(
                            'Offline edits are lost when reconnecting',
                        );
                        expect((await panel.locator('.task-panel__title h2').boundingBox())?.width).toBeLessThanOrEqual(
                            1,
                        );
                        const sideColumn = panel.locator('.task-panel__side');
                        expect(await sideColumn.locator('legend').first().textContent()).toBe('Properties');
                        expect(await sideColumn.locator('.task-panel__field-label').allTextContents()).toEqual([
                            'Project',
                            'Type',
                            'Status',
                            'Priority',
                            'Due date',
                            'Effort',
                        ]);
                        expect(await sideColumn.locator('.task-panel__tags-section').count()).toBe(1);
                        expect(await panel.locator('.task-panel__main .task-panel__tags-section').count()).toBe(0);
                        await expect.poll(() => sideColumn.locator('.task-panel__ownership-custom').count()).toBe(0);
                        await page.waitForTimeout(300);
                        await page.screenshot({ path: path.join(ARTIFACTS, 'large-1440.png'), animations: 'disabled' });

                        // Narrower screens keep the large view in one column.
                        await page.setViewportSize({ width: 820, height: 900 });
                        await expect
                            .poll(async () => (await panel.locator('.task-panel__side').boundingBox())?.width ?? 0)
                            .toBe(0);
                        await page.screenshot({ path: path.join(ARTIFACTS, 'large-820.png'), animations: 'disabled' });

                        // Moving to another view and back keeps the choice until the panel closes.
                        // The command palette switches views too.
                        await page.setViewportSize({ width: 1440, height: 900 });
                        await page.keyboard.press(process.platform === 'darwin' ? 'Meta+k' : 'Control+k');
                        await page.keyboard.type('side panel');
                        await expect
                            .poll(() => page.getByRole('option', { name: /Switch to side panel/ }).count())
                            .toBe(1);
                        await page.keyboard.press('Enter');
                        await expect.poll(async () => (await panelBox(page)).width).toBeLessThanOrEqual(660);
                        await page.getByRole('button', { name: 'Close panel' }).click();
                        await expect.poll(() => panel.count()).toBe(0);

                        // Preferences choose the starting view for existing and new tasks.
                        await page.goto(`${server.url}/preferences`, { waitUntil: 'domcontentloaded' });
                        await page.getByTestId('preferences-task-view-existing').selectOption('large');
                        await page.goto(`${server.url}/?project=VIEW&taskPanel=${task.id}`, {
                            waitUntil: 'domcontentloaded',
                        });
                        await expect.poll(async () => (await panelBox(page)).width).toBeGreaterThan(1200);
                        await page.getByRole('button', { name: 'Close panel' }).click();
                        await expect.poll(() => panel.count()).toBe(0);
                        await page.getByTestId('global-new-task').click();
                        await expect.poll(() => panel.getByRole('heading', { name: 'Create task' }).count()).toBe(1);
                        expect((await panelBox(page)).width).toBeLessThanOrEqual(660);
                        await page.getByRole('button', { name: 'Close panel' }).click();
                        await expect.poll(() => panel.count()).toBe(0);

                        await page.goto(`${server.url}/preferences`, { waitUntil: 'domcontentloaded' });
                        await page.getByTestId('preferences-task-view-new').selectOption('large');
                        await page.goto(`${server.url}/?project=VIEW`, { waitUntil: 'domcontentloaded' });
                        await page.getByTestId('global-new-task').click();
                        await expect.poll(async () => (await panelBox(page)).width).toBeGreaterThan(1200);
                        await page.waitForTimeout(300);
                        await page.screenshot({
                            path: path.join(ARTIFACTS, 'new-large-1440.png'),
                            animations: 'disabled',
                        });
                        await page.getByRole('button', { name: 'Close panel' }).click();

                        // Phones: both views fill the screen, so the switch is hidden.
                        await page.setViewportSize({ width: 390, height: 844 });
                        await page.goto(`${server.url}/?project=VIEW&taskPanel=${task.id}`, {
                            waitUntil: 'domcontentloaded',
                        });
                        await expect.poll(async () => (await panelBox(page)).width).toBe(390);
                        await expect
                            .poll(() => panel.locator('h2').first().textContent())
                            .toBe('Offline edits are lost when reconnecting');
                        expect(await toggle.isVisible()).toBe(false);
                        const overflow = await page.evaluate(
                            () => document.documentElement.scrollWidth - window.innerWidth,
                        );
                        expect(overflow).toBeLessThanOrEqual(0);
                        await page.screenshot({ path: path.join(ARTIFACTS, 'large-390.png'), animations: 'disabled' });
                    } finally {
                        await page.close();
                    }
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
