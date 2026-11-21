import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { assigneePickerValues, withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-545: the task panel suggests the people of the project's tasks from
// `/api/tasks/values`, not from a list of 200 whole tasks.
describe('UI task panel suggestions', () => {
    it('offers the names of other tasks without listing whole tasks', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();
        try {
            const opened = await workspace.addTask('Opened task', { args: ['--assignee=opened@example.com'] });
            await workspace.addTask('Other task', { args: ['--assignee=elsewhere@example.com'] });
            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async (page) => {
                    const requests: string[] = [];
                    page.on('request', (request) => {
                        const url = new URL(request.url());
                        if (url.pathname.startsWith('/api/tasks/')) requests.push(url.pathname + url.search);
                    });
                    await page.goto(`${server.url}/task/${opened.id}`);
                    await page.locator('#task-panel-assignee-select').waitFor();
                    await expect
                        .poll(() => assigneePickerValues(page, '#task-panel-assignee-select'))
                        .toContain('elsewhere@example.com');
                    expect(requests.some((entry) => entry.startsWith('/api/tasks/values'))).toBe(true);
                    expect(requests.filter((entry) => entry.includes('limit=200'))).toEqual([]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
