import { relative } from 'node:path';
import { describe, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const comments = [
    { date: '2026-11-13T10:00:00Z', author: 'automation', text: 'Job started for claude.' },
    { date: '2026-11-13T11:00:00Z', author: 'claude', text: 'Implemented the change.' },
    { date: '2026-11-13T12:00:00Z', author: 'mallox', text: 'Looks good.' },
];

const history = [
    { at: '2026-11-13T10:00:00Z', actor: 'automation', changes: [{ field: 'status', old: 'Todo', new: 'InProgress' }] },
    { at: '2026-11-13T11:00:00Z', actor: 'claude', changes: [{ field: 'tags', old: '', new: 'ui' }] },
    { at: '2026-11-13T12:00:00Z', actor: 'mallox', changes: [{ field: 'assignee', old: 'claude', new: '' }] },
];

describe.concurrent('Comments and Timeline tab filters (DEV-502, DEV-518)', () => {
    for (const colorScheme of ['light', 'dark'] as const) {
        it(`hides an author's comments and a field's updates at 390px in the ${colorScheme} theme`, async ({
            expect,
        }) => {
            const workspace = await SmokeWorkspace.create({ name: `ui-activity-filter-${colorScheme}-` });
            try {
                const first = await workspace.addTask('Filter fixture');
                const second = await workspace.addTask('Second filter fixture');
                for (const id of [first.id, second.id]) {
                    const task = parse(await workspace.readTaskYaml(id));
                    task.comments = comments;
                    task.history = history;
                    await workspace.write(relative(workspace.root, workspace.getTaskFilePath(id)), stringify(task));
                }
                const server = await startLotarServer(workspace);
                try {
                    await withPage('about:blank', async (page) => {
                        await page.setViewportSize({ width: 390, height: 844 });
                        await page.emulateMedia({ colorScheme });
                        await page.goto(`${server.url}/task/${first.id}`);
                        await page.getByRole('tab', { name: 'Comments', exact: true }).click();
                        const filter = page.getByRole('group', { name: 'Filter comments' });
                        const automation = filter.getByRole('button', { name: 'automation', exact: true });
                        await expect.poll(() => automation.getAttribute('aria-pressed')).toBe('true');
                        await automation.click();
                        await expect.poll(() => automation.getAttribute('aria-pressed')).toBe('false');
                        await expect.poll(() => page.locator('.task-panel__comment').count()).toBe(2);
                        expect(await filter.getByRole('status').textContent()).toContain('1 of 3 comments hidden.');
                        const fits = await page.evaluate(
                            () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
                        );
                        expect(fits).toBe(true);
                        const styles = await filter
                            .locator('.task-panel__activity-filter-chip')
                            .evaluateAll((chips) => chips.map((chip) => getComputedStyle(chip).textDecorationLine));
                        expect(styles).toEqual(['line-through', 'none', 'none']);

                        await page.getByRole('tab', { name: 'Timeline', exact: true }).click();
                        const timelineFilter = page.getByRole('group', { name: 'Filter timeline' });
                        await timelineFilter.getByRole('button', { name: 'Status', exact: true }).click();
                        await timelineFilter.getByRole('button', { name: 'mallox', exact: true }).click();
                        await expect
                            .poll(() => page.locator('.task-panel__history-change strong').allTextContents())
                            .toEqual(['Tags']);
                        // Status's change empties its entry; mallox's change and comment go too.
                        expect(await timelineFilter.getByRole('status').textContent()).toContain(
                            '3 of 6 entries hidden.',
                        );

                        // The choice is the viewer's and carries over to the next task.
                        await page.goto(`${server.url}/task/${second.id}`);
                        await page.getByRole('tab', { name: 'Comments', exact: true }).click();
                        await expect.poll(() => page.locator('.task-panel__comment').count()).toBe(2);
                        await page.getByRole('button', { name: 'Show all', exact: true }).click();
                        await expect.poll(() => page.locator('.task-panel__comment').count()).toBe(3);
                        await page.getByRole('tab', { name: 'Timeline', exact: true }).click();
                        await expect
                            .poll(() => page.locator('.task-panel__history-change strong').allTextContents())
                            .toEqual(['Tags']);
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
