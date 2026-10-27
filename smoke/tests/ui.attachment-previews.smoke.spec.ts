import { expect } from '@playwright/test';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-204: image attachments show up as board card covers, list thumbnails,
// calendar/hover previews and a task panel gallery, and the Attachments field
// in each view's field menu turns them off.

// 2x2 red PNG.
const PNG_BASE64 =
    'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAFklEQVR4nGP8z8DAwMDAxMDAwMDAAAANHQEDasKb6QAAAABJRU5ErkJggg==';
const PDF_BASE64 = Buffer.from('%PDF-1.4\n%%EOF\n').toString('base64');

async function upload(serverUrl: string, id: string, filename: string, contentBase64: string) {
    const response = await fetch(`${serverUrl}/api/tasks/attachments/upload`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id, filename, content_base64: contentBase64 }),
    });
    expect(response.ok).toBe(true);
}

describe('UI attachment previews (DEV-204)', () => {
    it('previews images on cards, in the list and the task panel, and the field toggle hides them', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const task = await workspace.addTask('Screenshot of the bug');
            const server = await startLotarServer(workspace);
            try {
                await upload(server.url, task.id, 'screen.png', PNG_BASE64);
                await upload(server.url, task.id, 'spec.pdf', PDF_BASE64);

                await withPage(`${server.url}/boards`, async (page) => {
                    const card = page.locator('[data-board-card]', { hasText: task.id });
                    const cover = card.locator('.task-cover img');
                    await expect(cover).toBeVisible({ timeout: 15_000 });
                    // The image loads lazily, so wait for it rather than reading it once.
                    await expect
                        .poll(() => cover.evaluate((img: HTMLImageElement) => img.naturalWidth), { timeout: 15_000 })
                        .toBe(2);
                    await expect(card.locator('.task-meta__attachments')).toHaveText('2');

                    await page.getByRole('button', { name: 'Configure card fields' }).click();
                    await page
                        .getByRole('dialog', { name: 'Card fields' })
                        .locator('label', { hasText: 'Attachments' })
                        .locator('input')
                        .uncheck();
                    await expect(card.locator('.task-cover')).toHaveCount(0);
                    await expect(card.locator('.task-meta__attachments')).toHaveCount(0);
                });

                await withPage(`${server.url}/`, async (page) => {
                    const cell = page
                        .locator('tr', { hasText: 'Screenshot of the bug' })
                        .locator('.task-table__cell--attachments');
                    await expect(cell.locator('.attachment-tile')).toHaveCount(2, { timeout: 15_000 });
                    await expect(cell.locator('img.attachment-tile')).toHaveCount(1);
                    await expect(cell.locator('.attachment-tile--pdf')).toHaveText('PDF');
                });

                await withPage(`${server.url}/?taskPanel=${task.id}`, async (page) => {
                    const gallery = page.locator('.task-panel__gallery-image');
                    await expect(gallery).toHaveCount(1, { timeout: 15_000 });
                    await expect(gallery).toHaveAttribute('alt', 'screen.png');
                    const chips = page.locator('.task-panel__attachment-chip');
                    await expect(chips).toHaveCount(1);
                    await expect(chips.locator('.attachment-tile--pdf')).toHaveText('PDF');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
