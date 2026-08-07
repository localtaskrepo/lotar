import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// Regression coverage for the "edit tags" row action.
//
// Previously the row-menu "Edit tags" only rendered an inline editor that
// required the `tags` column to be visible, so it silently did nothing when the
// column was hidden. It now opens the task panel's tag-management dialog
// regardless of column visibility. This suite reproduces the original failure
// condition (tags column hidden) and asserts the dialog still opens.
describe.concurrent('UI edit-tags row action', () => {
    it('opens the tag-management dialog even when the tags column is hidden', async () => {
        const workspace = await SmokeWorkspace.create();
        const tag = 'smoke-edit-tags';
        const taskTitle = 'Edit Tags Regression Task';

        try {
            await workspace.addTask(taskTitle, { args: ['--tag', tag] });

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector(`text=${taskTitle}`, { timeout: 15_000 });

                    // Sanity: the tags column is visible and shows our tag.
                    await page.waitForSelector('.task-table__cell--tags', { timeout: 15_000 });
                    expect(
                        await page
                            .locator('.task-table__cell--tags', { hasText: tag })
                            .count(),
                    ).toBeGreaterThan(0);

                    // Hide the tags column through the "Configure columns" control
                    // (the exact path a user takes) to reproduce the bug condition.
                    await page.locator('button[title="Configure columns"]').click();
                    await page.waitForSelector('.columns-popover', { timeout: 10_000 });
                    await page
                        .locator('.columns-popover .column-option', { hasText: 'Tags' })
                        .locator('input[type="checkbox"]')
                        .uncheck();
                    await page.click('.columns-popover button:has-text("Close")');

                    // The regression condition: the tags column is gone.
                    await page.waitForSelector('.task-table__cell--tags', {
                        state: 'detached',
                        timeout: 10_000,
                    });
                    expect(await page.locator('.task-table__cell--tags').count()).toBe(0);

                    // Open the row actions menu and trigger "Edit tags".
                    await page.click('button[aria-label="Row actions"]');
                    await page.waitForSelector('.menu-popover', { timeout: 10_000 });
                    await page.click('.menu-popover .menu-item:has-text("Edit tags")');

                    // The fix: the tag-management dialog opens even with the column hidden.
                    await page.waitForSelector('[data-testid="tag-dialog"]', {
                        timeout: 15_000,
                    });
                    const heading = await page
                        .locator('[data-testid="tag-dialog"] h2')
                        .textContent();
                    expect(heading).toBe('Manage tags');

                    // The existing tag travels into the panel context.
                    expect(
                        await page
                            .locator('.task-panel__tag-label', { hasText: tag })
                            .count(),
                    ).toBeGreaterThan(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
