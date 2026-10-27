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

    // DEV-188: leaving the description editor shrinks it (the toggle's
    // reserved padding goes away). Pressing "Create task" right below used to
    // collapse the editor on mouse-down, so the button moved away before the
    // mouse-up and the first click did nothing.
    it('creates the task on the first click straight from the description editor', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();
        try {
            await workspace.addTask('Existing task');
            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async (page) => {
                    // Tall enough that the panel does not scroll, so the
                    // footer moves when the editor shrinks.
                    await page.setViewportSize({ width: 1280, height: 1400 });
                    const addRequests: unknown[] = [];
                    page.on('request', (request) => {
                        if (new URL(request.url()).pathname === '/api/tasks/add')
                            addRequests.push(request.postDataJSON());
                    });
                    await page.getByTestId('global-new-task').click();
                    const panel = page.locator('.task-panel');
                    const create = panel.getByRole('button', { name: 'Create task', exact: true });
                    await expect.poll(() => create.isEnabled(), { timeout: 15_000 }).toBe(true);
                    await panel.locator('input[placeholder="Title"]').fill('Created from the description');
                    await panel.locator('.task-panel__description-display').click();
                    const textarea = panel.locator('.task-panel__description-body textarea');
                    await textarea.fill('Some details');
                    expect(await textarea.evaluate((el) => el === document.activeElement)).toBe(true);

                    // Hold the button like a person does; an instant click
                    // releases before the editor collapses.
                    await create.click({ delay: 150 });
                    await expect.poll(() => addRequests.length, { timeout: 10_000 }).toBe(1);
                    expect(addRequests[0]).toMatchObject({
                        title: 'Created from the description',
                        description: 'Some details',
                    });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
