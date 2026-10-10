import { existsSync } from 'node:fs';
import type { Page } from '@playwright/test';
import { describe, it } from 'vitest';
import { resolveBinaryPath } from '../helpers/binary.js';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';
/**
 * Browser-level connection-handling regression for `lotar serve`, run
 * WITHOUT Git so it executes in sandboxes where .git initialization is
 * blocked. Two tabs share ONE browser context — the real two-tab user
 * scenario. The app holds a single consolidated /api/events stream per
 * tab, so both tabs together stay far below Chromium's per-host
 * connection pool cap and concurrent `page.reload()` calls must complete
 * under Playwright's DEFAULT waitUntil ('load') and default 30s timeout —
 * no relaxed waiting. A regression back to serial socket servicing or to
 * multiple SSE streams per tab fails this test the same way the original
 * Linux CI run (37864256299) failed.
 */

const lotarBinaryAvailable = existsSync(resolveBinaryPath());

function trackEventSourceRequests(page: Page): () => number {
    const requests: string[] = [];
    page.on('request', (request) => {
        if (request.url().includes('/api/events')) {
            requests.push(request.url());
        }
    });
    return () => requests.length;
}

describe.skipIf(!lotarBinaryAvailable)('lotar serve browser load contract (real binary, no git)', () => {
    it('two tabs in one context each hold one event source and reload concurrently', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'dev101-browser-load-' });
        try {
            const server = await startLotarServer(workspace);
            try {
                const addTaskViaApi = async (title: string): Promise<void> => {
                    const response = await fetch(`${server.url}/api/tasks/add?project=DEVA`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ title }),
                    });
                    expect(response.status).toBeLessThan(300);
                };
                await addTaskViaApi('Dev101 reload task');

                await withBrowser({}, async (context) => {
                    const pageA = await context.newPage();
                    const pageB = await context.newPage();
                    const eventSourcesA = trackEventSourceRequests(pageA);
                    const eventSourcesB = trackEventSourceRequests(pageB);

                    await Promise.all([
                        pageA.goto(server.url, { waitUntil: 'load' }),
                        pageB.goto(server.url, { waitUntil: 'load' }),
                    ]);
                    await Promise.all([
                        pageA.waitForSelector('text=Dev101 reload task', { timeout: 15_000 }),
                        pageB.waitForSelector('text=Dev101 reload task', { timeout: 15_000 }),
                    ]);

                    // Exactly one consolidated /api/events source per tab.
                    await expect.poll(eventSourcesA, { timeout: 15_000 }).toBe(1);
                    await expect.poll(eventSourcesB, { timeout: 15_000 }).toBe(1);

                    await addTaskViaApi('Dev101 live update task');

                    // The real task event reaches both tabs before any reload,
                    // and neither tab needed a second SSE connection to see it.
                    await Promise.all([
                        pageA.waitForSelector('text=Dev101 live update task', { timeout: 15_000 }),
                        pageB.waitForSelector('text=Dev101 live update task', { timeout: 15_000 }),
                    ]);
                    expect(eventSourcesA()).toBe(1);
                    expect(eventSourcesB()).toBe(1);

                    // Default waitUntil 'load' and default timeout on purpose.
                    await Promise.all([pageA.reload(), pageB.reload()]);

                    await Promise.all([
                        pageA.waitForSelector('text=Dev101 live update task', { timeout: 15_000 }),
                        pageB.waitForSelector('text=Dev101 live update task', { timeout: 15_000 }),
                    ]);
                    await expect.poll(eventSourcesA, { timeout: 15_000 }).toBe(2);
                    await expect.poll(eventSourcesB, { timeout: 15_000 }).toBe(2);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
