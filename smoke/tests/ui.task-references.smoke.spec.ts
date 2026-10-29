import path from 'node:path';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe('DEV-6 task references in markdown (actual backend/browser)', () => {
    it('links a task ID in a description, shows its hover card, and opens it on click', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            name: 'task-refs-ui-',
            env: { LOTAR_DEFAULT_REPORTER: 'Smoke' },
        });
        try {
            const target = await workspace.addTask('Paginate the notes API', {
                args: ['--project=REF', '--priority=High', '--reporter=Smoke'],
            });
            const source = await workspace.addTask('Ship the notes screen', {
                args: [
                    '--project=REF',
                    '--reporter=Smoke',
                    `--description=Blocked by ${target.id} until it lands. UTF-8 and \`${target.id}\` stay text.`,
                ],
            });
            const server = await startLotarServer(workspace, { env: { LOTAR_SSE_READY: '1' } });
            try {
                await withBrowser({}, async (context) => {
                    const page = await context.newPage();
                    try {
                        await page.goto(`${server.url}/?project=REF&taskPanel=${source.id}`, {
                            waitUntil: 'domcontentloaded',
                        });
                        const panel = page.locator('.task-panel');
                        const description = panel.locator('.task-panel__description-display');
                        const link = description.locator('a.task-ref');
                        await expect.poll(() => link.count()).toBe(1);
                        expect(await link.textContent()).toBe(target.id);
                        expect(await description.locator('code').textContent()).toBe(target.id);

                        await link.hover();
                        const card = page.locator('.task-hover-card.is-open');
                        await expect.poll(() => card.isVisible()).toBe(true);
                        expect(await card.textContent()).toContain('Paginate the notes API');
                        expect(await card.textContent()).toContain('High');
                        await page.screenshot({
                            path: path.join(process.cwd(), 'target/smoke-artifacts/task-references/hover-card.png'),
                            animations: 'disabled',
                        });

                        await link.click();
                        await expect.poll(() => new URL(page.url()).searchParams.get('taskPanel')).toBe(target.id);
                        await expect
                            .poll(() => panel.locator('h2').first().textContent())
                            .toBe('Paginate the notes API');
                        expect(await panel.locator('.task-panel__description-editor').count()).toBe(0);
                        expect(await page.locator('.task-hover-card').count()).toBe(0);
                    } finally {
                        await page.close();
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
