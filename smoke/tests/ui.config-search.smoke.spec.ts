import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-166: the Config page search finds a setting and opens its section.

describe.concurrent('UI config search', () => {
    it('finds a setting by name and jumps to it', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': 'default:\n  project: CFS\n' },
        });
        try {
            await workspace.addTask('Seed', { args: ['-p', 'CFS'] });
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/config`, async (page) => {
                    const search = page.getByTestId('config-search');
                    await search.waitFor({ timeout: 20_000 });
                    await search.fill('jobs at once');
                    const option = page.getByRole('option', { name: /Jobs at once/ });
                    await option.waitFor();
                    const artifacts = path.resolve('target/smoke-artifacts/config-search');
                    await fs.ensureDir(artifacts);
                    await page.screenshot({ path: path.join(artifacts, 'results.png') });
                    await search.press('Enter');
                    await expect.poll(() => new URL(page.url()).searchParams.get('section')).toBe('agents');
                    const field = page.getByTestId('config-agent-max-jobs');
                    await expect.poll(() => field.evaluate((element) => element === document.activeElement)).toBe(true);
                    expect(await field.isVisible()).toBe(true);

                    // Words from a description find the setting too.
                    await search.fill('mentioned');
                    await page.getByRole('option', { name: /Link mentioned tasks/ }).waitFor();
                    await search.press('Enter');
                    await expect.poll(() => new URL(page.url()).searchParams.get('section')).toBe('automation');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
