import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

interface TaskSnapshot {
    id: string;
    title: string;
    modified: string;
    archived?: boolean;
}

async function getTask(url: string, id: string): Promise<TaskSnapshot> {
    const response = await fetch(`${url}/api/tasks/get?id=${id}`);
    const text = await response.text();
    if (!response.ok) throw new Error(`get ${id}: ${response.status} ${text}`);
    return (JSON.parse(text) as { data: TaskSnapshot }).data;
}

describe('DEV-4 archive (actual backend/browser)', () => {
    it('archives from the task panel, shows it read-only, finds it with archived=all and brings it back', async ({
        expect,
    }) => {
        const workspace = await SmokeWorkspace.create({
            name: 'archive-ui-',
            env: { LOTAR_DEFAULT_REPORTER: 'Smoke' },
        });
        try {
            const task = await workspace.addTask('Finished work to file away', {
                args: ['--project=ARC', '--reporter=Smoke'],
            });
            const projectDir = path.dirname(task.filePath);
            const archivedPath = path.join(projectDir, '@archive', path.basename(task.filePath));
            const server = await startLotarServer(workspace, { env: { LOTAR_SSE_READY: '1' } });
            try {
                const before = await getTask(server.url, task.id);
                await withBrowser({}, async (context) => {
                    const page = await context.newPage();
                    try {
                        await page.goto(`${server.url}/?project=ARC`, { waitUntil: 'domcontentloaded' });
                        const title = page
                            .locator('.table-wrap')
                            .getByText('Finished work to file away', { exact: true });
                        await expect.poll(() => title.count()).toBe(1);
                        await title.click();
                        const panel = page.locator('.task-panel');
                        const fieldset = panel.locator('fieldset.task-panel__form-fieldset');
                        await expect.poll(() => panel.getByTestId('task-archive').isVisible()).toBe(true);
                        expect(await fieldset.getAttribute('disabled')).toBeNull();

                        await panel.getByTestId('task-archive').click();
                        await expect.poll(() => fs.pathExists(archivedPath)).toBe(true);
                        expect(await fs.pathExists(task.filePath)).toBe(false);
                        await expect.poll(() => panel.getByTestId('task-archived-banner').isVisible()).toBe(true);
                        expect(await panel.getByTestId('task-panel-archived-badge').isVisible()).toBe(true);
                        expect(await fieldset.getAttribute('disabled')).toBe('');
                        expect(await panel.getByTestId('task-archive').count()).toBe(0);
                        await expect.poll(() => title.count()).toBe(0);

                        const archived = await getTask(server.url, task.id);
                        expect(archived.archived).toBe(true);
                        expect(archived.modified).toBe(before.modified);
                        const rejected = await fetch(`${server.url}/api/tasks/update`, {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({ id: task.id, title: 'Must not change an archived task' }),
                        });
                        expect(rejected.status).toBe(400);
                        await page.screenshot({
                            path: path.join(process.cwd(), 'target/smoke-artifacts/archive/archived-panel.png'),
                            animations: 'disabled',
                        });

                        await page.press('body', 'Escape');
                        await expect.poll(() => panel.count()).toBe(0);
                        const search = page.getByTestId('filter-search');
                        await search.fill('archived=all');
                        await search.press('Enter');
                        await expect.poll(() => title.count()).toBe(1);
                        expect(await page.locator('.table-wrap').getByTestId('task-archived').count()).toBe(1);

                        await title.click();
                        const unarchive = panel.getByTestId('task-unarchive');
                        await expect.poll(() => unarchive.isVisible()).toBe(true);
                        await unarchive.click();
                        await expect.poll(() => fs.pathExists(task.filePath)).toBe(true);
                        expect(await fs.pathExists(archivedPath)).toBe(false);
                        await expect.poll(() => panel.getByTestId('task-archived-banner').count()).toBe(0);
                        await expect.poll(() => fieldset.getAttribute('disabled')).toBeNull();
                        expect((await getTask(server.url, task.id)).archived).toBeUndefined();
                    } finally {
                        await page.close();
                    }
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
