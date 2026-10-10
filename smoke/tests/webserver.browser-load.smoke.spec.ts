import { existsSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { resolveBinaryPath } from '../helpers/binary.js';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

/**
 * Browser-level connection-handling regression for `lotar serve`, run
 * WITHOUT Git so it executes in sandboxes where .git initialization is
 * blocked. It mirrors the Linux CI failure (run 37864256299): two browser
 * sessions holding speculative preconnect sockets timed out on
 * `page.reload()` waiting for the load event, because the server serviced
 * accepted sockets serially and each idle preconnect starved later
 * requests until its 30s read timeout.
 *
 * The reloads use Playwright's DEFAULT waitUntil ('load') and default 30s
 * timeout — no relaxed waiting — so a regression back to serial servicing
 * fails this test the same way CI failed.
 */

const lotarBinaryAvailable = existsSync(resolveBinaryPath());

describe.skipIf(!lotarBinaryAvailable)('lotar serve browser load contract (real binary, no git)', () => {
    it('two browser sessions reload concurrently and both see the added task', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev101-browser-load-' });
        try {
            const server = await startLotarServer(workspace);
            try {
                const response = await fetch(`${server.url}/api/tasks/add?project=DEVA`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ title: 'Dev101 reload task' }),
                });
                expect(response.status).toBeLessThan(300);

                await withBrowser({}, async (context) => {
                    const pageA = await context.newPage();
                    const pageB = await context.newPage();
                    await Promise.all([
                        pageA.goto(server.url, { waitUntil: 'load' }),
                        pageB.goto(server.url, { waitUntil: 'load' }),
                    ]);

                    // Default waitUntil ('load') and default 30s timeout:
                    // the exact call shape that failed in CI.
                    await Promise.all([pageA.reload(), pageB.reload()]);

                    await Promise.all([
                        pageA.waitForSelector('text=Dev101 reload task', { timeout: 15_000 }),
                        pageB.waitForSelector('text=Dev101 reload task', { timeout: 15_000 }),
                    ]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
