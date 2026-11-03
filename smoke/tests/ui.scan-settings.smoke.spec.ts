import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-264: the Scan page's "Global" save writes the global config, sends only
// the setting that changed, and leaves counted-repetition regexes intact.
describe('Scan page scanner settings', () => {
    it('saves a Global change to the global config only, without pinning other values', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml':
                    'default.project: SCAN\nscan:\n  ticket_patterns:\n  - "[A-Z]{2,5}-\\\\d+"\n  enable_mentions: true\n',
                '.tasks/SCAN/config.yml': 'project.name: Scan fixture\n',
            },
        });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/scan`, async (page) => {
                    const requests: Record<string, unknown>[] = [];
                    page.on('request', (request) => {
                        if (new URL(request.url()).pathname === '/api/config/set')
                            requests.push(request.postDataJSON());
                    });
                    const patterns = page.getByTestId('scan-ticket-patterns');
                    await expect.poll(() => patterns.inputValue(), { timeout: 15_000 }).toBe('[A-Z]{2,5}-\\d+');

                    await page.locator('#scan-mentions').uncheck();
                    await page.getByRole('button', { name: 'Save', exact: true }).click();
                    await expect.poll(() => requests.length, { timeout: 10_000 }).toBe(1);
                    expect(requests[0]).toEqual({ values: { scan_enable_mentions: 'false' }, global: true });

                    const globalPath = path.join(workspace.tasksDir, 'config.yml');
                    await expect
                        .poll(async () => fs.readFile(globalPath, 'utf8'), { timeout: 10_000 })
                        .toMatch(/enable_mentions: false/);
                    expect(await fs.readFile(globalPath, 'utf8')).toContain('[A-Z]{2,5}-\\d+');
                    const projectConfig = await fs.readFile(
                        path.join(workspace.tasksDir, 'SCAN', 'config.yml'),
                        'utf8',
                    );
                    expect(projectConfig).not.toContain('scan');
                    expect(await page.getByTestId('scan-settings-error').count()).toBe(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
