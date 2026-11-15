import { it } from 'vitest';
import { startLotarServer } from '../../helpers/server.js';
import { withPage } from '../../helpers/ui.js';
import { SmokeWorkspace } from '../../helpers/workspace.js';

it('times out with its browser, server and workspace open', async () => {
    const workspace = await SmokeWorkspace.create({ name: 'timed-out-' });
    try {
        const server = await startLotarServer(workspace);
        try {
            await withPage(server.url, async (page) => {
                await page.waitForSelector('#never-rendered', { timeout: 0 });
            });
        } finally {
            await server.stop();
        }
    } finally {
        await workspace.dispose();
    }
});
