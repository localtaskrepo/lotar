import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('SyncHub required empty report list', () => {
    it('renders an empty array as No reports yet without an error in each scope', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            name: 'sync-empty-reports-',
            seedFiles: {
                '.tasks/config.yml': 'remotes: {}\n',
                '.tasks/RPT/config.yml': 'project_name: Reports\nremotes: {}\n',
            },
        });
        try {
            const server = await startLotarServer(workspace);
            try {
                for (const query of ['', '?project=RPT']) {
                    const response = await fetch(`${server.url}/api/sync/reports/list${query}`);
                    expect(response.status).toBe(200);
                    const payload = await response.json() as { data: { reports: unknown[]; total: number } };
                    expect(payload.data.reports).toEqual([]);
                    expect(payload.data.total).toBe(0);
                }
                await withPage(`${server.url}/sync`, async page => {
                    const empty = page.getByText('No reports yet.', { exact: true });
                    await expect.poll(() => empty.isVisible()).toBe(true);
                    expect(await page.getByText(/Cannot read properties|undefined.*map/).count()).toBe(0);
                    await page.selectOption('#sync-scope', 'RPT');
                    await expect.poll(() => empty.isVisible()).toBe(true);
                    expect(await page.getByText(/Cannot read properties|undefined.*map/).count()).toBe(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
