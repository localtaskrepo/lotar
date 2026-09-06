import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe('UI panel scope safety', () => {
    it('blocks create during pending and failed project config, then submits refreshed defaults', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const a = await workspace.addTask('Panel scope A seed', { args: ['--project=PANA'] });
            const b = await workspace.addTask('Panel scope B seed', { args: ['--project=PANB'] });
            expect(a.project).toBe('PANA');
            expect(b.project).toBe('PANB');
            const server = await startLotarServer(workspace, {
                env: process.env.LOTAR_WEB_UI_PATH ? {
                    LOTAR_WEB_UI_EMBEDDED: '0',
                    LOTAR_WEB_UI_PATH: process.env.LOTAR_WEB_UI_PATH,
                } : undefined,
            });
            try {
                await withPage(`${server.url}/?project=PANA`, async page => {
                    await page.getByRole('button', { name: 'Add task', exact: true }).click();
                    const panel = page.locator('.task-panel');
                    const create = panel.getByRole('button', { name: 'Create task', exact: true });
                    await expect.poll(() => create.isEnabled(), { timeout: 10_000 }).toBe(true);
                    await panel.locator('input[placeholder="Title"]').fill('Panel scope safety created');
                    let release!: () => void;
                    const pending = new Promise<void>(resolve => { release = resolve; });
                    let first = true;
                    const writes: Record<string, unknown>[] = [];
                    page.on('request', request => {
                        if (new URL(request.url()).pathname === '/api/tasks/add') writes.push(request.postDataJSON());
                    });
                    await page.route('**/api/config/show?project=PANB', async route => {
                        if (first) {
                            first = false;
                            await pending;
                            await route.fulfill({ status: 500, contentType: 'application/json', body: JSON.stringify({ error: 'Config temporarily unavailable' }) });
                        } else {
                            const response = await route.fetch();
                            const config = await response.json();
                            await route.fulfill({ response, json: {
                                ...config,
                                data: { ...config.data, default_assignee: 'b-owner', default_reporter: 'b-reporter', default_tags: ['b-tag'] },
                            } });
                        }
                    });
                    try {
                        await panel.locator('select').first().selectOption('PANB');
                        expect(await create.isDisabled()).toBe(true);
                        await panel.locator('form').evaluate(form => form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
                        expect(writes).toEqual([]);
                        release();
                        await panel.locator('.task-panel__errors').waitFor();
                        expect(await panel.locator('input[placeholder="Title"]').inputValue()).toBe('Panel scope safety created');
                        expect(await create.isDisabled()).toBe(true);
                        await panel.locator('form').evaluate(form => form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
                        expect(writes).toEqual([]);
                        await panel.getByRole('button', { name: 'Retry configuration', exact: true }).click();
                        await expect.poll(() => create.isEnabled(), { timeout: 10_000 }).toBe(true);
                        await create.click();
                        await expect.poll(() => writes.length, { timeout: 10_000 }).toBe(1);
                        expect(writes[0]).toMatchObject({
                            project: 'PANB', title: 'Panel scope safety created',
                            assignee: 'b-owner', reporter: 'b-reporter', tags: ['b-tag'],
                        });
                        await panel.waitFor({ state: 'hidden' });
                    } finally {
                        release();
                    }
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
