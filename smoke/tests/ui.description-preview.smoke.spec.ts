import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-145: the description "Preview" toggle used to float over the top-right
// corner of the textarea and cover the first line of text. It now sits in the
// editor's bottom-right corner, in padding the textarea reserves for it, so it
// stays attached to the box without covering text or moving the Tags section.
describe.concurrent('UI description preview toggle', () => {
    for (const width of [1280, 390]) {
        it(`stays clear of the text and the layout at ${width}px`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create();
            try {
                const created = await workspace.addTask('Description preview layout task');
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/task/${created.id}`, async (page) => {
                        await page.setViewportSize({ width, height: 900 });
                        const display = page.locator('.task-panel__description-display');
                        await display.waitFor({ timeout: 15_000 });

                        await display.click();
                        const textarea = page.locator('.task-panel__description-body textarea');
                        await textarea.fill(
                            Array.from(
                                { length: 6 },
                                (_, i) =>
                                    `Line ${i + 1} is long enough to reach the right edge of the description editor on every viewport width`,
                            ).join('\n'),
                        );
                        const toggle = page.getByTestId('description-preview-toggle');
                        const box = (await textarea.boundingBox())!;
                        const button = (await toggle.boundingBox())!;

                        const paddingBottom = await textarea.evaluate((el) =>
                            parseFloat(getComputedStyle(el).paddingBottom),
                        );
                        // Inside the box, hugging its bottom-right corner.
                        expect(box.x + box.width - (button.x + button.width)).toBeGreaterThanOrEqual(0);
                        expect(box.x + box.width - (button.x + button.width)).toBeLessThan(8);
                        expect(box.y + box.height - (button.y + button.height)).toBeGreaterThanOrEqual(0);
                        expect(box.y + box.height - (button.y + button.height)).toBeLessThan(8);
                        // Below the last line of text (in the reserved padding).
                        expect(button.y).toBeGreaterThanOrEqual(box.y + box.height - paddingBottom - 1);
                        // The toggle takes no layout space: the next section (the
                        // attachments drop hint) keeps its normal distance below
                        // the description editor.
                        const spacing = await page.evaluate(() => {
                            const description = document.querySelector('.task-panel__description')!;
                            const next = description.nextElementSibling!;
                            return {
                                actual: next.getBoundingClientRect().top - description.getBoundingClientRect().bottom,
                                gap:
                                    parseFloat(getComputedStyle(description.parentElement!).rowGap) +
                                    parseFloat(getComputedStyle(next).marginTop),
                            };
                        });
                        expect(spacing.actual).toBeCloseTo(spacing.gap, 0);

                        await toggle.click();
                        await page.locator('.task-panel__description-preview').waitFor();
                        await expect.poll(() => toggle.textContent()).toContain('Hide preview');
                    });
                } finally {
                    await server.stop();
                }
            } finally {
                await workspace.dispose();
            }
        });
    }
});
