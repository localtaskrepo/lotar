import fs from 'fs-extra';
import path from 'node:path';
import { parse } from 'yaml';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-364: the build output deleted when an agent job ends is set on the
// Config page (Agents) and saved as agent.worktree.cleanup_paths.

describe.concurrent('UI config cleanup paths', () => {
    it('adds a build output path and saves it', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': 'default:\n  project: CLP\n' },
        });
        try {
            await workspace.addTask('Seed', { args: ['-p', 'CLP'] });
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/config?section=agents`, async (page) => {
                    const field = page.getByTestId('config-agent-cleanup-paths');
                    await field.waitFor({ timeout: 20_000 });
                    expect(await field.textContent()).toContain('Nothing is deleted');
                    await field.getByRole('button', { name: 'Add path' }).click();
                    const input = field.getByPlaceholder('target');
                    await input.fill('target');
                    await input.press('Enter');
                    await field.getByText('target', { exact: true }).waitFor();
                    const artifacts = path.resolve('target/smoke-artifacts/config-cleanup-paths');
                    await fs.ensureDir(artifacts);
                    await field.screenshot({ path: path.join(artifacts, 'field.png') });
                    await page.getByRole('button', { name: 'Save changes' }).click();

                    const configPath = path.join(workspace.tasksDir, 'config.yml');
                    await expect
                        .poll(async () => {
                            const config = parse(await fs.readFile(configPath, 'utf8')) as {
                                agent?: { worktree?: { cleanup_paths?: string[] } };
                            };
                            return config.agent?.worktree?.cleanup_paths;
                        })
                        .toEqual(['target']);

                    await page.reload();
                    await field.getByText('target', { exact: true }).waitFor({ timeout: 20_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
