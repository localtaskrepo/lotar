import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('Global task action and compact toolbars (DEV-98)', () => {
    for (const viewport of [
        { name: 'desktop', width: 1280, height: 900 },
        { name: 'mobile', width: 390, height: 844 },
    ]) {
        it(`keeps actions usable and creates in the known project on ${viewport.name}`, async ({ expect }) => {
            const today = new Date().toISOString().slice(0, 10);
            const workspace = await SmokeWorkspace.create({ seedFiles: {
                '.tasks/config.yml': 'default.project: ALT\ndefault.reporter: Fixture\nauto.set_reporter: false\n',
                '.tasks/ACT/config.yml': 'project.name: Actions fixture\n',
                '.tasks/ALT/config.yml': 'project.name: Alternate fixture\n',
                '.tasks/ACT/1.yml': stringify({ title: 'Existing action task', status: 'Todo', priority: 'Medium', type: 'Feature', reporter: 'Fixture', created: `${today}T00:00:00Z`, due_date: today }),
                '.tasks/ALT/1.yml': stringify({ title: 'Other project task', status: 'Todo', priority: 'Medium', type: 'Feature', reporter: 'Fixture', created: `${today}T00:00:00Z`, due_date: today }),
            } });
            const artifacts = path.resolve('target/smoke-artifacts/actions');
            await fs.ensureDir(artifacts);
            try {
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/?project=ACT`, async page => {
                        await page.setViewportSize(viewport);
                        const globalCreate = page.getByTestId('global-new-task');
                        const writes: Record<string, unknown>[] = [];
                        page.on('request', request => {
                            if (request.method() === 'POST' && new URL(request.url()).pathname === '/api/tasks/add') writes.push(request.postDataJSON());
                        });
                        await expect.poll(() => page.getByText('Existing action task', { exact: true }).count()).toBe(1);
                        expect(await globalCreate.isVisible()).toBe(true);
                        expect(await page.locator('.filter-bar__actions').getByRole('button', { name: 'Add task', exact: true }).count()).toBe(0);
                        for (const name of ['Bulk select', 'Export CSV', 'Configure columns']) {
                            const button = page.getByRole('button', { name, exact: true });
                            expect(await button.isVisible()).toBe(true);
                            const box = await button.boundingBox();
                            if (viewport.name === 'mobile') {
                                expect(box!.width).toBeLessThanOrEqual(40);
                                expect(box!.height).toBeGreaterThanOrEqual(32);
                            }
                        }
                        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
                        await page.screenshot({ path: path.join(artifacts, `toolbar-${viewport.name}.png`), animations: 'disabled' });
                        const bulk = page.getByRole('button', { name: 'Bulk select', exact: true });
                        await bulk.click();
                        expect(await bulk.getAttribute('aria-pressed')).toBe('true');
                        await page.locator('tbody input[type="checkbox"]').first().check();
                        expect(await page.getByRole('status', { name: 'Selected 1 of 1', exact: true }).isVisible()).toBe(true);
                        expect(await page.getByRole('button', { name: 'Bulk actions', exact: true }).isEnabled()).toBe(true);
                        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
                        await page.screenshot({ path: path.join(artifacts, `bulk-${viewport.name}.png`), animations: 'disabled' });
                        await bulk.click();
                        expect(await bulk.getAttribute('aria-pressed')).toBe('false');

                        for (const route of ['/boards?project=ACT', '/calendar?project=ACT', '/sprints?project=ACT', '/config?project=ACT', '/preferences']) {
                            await page.goto(`${server.url}${route}`);
                            expect(await globalCreate.isVisible()).toBe(true);
                            await globalCreate.focus();
                            await globalCreate.press('Enter');
                            const panel = page.getByRole('dialog', { name: 'Create task', exact: true });
                            await panel.waitFor({ state: 'visible' });
                            await expect.poll(() => panel.getByRole('button', { name: 'Create task', exact: true }).isEnabled()).toBe(true);
                            if (route.includes('project=ACT')) expect(await panel.locator('select').first().inputValue()).toBe('ACT');
                            expect(await globalCreate.isDisabled()).toBe(true);
                            await panel.getByRole('button', { name: 'Close panel', exact: true }).click();
                            await panel.waitFor({ state: 'detached' });
                        }
                        expect(writes).toEqual([]);

                        await page.goto(`${server.url}/boards?project=ACT`);
                        await globalCreate.click();
                        const panel = page.getByRole('dialog', { name: 'Create task', exact: true });
                        await panel.locator('input[placeholder="Title"]').fill('Global action fixture');
                        await expect.poll(() => panel.getByRole('button', { name: 'Create task', exact: true }).isEnabled()).toBe(true);
                        await panel.getByRole('button', { name: 'Create task', exact: true }).click();
                        await panel.waitFor({ state: 'detached' });
                        expect(writes).toHaveLength(1);
                        expect(writes[0]).toMatchObject({ title: 'Global action fixture', project: 'ACT' });
                        await expect.poll(() => page.locator('article.task .title', { hasText: 'Global action fixture' }).count()).toBe(1);
                        expect(await workspace.listTaskFiles()).toHaveLength(3);
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
