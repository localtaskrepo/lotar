import { it } from 'vitest';
import { startLotarServer } from '../../helpers/server.js';
import { withBrowser, withPage } from '../../helpers/ui.js';
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

// DEV-788: under machine load the timeout can fire while Chromium is still starting.
it('times out while its browser is still launching', async () => {
    await withBrowser({}, async (context) => {
        const page = await context.newPage();
        await page.waitForSelector('#never-rendered', { timeout: 0 });
    });
}, 50);
