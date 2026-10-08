import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('Scan with unavailable preferences (DEV-70)', () => {
    for (const mode of ['get', 'set', 'unavailable'] as const) {
        it(`keeps cancellation safe and submits one real scan when storage ${mode} fails`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create({ seedFiles: {
                '.tasks/config.yml': 'default.project: SCAN\ndefault.reporter: fixture-user\nauto.set_reporter: false\n',
                '.tasks/SCAN/config.yml': 'project.name: Scan fixture\n',
                'scan-source.ts': '// TODO: Storage-safe scan fixture\n',
            } });
            try {
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/scan`, async page => {
                        await page.getByLabel('Paths', { exact: true }).fill('scan-source.ts');
                        await page.evaluate(mode => {
                            if (mode === 'unavailable') {
                                Object.defineProperty(window, 'localStorage', {
                                    configurable: true,
                                    get() { throw new DOMException('Storage denied', 'SecurityError'); },
                                });
                            } else if (mode === 'get') {
                                const original = Storage.prototype.getItem;
                                Storage.prototype.getItem = function (key: string) {
                                    if (key === 'scan-skip-run-confirm') throw new DOMException('Read denied', 'SecurityError');
                                    return original.call(this, key);
                                };
                            } else {
                                const original = Storage.prototype.setItem;
                                Storage.prototype.setItem = function (key: string, value: string) {
                                    if (key === 'scan-skip-run-confirm') throw new DOMException('Quota exceeded', 'QuotaExceededError');
                                    original.call(this, key, value);
                                };
                            }
                        }, mode);
                        let submissions = 0;
                        page.on('request', request => {
                            if (request.method() === 'POST' && new URL(request.url()).pathname === '/api/scan/run') submissions += 1;
                        });
                        const run = page.getByRole('button', { name: 'Run', exact: true });
                        const dialog = page.getByRole('dialog', { name: 'Run scan?', exact: true });
                        await run.click();
                        await dialog.waitFor({ state: 'visible' });
                        expect(await dialog.getByRole('button', { name: 'Cancel', exact: true }).evaluate(el => el === document.activeElement)).toBe(true);
                        await page.keyboard.press('Enter');
                        await dialog.waitFor({ state: 'detached' });
                        expect(submissions).toBe(0);
                        expect(await workspace.listTaskFiles()).toEqual([]);
                        expect(await workspace.read('scan-source.ts')).toBe('// TODO: Storage-safe scan fixture\n');

                        await run.click();
                        await dialog.waitFor({ state: 'visible' });
                        await dialog.getByLabel("Don't show this again", { exact: true }).check();
                        const completed = page.waitForResponse(response =>
                            new URL(response.url()).pathname === '/api/scan/run' && response.request().method() === 'POST');
                        await dialog.getByRole('button', { name: 'Run', exact: true }).click();
                        const response = await completed;
                        expect(response.ok()).toBe(true);
                        const payload = await response.json();
                        expect(payload.data.summary.created).toBe(1);
                        await dialog.waitFor({ state: 'detached' });
                        expect(submissions).toBe(1);
                        expect(await workspace.listTaskFiles()).toHaveLength(1);

                        // Failed reads/writes never silently opt out of subsequent confirmation.
                        await run.click();
                        await dialog.waitFor({ state: 'visible' });
                        await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
                        await dialog.waitFor({ state: 'detached' });
                        expect(submissions).toBe(1);
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
