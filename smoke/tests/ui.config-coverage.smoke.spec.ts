import { parse, stringify } from 'yaml';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-146: the Config page represents every settable option with a bespoke
// control. Custom fields could not be turned off for one project because a
// blank project value means "inherit"; the explicit "No custom fields" choice
// must persist as an empty list, and inherit must remove the override again.

async function inspect(url: string, project?: string) {
    const response = await fetch(`${url}/api/config/inspect${project ? `?project=${project}` : ''}`);
    if (!response.ok) throw new Error(`inspect: ${response.status}`);
    return (await response.json()).data as {
        effective: Record<string, unknown>;
        project_raw?: Record<string, unknown> | null;
    };
}

describe.concurrent('UI configuration coverage (DEV-146)', () => {
    it('turns custom fields off for one project and back to inherit', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            name: 'dev146-config-',
            seedFiles: {
                '.tasks/config.yml': stringify({ default: { project: 'ALPHA' } }),
                '.tasks/ALPHA/config.yml': stringify({ project: { name: 'Alpha' } }),
            },
        });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/config?section=fields`, async (page) => {
                    await page.getByLabel('Project scope').selectOption('ALPHA');
                    const mode = page.getByTestId('config-custom-fields').getByRole('combobox');
                    await expect.poll(() => mode.inputValue()).toBe('inherit');
                    const save = page.getByRole('button', { name: 'Save changes', exact: true });

                    await mode.selectOption('none');
                    let saved = page.waitForResponse((r) => new URL(r.url()).pathname === '/api/config/set');
                    await save.click();
                    expect((await saved).ok()).toBe(true);
                    await expect
                        .poll(async () => (await inspect(server.url, 'ALPHA')).project_raw?.custom_fields)
                        .toEqual([]);
                    expect((await inspect(server.url, 'ALPHA')).effective.custom_fields).toEqual([]);
                    expect(parse(await workspace.read('.tasks/ALPHA/config.yml'))).toMatchObject({
                        custom: { fields: [] },
                    });
                    // The global wildcard is untouched.
                    expect((await inspect(server.url)).effective.custom_fields).toEqual(['*']);

                    await expect.poll(() => mode.inputValue()).toBe('none');
                    await mode.selectOption('inherit');
                    saved = page.waitForResponse((r) => new URL(r.url()).pathname === '/api/config/set');
                    await save.click();
                    expect((await saved).ok()).toBe(true);
                    await expect
                        .poll(async () => (await inspect(server.url, 'ALPHA')).project_raw?.custom_fields ?? null)
                        .toBeNull();
                    expect((await inspect(server.url, 'ALPHA')).effective.custom_fields).toEqual(['*']);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('saves previously hidden global settings through their bespoke controls', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            name: 'dev146-config-global-',
            seedFiles: { '.tasks/config.yml': stringify({ default: { project: 'ALPHA' } }) },
        });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/config`, async (page) => {
                    const meta = page.locator('.floating-actions__meta');
                    // A freshly loaded page is clean.
                    await expect.poll(() => meta.textContent()).toContain('Last updated');

                    // Settings live in separate sections (DEV-19); edits survive switching.
                    const section = (id: string) => page.locator(`[data-section-link="${id}"]`).click();
                    await section('sprints');
                    await page
                        .getByTestId('config-sprints')
                        .getByLabel(/^Length/)
                        .fill('2w');
                    await page.getByLabel('Capacity (points)').fill('30');
                    await section('people');
                    const members = page.getByTestId('config-members');
                    await members.getByRole('button', { name: /add member/i }).click();
                    await members.getByRole('textbox').fill('alice');
                    await members.getByRole('textbox').press('Enter');
                    await section('storage');
                    await page.getByTestId('config-sync-reports').getByRole('switch').uncheck();
                    await section('agents');
                    await page.getByLabel('Context file extension').fill('.ctx');
                    expect(await page.locator('.config-nav__badge--dirty').count()).toBe(4);

                    const saved = page.waitForResponse((r) => new URL(r.url()).pathname === '/api/config/set');
                    await page.getByRole('button', { name: 'Save changes', exact: true }).click();
                    const response = await saved;
                    expect(response.ok()).toBe(true);
                    expect(response.request().postDataJSON().values).toEqual({
                        members: 'alice',
                        sync_write_reports: 'false',
                        agent_context_extension: '.ctx',
                        sprints_defaults_length: '2w',
                        sprints_defaults_capacity_points: '30',
                    });
                    await expect
                        .poll(async () => {
                            const effective = (await inspect(server.url)).effective;
                            return [
                                effective.members,
                                effective.sync_write_reports,
                                effective.agent_context_extension,
                                effective.sprint_defaults,
                            ];
                        })
                        .toEqual([['alice'], false, '.ctx', { length: '2w', capacity_points: 30 }]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    // DEV-157: one control size and aligned settings rows in every section.
    it('aligns setting rows and sizes controls consistently in both scopes', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            name: 'dev157-config-layout-',
            seedFiles: {
                '.tasks/config.yml': stringify({ default: { project: 'ALPHA' } }),
                '.tasks/ALPHA/config.yml': stringify({ project: { name: 'Alpha' } }),
            },
        });
        try {
            const server = await startLotarServer(workspace);
            try {
                for (const scope of ['', 'ALPHA']) {
                    await withPage(
                        `${server.url}/config?section=automation${scope ? `&project=${scope}` : ''}`,
                        async (page) => {
                            await page.setViewportSize({ width: 1440, height: 1000 });
                            const rows = page.locator('[data-testid="config-automation"] .cfg-toggle');
                            await expect.poll(() => rows.count()).toBe(9);
                            const controls = await rows.evaluateAll((elements) =>
                                elements.map((row) => {
                                    const control = row.querySelector('.cfg-switch, select')!.getBoundingClientRect();
                                    const box = row.getBoundingClientRect();
                                    return {
                                        right: Math.round(control.right),
                                        centerOffset: Math.abs(
                                            control.top + control.height / 2 - (box.top + box.height / 2),
                                        ),
                                    };
                                }),
                            );
                            expect(new Set(controls.map((c) => c.right)).size).toBe(1);
                            for (const control of controls) expect(control.centerOffset).toBeLessThan(2);
                        },
                    );
                }
                for (const section of ['defaults', 'agents', 'storage', 'sprints']) {
                    await withPage(`${server.url}/config?section=${section}`, async (page) => {
                        await page.setViewportSize({ width: 1440, height: 1000 });
                        const fields = page.locator(
                            `section[data-section="${section}"] .field input.input, section[data-section="${section}"] .field select.input`,
                        );
                        await expect.poll(() => fields.count()).toBeGreaterThan(1);
                        const heights = await fields.evaluateAll((elements) =>
                            elements.map((el) => Math.round(el.getBoundingClientRect().height)),
                        );
                        expect(new Set(heights).size, `${section}: ${heights.join(',')}`).toBe(1);
                    });
                }
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
