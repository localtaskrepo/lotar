import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-180: board columns can be sorted by a field (saved per project), and by
// the manual Sprints-page order while the board is filtered to one sprint.
function seed(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': 'default.project: SRT\n',
        '.tasks/SRT/config.yml': stringify({
            issue: { states: ['Todo', 'Done'], priorities: ['Low', 'Medium', 'High'], done_states: ['Done'] },
        }),
        // Manual sprint order 2, 3, 1 differs from every field order below.
        '.tasks/@sprints/1.yml': stringify({
            created: '2026-10-01T00:00:00Z',
            plan: { label: 'Ordered sprint' },
            tasks: ['SRT-2', 'SRT-3', 'SRT-1'],
        }),
    };
    const cards = [
        { priority: 'Medium', title: 'Bravo' },
        { priority: 'High', title: 'Charlie' },
        { priority: 'Low', title: 'Alpha' },
    ];
    cards.forEach((card, index) => {
        files[`.tasks/SRT/${index + 1}.yml`] = stringify({
            ...card,
            status: 'Todo',
            type: 'Feature',
            reporter: 'Fixture',
            created: '2026-10-01T00:00:00Z',
            modified: `2026-10-0${index + 1}T00:00:00Z`,
            sprints: [1],
        });
    });
    return files;
}

describe.concurrent('Board card sorting (DEV-180)', () => {
    it('sorts by field and direction, remembers it, and follows sprint order for one sprint', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ seedFiles: seed() });
        const artifacts = path.resolve('target/smoke-artifacts/board-sort');
        await fs.ensureDir(artifacts);
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/boards?project=SRT`, async (page) => {
                    await page.setViewportSize({ width: 1280, height: 900 });
                    const ids = () => page.locator('article.task .id').allTextContents();
                    const sortBy = page.getByTestId('board-sortby');
                    const direction = page.getByTestId('board-sortdir');
                    const optionValues = () =>
                        sortBy
                            .locator('option')
                            .evaluateAll((options) => options.map((option) => (option as HTMLOptionElement).value));
                    const writes: string[] = [];
                    page.on('request', (request) => {
                        if (['POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method()))
                            writes.push(new URL(request.url()).pathname);
                    });

                    // Default: most recently modified first.
                    await expect.poll(ids).toEqual(['SRT-3', 'SRT-2', 'SRT-1']);
                    expect(await optionValues()).not.toContain('sprint');

                    await sortBy.selectOption('priority');
                    await expect.poll(ids).toEqual(['SRT-2', 'SRT-1', 'SRT-3']);
                    await direction.click();
                    await expect.poll(ids).toEqual(['SRT-3', 'SRT-1', 'SRT-2']);

                    // Let the card move animation settle before capturing.
                    await page.waitForTimeout(400);
                    await page.screenshot({ path: path.join(artifacts, 'priority-asc.png') });
                    await page.setViewportSize({ width: 390, height: 844 });
                    await page.screenshot({ path: path.join(artifacts, 'mobile.png') });
                    await page.setViewportSize({ width: 1280, height: 900 });

                    await sortBy.selectOption('title');
                    await expect.poll(ids).toEqual(['SRT-3', 'SRT-1', 'SRT-2']);

                    // The choice survives a reload.
                    await page.reload();
                    await expect.poll(ids).toEqual(['SRT-3', 'SRT-1', 'SRT-2']);
                    expect(await sortBy.inputValue()).toBe('title');

                    // Filtering to one sprint offers the manual sprint order.
                    await page.evaluate(() =>
                        localStorage.setItem('lotar.boards.filter', JSON.stringify({ project: 'SRT', sprints: '1' })),
                    );
                    // A saved filter is only restored when the URL names no project.
                    await page.goto(`${server.url}/boards`);
                    await expect.poll(optionValues).toContain('sprint');
                    await sortBy.selectOption('sprint');
                    await expect.poll(ids).toEqual(['SRT-2', 'SRT-3', 'SRT-1']);
                    expect(await direction.isDisabled()).toBe(true);
                    await page.screenshot({ path: path.join(artifacts, 'sprint-order.png') });

                    // Sorting never writes tasks or sprints.
                    expect(writes).toEqual([]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
