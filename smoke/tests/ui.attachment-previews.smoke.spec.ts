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
// Same picture, different bytes: uploads are stored by content hash, so
// identical files would collapse into one attachment.
const pngVariant = (tag: number) => Buffer.concat([Buffer.from(PNG_BASE64, 'base64'), Buffer.from([tag])]);
const PDF_BASE64 = Buffer.from('%PDF-1.4\n%%EOF\n').toString('base64');

async function upload(serverUrl: string, id: string, filename: string, contentBase64: string) {
    const response = await fetch(`${serverUrl}/api/tasks/attachments/upload`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id, filename, content_base64: contentBase64 }),
    });
    expect(response.ok).toBe(true);
    const body = (await response.json()) as { data: { stored_path: string } };
    return body.data.stored_path;
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

    it('attaches with the picker and by pasting, skips broken covers, and serves files safely (DEV-216)', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const empty = await workspace.addTask('Needs a screenshot');
            const mixed = await workspace.addTask('Has a broken image');
            const server = await startLotarServer(workspace);
            try {
                await upload(server.url, mixed.id, 'broken.png', Buffer.from('not a png').toString('base64'));
                const stored = await upload(server.url, mixed.id, 'ok.png', PNG_BASE64);

                // Content-addressed URLs are cacheable for good and never sniffed.
                const hash = stored.split('.').at(-2);
                const project = mixed.id.split('-')[0];
                const served = await fetch(`${server.url}/api/attachments/h/${hash}/ok.png?project=${project}`);
                expect(served.headers.get('cache-control')).toContain('immutable');
                expect(served.headers.get('x-content-type-options')).toBe('nosniff');
                const etag = served.headers.get('etag') ?? '';
                const revalidated = await fetch(`${server.url}/api/attachments/h/${hash}/ok.png?project=${project}`, {
                    headers: { 'If-None-Match': etag },
                });
                expect(revalidated.status).toBe(304);

                await withPage(`${server.url}/boards`, async (page) => {
                    // The broken first image is skipped for the next one.
                    const card = page.locator('[data-board-card]', { hasText: mixed.id });
                    await expect(card.locator('.task-cover img')).toHaveAttribute('src', /ok\.png/, {
                        timeout: 15_000,
                    });
                });

                await withPage(`${server.url}/?taskPanel=${empty.id}`, async (page) => {
                    await expect(page.locator('.task-panel__attachments-empty')).toBeVisible({ timeout: 15_000 });
                    await page.locator('[data-testid="attachment-picker"]').setInputFiles({
                        name: 'picked.png',
                        mimeType: 'image/png',
                        buffer: pngVariant(1),
                    });
                    const gallery = page.locator('.task-panel__gallery-image');
                    await expect(gallery).toHaveCount(1, { timeout: 15_000 });
                    await expect(gallery).toHaveAttribute('alt', 'picked.png');

                    // A pasted screenshot gets a findable name instead of "image.png".
                    await page.locator('.task-panel').evaluate((panel, base64) => {
                        const bytes = Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
                        const data = new DataTransfer();
                        data.items.add(new File([bytes], 'image.png', { type: 'image/png' }));
                        panel.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true }));
                    }, pngVariant(2).toString('base64'));
                    await expect(gallery).toHaveCount(2, { timeout: 15_000 });
                    await expect(gallery.nth(1)).toHaveAttribute('alt', /^pasted-\d{4}-\d{2}-\d{2}-\d{6}\.png$/);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('views, reorders, undoes removals and links pasted screenshots (DEV-216 round 2)', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const task = await workspace.addTask('Gallery work');
            const server = await startLotarServer(workspace);
            try {
                const first = await upload(server.url, task.id, 'first.png', pngVariant(3).toString('base64'));
                const second = await upload(server.url, task.id, 'second.png', pngVariant(4).toString('base64'));
                await upload(server.url, task.id, 'notes.txt', Buffer.from('hello notes').toString('base64'));
                const project = task.id.split('-')[0];

                // HEAD answers like GET without a body; previews carry their own ETag.
                const hash = first.split('.').at(-2);
                const head = await fetch(`${server.url}/api/attachments/h/${hash}/first.png?project=${project}`, {
                    method: 'HEAD',
                });
                expect(head.status).toBe(200);
                expect(head.headers.get('accept-ranges')).toBe('bytes');
                const preview = await fetch(
                    `${server.url}/api/attachments/h/${hash}/first.png?project=${project}&w=96`,
                );
                expect(preview.headers.get('etag')).toBe(`"${hash}-w96"`);

                const listed = await fetch(`${server.url}/api/tasks/list?has=image`);
                const body = (await listed.json()) as { data: { tasks: Array<{ id: string }> } };
                expect(body.data.tasks.map((item) => item.id)).toContain(task.id);

                await withPage(`${server.url}/?taskPanel=${task.id}`, async (page) => {
                    const tiles = page.locator('.task-panel__gallery-item');
                    await expect(tiles).toHaveCount(2, { timeout: 15_000 });
                    await expect(tiles.first().locator('.task-panel__cover-badge')).toBeVisible();

                    // Make the second image the cover; the order is saved.
                    await tiles.nth(1).hover();
                    await tiles.nth(1).getByRole('button', { name: 'Use as cover' }).click();
                    await expect(page.locator('.task-panel__gallery-name').first()).toHaveText('second.png');
                    await expect
                        .poll(async () => {
                            const response = await fetch(`${server.url}/api/tasks/get?id=${task.id}`);
                            const data = (await response.json()) as {
                                data: { references: Array<{ attachment?: string }> };
                            };
                            return data.data.references.find((reference) => reference.attachment)?.attachment;
                        })
                        .toBe(second);

                    // The viewer opens in place and Esc closes only the viewer.
                    await page.locator('.task-panel__gallery-link').first().click();
                    const viewer = page.getByRole('dialog', { name: 'Attachment second.png' });
                    await expect(viewer).toBeVisible();
                    await expect(viewer.locator('.attachment-viewer__count')).toHaveText('1 / 3');
                    await page.keyboard.press('ArrowLeft');
                    await expect(page.getByRole('dialog', { name: 'Attachment notes.txt' })).toBeVisible();
                    await expect(page.locator('.attachment-viewer__text')).toHaveText('hello notes');
                    await page.keyboard.press('Escape');
                    await expect(page.locator('.attachment-viewer')).toHaveCount(0);
                    await expect(page.locator('.task-panel')).toBeVisible();

                    // Removal waits for Undo.
                    const chip = page.locator('.task-panel__attachment-chip', { hasText: 'notes.txt' });
                    await chip.getByRole('button', { name: 'Remove attachment' }).click();
                    await expect(chip).toHaveCount(0);
                    await page.getByRole('button', { name: 'Undo' }).click();
                    await expect(chip).toHaveCount(1);

                    // A screenshot pasted into the description is attached and linked.
                    await page.locator('.task-panel__description-display').click();
                    const textarea = page.locator('.task-panel__description-body textarea');
                    await textarea.focus();
                    await textarea.evaluate((element, base64) => {
                        const bytes = Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
                        const data = new DataTransfer();
                        data.items.add(new File([bytes], 'image.png', { type: 'image/png' }));
                        element.dispatchEvent(
                            new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
                        );
                    }, pngVariant(5).toString('base64'));
                    await expect(textarea).toHaveValue(
                        /^!\[pasted-[\d-]+\.png\]\(attachment:pasted-[\d-]+\.[0-9a-f]{32}\.png\)$/,
                        {
                            timeout: 15_000,
                        },
                    );
                    await expect(tiles).toHaveCount(3);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
