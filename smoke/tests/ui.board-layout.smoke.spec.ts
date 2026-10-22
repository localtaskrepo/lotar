import { describe, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-130: one Columns dialog arranges the board (order, stacking any
// statuses, hiding, per-column WIP limits and card rules); stacked statuses
// keep their own sections and drop targets; the layout persists.
const STATES = ['Todo', 'InProgress', 'NeedsReview', 'QA', 'Done', 'WontDo'];

function seed(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': 'default.project: LAY\ndefault.reporter: Fixture\nauto.set_reporter: false\nauto.assign_on_status: false\n',
        '.tasks/LAY/config.yml': stringify({ project: { name: 'Layout' }, issue: { states: STATES, done_states: ['Done', 'WontDo'] } }),
    };
    STATES.forEach((status, index) => {
        files[`.tasks/LAY/${index + 1}.yml`] = stringify({
            title: `${status} card`, status, priority: index % 2 ? 'High' : 'Low', type: 'Feature', reporter: 'Fixture',
            created: '2026-10-01T00:00:00Z', modified: `2026-10-01T00:00:0${index}Z`,
        });
    });
    return files;
}

describe.concurrent('UI board columns dialog (DEV-130)', () => {
    it('arranges, stacks, hides and limits columns, drops into stacked sections, and persists', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'ui-board-layout-', seedFiles: seed() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/boards?project=LAY`, async page => {
                    await page.setViewportSize({ width: 1280, height: 900 });
                    const lanes = () => page.locator('.board.grid > .col.column .lane-header__title').allTextContents();
                    await expect.poll(lanes).toEqual(STATES);

                    // The grouping control matches the button height.
                    const heights = await page.evaluate(() => [
                        document.querySelector('[data-testid="board-groupby"]')!.getBoundingClientRect().height,
                        document.querySelector('[data-testid="board-columns-button"]')!.getBoundingClientRect().height,
                    ]);
                    expect(Math.abs(heights[0]! - heights[1]!)).toBeLessThanOrEqual(1);

                    await page.getByTestId('board-columns-button').click();
                    const dialog = page.getByRole('dialog', { name: 'Board columns' });
                    await dialog.waitFor();
                    // Stack non-adjacent statuses through the status menus: WontDo with Todo, QA with NeedsReview.
                    const moveVia = async (status: string, item: string) => {
                        await dialog.getByLabel(`Move ${status}`, { exact: true }).click();
                        await dialog.getByRole('button', { name: item, exact: true }).click();
                    };
                    await moveVia('WontDo', 'Move to Todo');
                    await moveVia('QA', 'Move to NeedsReview');
                    await dialog.getByLabel('Hide InProgress').click();
                    await dialog.getByLabel('Rename column NeedsReview · QA').click();
                    await dialog.getByLabel('Name for column 3').fill('Review');
                    await dialog.getByLabel('Name for column 3').press('Enter');
                    const columnTitles = () => dialog.locator('.board-columns-dialog__title').allTextContents();
                    expect(await columnTitles()).toEqual(['Todo · WontDo', 'InProgress', 'Review', 'Done']);
                    // Drag the Done column by its header onto the left half of InProgress, then back to the end.
                    const columnNamed = (title: string) => dialog.locator('.board-columns-dialog__column')
                        .filter({ has: page.locator('.board-columns-dialog__title').getByText(title, { exact: true }) });
                    const header = (title: string) => columnNamed(title).locator('.board-columns-dialog__column-head');
                    await header('Done').dragTo(columnNamed('InProgress'), { targetPosition: { x: 8, y: 80 } });
                    await expect.poll(columnTitles).toEqual(['Todo · WontDo', 'Done', 'InProgress', 'Review']);
                    const reviewBox = (await columnNamed('Review').boundingBox())!;
                    await header('Done').dragTo(columnNamed('Review'), { targetPosition: { x: reviewBox.width - 10, y: 80 } });
                    await expect.poll(columnTitles).toEqual(['Todo · WontDo', 'InProgress', 'Review', 'Done']);
                    const reviewColumn = dialog.locator('.board-columns-dialog__column').nth(2);
                    await reviewColumn.getByLabel('WIP limit').fill('1');
                    await dialog.getByTestId('board-columns-apply').click();
                    await dialog.waitFor({ state: 'detached' });

                    await expect.poll(lanes).toEqual(['Todo · WontDo', 'Review', 'Done']);
                    expect(await page.getByTestId('board-hidden-columns').textContent()).toContain('1 column hidden (InProgress) · 1 task');
                    const review = page.locator('.board.grid > .col.column', { hasText: 'Review' }).first();
                    expect(await review.locator('.lane-header__count').textContent()).toContain('2 / 1');
                    expect(await review.evaluate(el => el.classList.contains('over-limit'))).toBe(true);

                    // Once the card move animation settles, the lanes fit without horizontal scroll.
                    await expect.poll(() => page.locator('.board.grid').evaluate(el => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(1);

                    // Drop the Done card into the WontDo section of the stacked Todo lane.
                    const card = page.locator('article.task', { hasText: 'Done card' });
                    await card.dispatchEvent('dragstart');
                    await page.locator('.column--stacked .column-section[data-status="WontDo"] .col-cards').dispatchEvent('drop');
                    await expect.poll(async () => parse(await workspace.readTaskYaml('LAY-5')).status).toBe('WontDo');

                    await page.reload();
                    await expect.poll(lanes).toEqual(['Todo · WontDo', 'Review', 'Done']);

                    // Priority swimlanes: one compact header row, per-status sections in stacked cells.
                    await page.getByTestId('board-groupby').selectOption('priority');
                    await expect.poll(() => page.locator('.column-group-cell--stacked').count()).toBeGreaterThan(0);
                    const headerHeights = await page.locator('.board-col-header').evaluateAll(nodes => nodes.map(n => Math.round(n.getBoundingClientRect().height)));
                    expect(new Set(headerHeights).size).toBe(1);
                    expect(await page.locator('.column-group-cell--stacked').first().locator('.column-section__label').allTextContents()).toHaveLength(2);

                    await page.getByTestId('board-groupby').selectOption('none');
                    await page.getByTestId('board-hidden-columns').getByRole('button', { name: 'Show all' }).click();
                    await expect.poll(lanes).toEqual(['Todo · WontDo', 'InProgress', 'Review', 'Done']);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
