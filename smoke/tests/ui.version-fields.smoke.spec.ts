import fs from 'fs-extra';
import path from 'node:path';
import { parse, stringify } from 'yaml';
import type { Route } from '@playwright/test';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-373: custom fields can hold versions. The task panel flags a value
// that is not a version and does not save it, the search box compares
// versions, and the Config page lists the version fields.
// DEV-394: the Tasks list sets or clears a field on many tasks at once, and
// the Board groups by a custom field, version lanes in version order.

const artifacts = path.resolve('target/smoke-artifacts/version-fields');

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
    for (const [number, title, affects] of [
        ['4', 'Listed crash', ['0.9.1', '1.0.0-rc.1']],
        ['5', 'Unknown crash', null],
    ] as const) {
        files[`.tasks/VER/${number}.yml`] = stringify({
            title,
            status: 'Todo',
            priority: 'Low',
            type: 'Bug',
            reporter: 'Fixture',
            created: '2026-11-01T00:00:00Z',
            modified: '2026-11-01T00:00:00Z',
            custom_fields: affects ? { affects } : { product: 'cli' },
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
                    // The list shows a loader, not rows, while the new query loads,
                    // so wait for the whole result rather than for rows to go away.
                    const shown = async () => {
                        const counts: Record<string, number> = {};
                        for (const title of ['Old crash', 'Minor crash', 'New crash'])
                            counts[title] = await page.getByText(title, { exact: true }).count();
                        return counts;
                    };
                    await expect.poll(shown).toEqual({ 'Old crash': 0, 'Minor crash': 1, 'New crash': 0 });

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

    it('sets and clears a field on the selected tasks and keeps their other fields', async ({ expect }) => {
        await fs.ensureDir(artifacts);
        const workspace = await SmokeWorkspace.create({ name: 'dev394-bulk-field-', seedFiles: seed() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=VER`, async (page) => {
                    await page.waitForSelector('text=Old crash', { timeout: 15_000 });
                    await page.getByRole('button', { name: 'Bulk select', exact: true }).click();
                    const select = async (...titles: string[]) => {
                        for (const title of titles)
                            await page.locator('tr', { hasText: title }).locator('input[type="checkbox"]').check();
                    };
                    const setField = async (name: string, value: string) => {
                        await page.getByRole('button', { name: 'Bulk actions', exact: true }).click();
                        await page.getByTestId('bulk-set-field').click();
                        const dialog = page.getByTestId('bulk-field-dialog');
                        await dialog.waitFor();
                        await page.getByTestId('bulk-field-name').selectOption(name);
                        await page.getByTestId('bulk-field-value').fill(value);
                        return dialog;
                    };
                    const fields = async (id: string) => parse(await workspace.readTaskYaml(id))?.custom_fields ?? {};

                    await select('Old crash', 'New crash');
                    const dialog = await setField('affects', 'soon');
                    await expect
                        .poll(() => dialog.getByRole('alert').textContent())
                        .toContain('"soon" is not a version');
                    expect(await page.getByTestId('bulk-field-submit').isDisabled()).toBe(true);
                    await page.screenshot({ path: path.join(artifacts, 'bulk-set-field.png'), animations: 'disabled' });
                    // The update makes the list refresh in the background. Hold that
                    // refresh: the selection stays usable meanwhile (DEV-484).
                    const held: Route[] = [];
                    let hold = true;
                    await page.route('**/api/tasks/list*', async (route) => {
                        if (hold) held.push(route);
                        else await route.continue();
                    });
                    await page.getByTestId('bulk-field-value').fill('v0.14.0');
                    await page.getByTestId('bulk-field-submit').click();
                    await dialog.waitFor({ state: 'hidden' });
                    await expect
                        .poll(() => fields('VER-1'), { timeout: 10_000 })
                        .toEqual({ affects: '0.14.0', product: 'web' });
                    await expect.poll(() => fields('VER-3')).toEqual({ affects: '0.14.0', product: 'web' });
                    expect(await fields('VER-2')).toEqual({ affects: '0.13', product: 'web' });
                    await expect.poll(() => held.length).toBeGreaterThan(0);

                    await setField('product', '');
                    hold = false;
                    for (const route of held) await route.continue();
                    await expect
                        .poll(() => page.getByTestId('bulk-field-submit').textContent())
                        .toContain('Clear field');
                    await page.getByTestId('bulk-field-submit').click();
                    await expect.poll(() => fields('VER-1'), { timeout: 10_000 }).toEqual({ affects: '0.14.0' });
                    await expect.poll(() => fields('VER-3')).toEqual({ affects: '0.14.0' });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('groups the Board by a custom field, version lanes in version order', async ({ expect }) => {
        await fs.ensureDir(artifacts);
        const workspace = await SmokeWorkspace.create({ name: 'dev394-board-field-', seedFiles: seed() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/boards?project=VER`, async (page) => {
                    await expect.poll(() => page.locator('article.task').count(), { timeout: 15_000 }).toBe(5);
                    const group = page.getByTestId('board-groupby');
                    const labels = () => page.locator('.swimlane-label').allTextContents();
                    const counts = () => page.locator('.swimlane-count').allTextContents();
                    await group.selectOption('field:affects');
                    // A list value shows the task in each of its lanes.
                    await expect.poll(labels).toEqual(['0.9.1', '0.12.4', '0.13', '1.0.0-rc.1', '1.0.0', '(none)']);
                    expect(await counts()).toEqual(['1', '1', '1', '1', '1', '1']);
                    expect(await page.locator('article.task', { hasText: 'Listed crash' }).count()).toBe(2);
                    await page.screenshot({
                        path: path.join(artifacts, 'board-by-version.png'),
                        animations: 'disabled',
                    });
                    await group.selectOption('field:product');
                    await expect.poll(labels).toEqual(['cli', 'web', '(none)']);
                    expect(await counts()).toEqual(['1', '3', '1']);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
