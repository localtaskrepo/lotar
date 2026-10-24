import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-145: the description "Preview" toggle used to float over the top-right
// corner of the textarea and cover the first line of text. It now sits
// centered below the editor box without moving the Tags section.
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
                            'A first line long enough to reach the right edge of the description editor on every viewport width',
                        );
                        const toggle = page.getByTestId('description-preview-toggle');
                        const box = (await textarea.boundingBox())!;
                        const button = (await toggle.boundingBox())!;

                        expect(button.y).toBeGreaterThanOrEqual(box.y + box.height);
                        expect(Math.abs(button.x + button.width / 2 - (box.x + box.width / 2))).toBeLessThan(2);
                        // The toggle takes no layout space: Tags keeps the normal
                        // fieldset gap below the description editor.
                        const spacing = await page.evaluate(() => {
                            const description = document.querySelector('.task-panel__description')!;
                            const tagsSection = document.querySelector('.task-panel__tags-section')!;
                            return {
                                actual:
                                    tagsSection.getBoundingClientRect().top -
                                    description.getBoundingClientRect().bottom,
                                gap: parseFloat(getComputedStyle(description.parentElement!).rowGap),
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
