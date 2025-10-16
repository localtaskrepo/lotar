import path from 'node:path';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('UI multi-project smoke scenarios', () => {
    it('lists multi-project tasks and filters by detected project', async () => {
        const workspace = await SmokeWorkspace.create();

        const frontendDir = path.join('apps', 'frontend');
        const apiDir = path.join('services', 'api');
        const frontendTitle = 'UI Frontend auto task';
        const apiTitle = 'UI API auto task';

        try {

            await workspace.write(path.join(frontendDir, 'package.json'), JSON.stringify({ name: 'frontend-app', private: true }, null, 2));
            await workspace.write(path.join(apiDir, 'Cargo.toml'), ['[package]', 'name = "api-service"', 'version = "0.1.0"'].join('\n'));
            await workspace.write(path.join(apiDir, 'src', 'lib.rs'), '// lib');

            const frontendTask = await workspace.addTask(frontendTitle, {
                cwd: path.join(workspace.root, frontendDir),
            });
            const apiTask = await workspace.addTask(apiTitle, {
                cwd: path.join(workspace.root, apiDir),
            });

            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector(`text=${frontendTitle}`, { timeout: 20_000 });
                    await page.waitForSelector(`text=${apiTitle}`, { timeout: 20_000 });

                    const projectSelect = page.locator('select.ui-select').first();
                    await projectSelect.selectOption(apiTask.project);

                    await page.waitForFunction(
                        (excludeTitle: string) => !document.body.innerText.includes(excludeTitle),
                        frontendTitle,
                    );
                    await page.waitForSelector(`text=${apiTitle}`, { timeout: 20_000 });

                    await projectSelect.selectOption('');
                    await page.waitForSelector(`text=${frontendTitle}`, { timeout: 20_000 });
                    await page.waitForSelector(`text=${apiTitle}`, { timeout: 20_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('prioritizes explicit project overrides in UI filters', async () => {
        const workspace = await SmokeWorkspace.create();

        const frontendDir = path.join('apps', 'frontend');
        const defaultTitle = 'UI Default project task';
        const overrideTitle = 'UI Override project task';

        try {

            await workspace.write(path.join(frontendDir, 'package.json'), JSON.stringify({ name: 'frontend-app', private: true }, null, 2));

            const defaultTask = await workspace.addTask(defaultTitle, {
                cwd: path.join(workspace.root, frontendDir),
            });

            const overrideTask = await workspace.addTask(overrideTitle, {
                cwd: path.join(workspace.root, frontendDir),
                args: ['--project=QA'],
            });

            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector(`text=${defaultTitle}`, { timeout: 20_000 });
                    await page.waitForSelector(`text=${overrideTitle}`, { timeout: 20_000 });

                    const projectSelect = page.locator('select.ui-select').first();
                    await projectSelect.selectOption('QA');

                    await page.waitForFunction(
                        (excludeTitle: string) => !document.body.innerText.includes(excludeTitle),
                        defaultTitle,
                    );
                    await page.waitForSelector(`text=${overrideTitle}`, { timeout: 20_000 });

                    await projectSelect.selectOption(defaultTask.project);
                    await page.waitForSelector(`text=${defaultTitle}`, { timeout: 20_000 });
                    await page.waitForFunction(
                        (excluded: string) => !document.body.innerText.includes(excluded),
                        overrideTitle,
                    );
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
