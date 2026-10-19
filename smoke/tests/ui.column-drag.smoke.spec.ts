import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { html5DragAndDrop, withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

/**
 * DEV-112 browser coverage for column drag reorder on the tasks table (the
 * sprints table already has parity coverage in ui.sprints.smoke.spec.ts).
 * Asserts the reordered headers persist across a reload, exercising the
 * shared useColumnDragReorder composable through the real UI.
 */

async function pollUntil<T>(
    probe: () => Promise<T>,
    done: (value: T) => boolean,
    timeoutMs = 10_000,
    intervalMs = 100,
): Promise<T> {
    const deadline = Date.now() + timeoutMs;
    let last = await probe();
    while (!done(last)) {
        if (Date.now() >= deadline) {
            return last;
        }
        await new Promise((resolve) => setTimeout(resolve, intervalMs));
        last = await probe();
    }
    return last;
}

describe('UI column drag smoke scenarios (DEV-112)', () => {
    it('reorders task table columns via drag and drop and keeps the order after reload', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.addTask('Column drag task', { args: ['--project', 'WEB'] });

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('table.table thead th[data-column]', { timeout: 15_000 });

                    const headerOrder = async () =>
                        page.$$eval('table.table thead th[data-column]', (nodes: Element[]) =>
                            nodes.map((node) => (node as HTMLElement).dataset.column || ''),
                        );

                    const before = await headerOrder();
                    expect(before.includes('status')).toBe(true);
                    expect(before.includes('priority')).toBe(true);
                    expect(before.indexOf('priority')).toBeGreaterThan(before.indexOf('status'));

                    await html5DragAndDrop(
                        page,
                        'table.table th[data-column="priority"] button.header-button',
                        'table.table th[data-column="status"] button.header-button',
                    );

                    const after = await pollUntil(
                        headerOrder,
                        (order) => order.indexOf('priority') < order.indexOf('status'),
                    );
                    expect(after.indexOf('priority')).toBeLessThan(after.indexOf('status'));

                    await page.reload({ waitUntil: 'domcontentloaded' });
                    await page.waitForSelector('table.table thead th[data-column]', { timeout: 15_000 });

                    const reloaded = await pollUntil(
                        headerOrder,
                        (order) => order.indexOf('priority') < order.indexOf('status'),
                    );
                    expect(reloaded.indexOf('priority')).toBeLessThan(reloaded.indexOf('status'));
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
