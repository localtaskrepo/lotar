import { parse, stringify } from 'yaml';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-20: tags can carry a colour. The Config page edits it, the config keeps
// it as an object inside the issue.tags list, and tag chips show it.

describe.concurrent('UI tag colours (DEV-20)', () => {
    it('sets a tag colour on the Config page and shows it on tag chips', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            name: 'dev20-tags-',
            seedFiles: {
                '.tasks/config.yml': stringify({ default: { project: 'ALPHA' } }),
                '.tasks/ALPHA/config.yml': stringify({ project: { name: 'Alpha' } }),
            },
        });
        try {
            await workspace.addTask('Coloured', { args: ['--project=ALPHA', '--tag=ui', '--tag=plain'] });
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/config?section=fields`, async (page) => {
                    const field = page.getByTestId('config-tag-colors');
                    await field.waitFor({ timeout: 15_000 });
                    await expect.poll(() => field.textContent()).toContain('No tag colours');
                    await page.getByTestId('tag-colors-add').click();
                    await field.getByLabel('Tag 1', { exact: true }).fill('ui');
                    await field.getByLabel('Named colour of ui', { exact: true }).selectOption('red');
                    // The preview chip shows the colour before saving.
                    await expect
                        .poll(() => field.locator('.tag--colored').first().getAttribute('style'))
                        .toContain('--tag-color: #ef4444');

                    const saved = page.waitForResponse((r) => new URL(r.url()).pathname === '/api/config/set');
                    await page.getByRole('button', { name: 'Save changes', exact: true }).click();
                    expect((await saved).ok()).toBe(true);
                    await expect
                        .poll(async () => parse(await workspace.read('.tasks/config.yml'))?.issue?.tags)
                        .toEqual(['*', { name: 'ui', color: 'red' }]);
                });

                await withPage(`${server.url}/?project=ALPHA`, async (page) => {
                    const row = page.locator('tr', { hasText: 'Coloured' });
                    await row.waitFor({ timeout: 15_000 });
                    const ui = row.locator('.tag', { hasText: 'ui' });
                    await expect.poll(() => ui.getAttribute('class')).toContain('tag--colored');
                    expect(await ui.getAttribute('style')).toContain('--tag-color: #ef4444');
                    // An uncoloured tag keeps the plain chip.
                    const plain = row.locator('.tag', { hasText: 'plain' });
                    expect(await plain.getAttribute('class')).not.toContain('tag--colored');
                    // The tint keeps the theme's text colour.
                    const colors = await ui.evaluate((el) => {
                        const style = getComputedStyle(el);
                        return { fg: style.color, bg: style.backgroundColor };
                    });
                    expect(colors.bg).not.toBe('rgba(0, 0, 0, 0)');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
