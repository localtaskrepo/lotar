import type { Locator } from 'playwright';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-114: sprint and backlog tables order rows with the shared task sort
// contract (missing due dates last when ascending, first when descending,
// canonical ID ascending for ties in both directions).
function seedFiles(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': 'default.project: SORT\ndefault.reporter: Fixture\nauto.set_reporter: false\n',
        '.tasks/SORT/config.yml': 'project.name: Sorted backlog\n',
        '.tasks/@sprints/1.yml': stringify({ created: '2026-10-01T00:00:00Z', plan: { label: 'Sorted sprint' }, tasks: ['SORT-5', 'SORT-6', 'SORT-7'] }),
    };
    const due: Array<string | null> = [null, '2026-03-01', '2026-02-01', '2026-02-01', null, '2026-04-01', '2026-01-01'];
    due.forEach((value, index) => {
        const number = index + 1;
        files[`.tasks/SORT/${number}.yml`] = stringify({
            title: `Sort task ${number}`, status: 'Todo', priority: 'Medium', type: 'Feature', reporter: 'Fixture',
            created: '2026-10-01T00:00:00Z', modified: '2026-10-01T00:00:00Z',
            ...(value ? { due_date: value } : {}), sprints: number > 4 ? [1] : [], history: [],
        });
    });
    return files;
}

describe.concurrent('UI sprint and backlog automatic sorting (DEV-114)', () => {
    it('orders backlog and sprint rows by the shared due-date contract in both directions', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'ui-sprints-sort-', seedFiles: seedFiles() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage('about:blank', async page => {
                    await page.addInitScript(() => {
                        if (localStorage.getItem('sprints-sort-fixture') === 'ready') return;
                        localStorage.setItem('sprints-sort-fixture', 'ready');
                        localStorage.setItem('lotar.sprints.window.v2', 'all');
                        localStorage.setItem('lotar.sprints.sort', JSON.stringify({ key: 'due_date', dir: 'asc' }));
                    });
                    await page.goto(`${server.url}/sprints`);
                    const backlog = page.locator('[data-sprint-id="backlog"]');
                    const sprint = page.locator('[data-sprint-id="1"]');
                    const ids = (group: Locator) => group.locator('tr.task-row').evaluateAll(rows => rows.map(row => row.getAttribute('data-task-id')));

                    await expect.poll(() => ids(backlog)).toEqual(['SORT-3', 'SORT-4', 'SORT-2', 'SORT-1']);
                    await expect.poll(() => ids(sprint)).toEqual(['SORT-7', 'SORT-6', 'SORT-5']);

                    await backlog.locator('th[data-column="due_date"] button').click();
                    await expect.poll(() => ids(backlog)).toEqual(['SORT-1', 'SORT-2', 'SORT-3', 'SORT-4']);
                    await expect.poll(() => ids(sprint)).toEqual(['SORT-5', 'SORT-6', 'SORT-7']);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
