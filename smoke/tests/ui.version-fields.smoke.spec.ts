import { parse, stringify } from 'yaml';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-373: custom fields can hold versions. The task panel flags a value
// that is not a version and does not save it, the search box compares
// versions, and the Config page lists the version fields.

function seed(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': stringify({
            default: { project: 'VER', reporter: 'Fixture' },
            custom: { fields: ['product', { name: 'affects', type: 'version' }] },
        }),
        '.tasks/VER/config.yml': 'project.name: Versions\n',
    };
    for (const [number, title, affects] of [
        ['1', 'Old crash', '0.12.4'],
        ['2', 'Minor crash', '0.13'],
        ['3', 'New crash', '1.0.0'],
    ] as const) {
        files[`.tasks/VER/${number}.yml`] = stringify({
            title,
            status: 'Todo',
            priority: 'High',
            type: 'Bug',
            reporter: 'Fixture',
            created: '2026-11-01T00:00:00Z',
            modified: '2026-11-01T00:00:00Z',
            custom_fields: { affects, product: 'web' },
        });
    }
    return files;
}

describe.concurrent('UI version fields (DEV-373)', () => {
    it('checks version values in the task panel and compares them in the search box', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'dev373-versions-', seedFiles: seed() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=VER`, async (page) => {
                    const search = page.getByTestId('filter-search');
                    await page.waitForSelector('text=Minor crash', { timeout: 15_000 });
                    await search.fill('affects>=0.13 affects<1.0');
                    await search.press('Enter');
                    await expect.poll(() => page.getByText('Old crash', { exact: true }).count()).toBe(0);
                    expect(await page.getByText('New crash', { exact: true }).count()).toBe(0);
                    expect(await page.getByText('Minor crash', { exact: true }).count()).toBe(1);

                    await page.locator('tr', { hasText: 'Minor crash' }).click();
                    const panel = page.locator('.task-panel');
                    await panel.waitFor({ state: 'visible', timeout: 10_000 });
                    const row = panel.locator('.task-panel__custom-row[data-version-field="true"]');
                    const value = row.locator('input[placeholder="Version, e.g. 0.13.2"]');
                    await expect.poll(() => value.inputValue(), { timeout: 10_000 }).toBe('0.13');

                    const updates: unknown[] = [];
                    page.on('request', (request) => {
                        if (new URL(request.url()).pathname === '/api/tasks/update')
                            updates.push(request.postDataJSON());
                    });
                    await value.fill('soon');
                    await value.blur();
                    const error = panel.locator('.task-panel__custom-error');
                    await expect.poll(() => error.textContent()).toContain('"soon" is not a version');
                    expect(await value.getAttribute('aria-invalid')).toBe('true');
                    await page.waitForTimeout(300);
                    expect(updates).toEqual([]);

                    await value.fill('v0.13.2, 0.13.3');
                    await value.blur();
                    await expect.poll(() => error.count()).toBe(0);
                    await expect
                        .poll(async () => parse(await workspace.readTaskYaml('VER-2'))?.custom_fields?.affects, {
                            timeout: 10_000,
                        })
                        .toEqual(['0.13.2', '0.13.3']);
                });

                await withPage(`${server.url}/config?section=fields`, async (page) => {
                    const field = page.getByTestId('config-version-fields');
                    await field.waitFor({ timeout: 15_000 });
                    await expect.poll(() => field.textContent()).toContain('affects');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
