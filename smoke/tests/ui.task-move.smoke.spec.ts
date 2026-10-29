import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe('DEV-183 move a task to another project (actual backend/browser)', () => {
    it('moves from the task panel and opens old IDs under the new one', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            name: 'task-move-ui-',
            env: { LOTAR_DEFAULT_REPORTER: 'Smoke' },
        });
        try {
            await workspace.addTask('Existing web task', { args: ['--project=WEB', '--reporter=Smoke'] });
            const task = await workspace.addTask('Belongs on the website', {
                args: ['--project=APP', '--priority=High', '--reporter=Smoke'],
            });
            const server = await startLotarServer(workspace, { env: { LOTAR_SSE_READY: '1' } });
            try {
                await withBrowser({}, async (context) => {
                    const page = await context.newPage();
                    try {
                        await page.setViewportSize({ width: 1440, height: 900 });
                        await page.goto(`${server.url}/?taskPanel=${task.id}`, { waitUntil: 'domcontentloaded' });
                        const panel = page.locator('aside.task-panel');
                        await expect
                            .poll(() => panel.locator('h2').first().textContent())
                            .toBe('Belongs on the website');

                        // Choosing another project opens the move dialog with a preview.
                        const projectSelect = panel.locator(
                            'select[title="Choose another project to move the task there"]',
                        );
                        await projectSelect.selectOption('WEB');
                        const dialog = page.getByTestId('task-move-dialog');
                        await expect.poll(() => dialog.isVisible()).toBe(true);
                        await expect
                            .poll(() => dialog.getByTestId('task-move-new-id').textContent())
                            .toContain('WEB-2');
                        expect(await projectSelect.inputValue()).toBe('APP');
                        await page.screenshot({
                            path: path.join(process.cwd(), 'target/smoke-artifacts/task-move/dialog.png'),
                            animations: 'disabled',
                        });

                        await dialog.getByTestId('task-move-submit').click();
                        await expect.poll(() => new URL(page.url()).searchParams.get('taskPanel')).toBe('WEB-2');
                        await expect.poll(() => panel.locator('.task-panel__id').textContent()).toBe('WEB-2');
                        const stub = await fs.readFile(task.filePath, 'utf8');
                        expect(stub).toContain('moved_to: WEB-2');

                        // A link or bookmark with the old ID lands on the task's new ID.
                        await page.goto(`${server.url}/?taskPanel=${task.id}`, { waitUntil: 'domcontentloaded' });
                        await expect.poll(() => new URL(page.url()).searchParams.get('taskPanel')).toBe('WEB-2');
                        await expect
                            .poll(() => panel.locator('h2').first().textContent())
                            .toBe('Belongs on the website');
                        // The task list shows it once, under its new ID.
                        await page.press('body', 'Escape');
                        const rows = page.locator('.table-wrap').getByText('Belongs on the website', { exact: true });
                        await expect.poll(() => rows.count()).toBe(1);
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
