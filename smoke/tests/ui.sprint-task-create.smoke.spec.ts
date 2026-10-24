import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

function seedFiles(): Record<string, string> {
    return {
        '.tasks/config.yml': 'default.project: ALT\ndefault.reporter: Fixture\nauto.set_reporter: false\n',
        '.tasks/WEB/config.yml': 'project.name: Creation fixture\n',
        '.tasks/ALT/config.yml': 'project.name: Alternate fixture\n',
        '.tasks/@sprints/1.yml': stringify({
            created: '2026-10-01T00:00:00Z',
            plan: { label: 'Creation sprint' },
            tasks: [],
        }),
        '.tasks/@sprints/2.yml': stringify({
            created: '2026-10-01T00:00:00Z',
            plan: { label: 'Other sprint' },
            tasks: [],
        }),
    };
}

async function sprintTasks(workspace: SmokeWorkspace, id: number): Promise<string[]> {
    const record = parse(await fs.readFile(path.join(workspace.tasksDir, '@sprints', `${id}.yml`), 'utf8'));
    return record.tasks ?? [];
}

async function projectTaskFiles(workspace: SmokeWorkspace): Promise<string[]> {
    return (await fs.readdir(path.join(workspace.tasksDir, 'WEB'))).filter(
        (name) => name.endsWith('.yml') && name !== 'config.yml',
    );
}

describe.concurrent('Sprint and backlog task creation (DEV-104)', () => {
    for (const viewport of [
        { name: 'desktop', width: 1280, height: 900 },
        { name: 'mobile', width: 390, height: 844 },
    ]) {
        it(`creates directly into the selected sprint or backlog on ${viewport.name}`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create({ name: 'sprint-create-', seedFiles: seedFiles() });
            try {
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/sprints?project=WEB`, async (page) => {
                        await page.setViewportSize(viewport);
                        const group = page.locator('[data-sprint-id="1"]');
                        const add = group.getByTestId('sprint-add-task');
                        await add.waitFor({ state: 'visible' });
                        const box = await add.boundingBox();
                        expect(box).not.toBeNull();
                        expect(box!.x).toBeGreaterThanOrEqual(0);
                        expect(box!.x + box!.width).toBeLessThanOrEqual(viewport.width);
                        expect(box!.height).toBeGreaterThanOrEqual(32);
                        const writes: Array<{ route: string; body: Record<string, unknown> }> = [];
                        page.on('request', (request) => {
                            const route = new URL(request.url()).pathname;
                            if (
                                request.method() === 'POST' &&
                                /^\/api\/(tasks\/(add|update)|sprints\/(add|remove))$/.test(route)
                            ) {
                                writes.push({ route, body: request.postDataJSON() });
                            }
                        });

                        const artifacts = path.resolve('target/smoke-artifacts/sprint-create');
                        await fs.ensureDir(artifacts);
                        await page.screenshot({
                            path: path.join(artifacts, `${viewport.name}.png`),
                            animations: 'disabled',
                        });
                        await add.focus();
                        await add.press('Enter');
                        const panel = page.getByRole('dialog', { name: 'Create task', exact: true });
                        await panel.waitFor({ state: 'visible' });
                        expect(
                            await page.locator('[data-sprint-id="2"]').getByTestId('sprint-add-task').isDisabled(),
                        ).toBe(true);
                        expect(await page.getByTestId('backlog-add-task').isDisabled()).toBe(true);
                        await expect.poll(() => panel.locator('.task-panel__sprint-chip').count()).toBe(1);
                        expect(await panel.locator('select').first().inputValue()).toBe('WEB');
                        expect(await panel.locator('.task-panel__sprint-field').textContent()).toContain(
                            'Creation sprint',
                        );

                        // Draft changes must stay local until the one atomic create.
                        await panel.locator('.task-panel__sprint-chip-remove').click();
                        await expect.poll(() => panel.locator('.task-panel__sprint-chip').count()).toBe(0);
                        await panel.locator('.task-panel__sprint-field .chip-field__add').click();
                        const dialog = page.getByRole('dialog', { name: 'Assign to sprint', exact: true });
                        await dialog.waitFor({ state: 'visible' });
                        await dialog.locator('select.input').selectOption('1');
                        await dialog.locator('button[type="submit"]').click();
                        await dialog.waitFor({ state: 'detached' });
                        await expect.poll(() => panel.locator('.task-panel__sprint-chip').count()).toBe(1);
                        await panel.locator('select').first().selectOption('ALT');
                        await expect
                            .poll(() => panel.getByRole('button', { name: 'Create task', exact: true }).isEnabled())
                            .toBe(true);
                        expect(await panel.locator('.task-panel__sprint-chip').count()).toBe(1);
                        await panel.locator('select').first().selectOption('WEB');
                        await expect
                            .poll(() => panel.getByRole('button', { name: 'Create task', exact: true }).isEnabled())
                            .toBe(true);
                        expect(await panel.locator('.task-panel__sprint-chip').count()).toBe(1);
                        expect(writes).toEqual([]);
                        expect(await sprintTasks(workspace, 1)).toEqual([]);
                        expect(await projectTaskFiles(workspace)).toEqual([]);

                        const sprintTitle = `Sprint creation ${viewport.name}`;
                        await panel.locator('input[placeholder="Title"]').fill(sprintTitle);
                        const createdResponse = page.waitForResponse(
                            (response) => new URL(response.url()).pathname === '/api/tasks/add',
                        );
                        await panel.getByRole('button', { name: 'Create task', exact: true }).click();
                        const created = await createdResponse;
                        expect(created.status()).toBe(201);
                        const sprintTask = (await created.json()).data;
                        await panel.waitFor({ state: 'detached' });
                        expect(writes).toEqual([
                            {
                                route: '/api/tasks/add',
                                body: expect.objectContaining({ title: sprintTitle, project: 'WEB', sprints: [1] }),
                            },
                        ]);
                        await expect.poll(() => group.locator(`tr[data-task-id="${sprintTask.id}"]`).count()).toBe(1);
                        await expect.poll(() => page.getByTestId('backlog-add-task').isEnabled()).toBe(true);
                        expect(await sprintTasks(workspace, 1)).toEqual([sprintTask.id]);
                        expect(await sprintTasks(workspace, 2)).toEqual([]);

                        // Reopening without a destination must not retain the last sprint.
                        const backlogAdd = page.getByTestId('backlog-add-task');
                        await backlogAdd.click();
                        await panel.waitFor({ state: 'visible' });
                        expect(await add.isDisabled()).toBe(true);
                        await expect
                            .poll(() => panel.getByRole('button', { name: 'Create task', exact: true }).isEnabled())
                            .toBe(true);
                        expect(await panel.locator('.task-panel__sprint-chip').count()).toBe(0);
                        await panel.locator('input[placeholder="Title"]').fill('Cancelled backlog draft');
                        await panel.getByRole('button', { name: 'Close panel', exact: true }).click();
                        await panel.waitFor({ state: 'detached' });
                        expect(writes).toHaveLength(1);
                        expect(await projectTaskFiles(workspace)).toHaveLength(1);

                        await backlogAdd.click();
                        await panel.waitFor({ state: 'visible' });
                        const backlogTitle = `Backlog creation ${viewport.name}`;
                        await panel.locator('input[placeholder="Title"]').fill(backlogTitle);
                        const backlogResponse = page.waitForResponse(
                            (response) => new URL(response.url()).pathname === '/api/tasks/add',
                        );
                        await panel.getByRole('button', { name: 'Create task', exact: true }).click();
                        const response = await backlogResponse;
                        expect(response.status()).toBe(201);
                        const backlogTask = (await response.json()).data;
                        await panel.waitFor({ state: 'detached' });
                        expect(writes).toHaveLength(2);
                        expect(writes[1].body).toMatchObject({ project: 'WEB', title: backlogTitle });
                        expect(writes[1].body.sprints).toBeUndefined();
                        await expect
                            .poll(() =>
                                page.locator(`[data-sprint-id="backlog"] tr[data-task-id="${backlogTask.id}"]`).count(),
                            )
                            .toBe(1);
                        expect(await sprintTasks(workspace, 1)).toEqual([sprintTask.id]);
                        expect(await sprintTasks(workspace, 2)).toEqual([]);
                        expect(await projectTaskFiles(workspace)).toHaveLength(2);
                        expect(
                            (await fs.readdir(path.join(workspace.tasksDir, 'ALT'))).filter(
                                (name) => name !== 'config.yml',
                            ),
                        ).toEqual([]);
                        expect(writes).toHaveLength(2);
                        expect(writes.map((write) => write.route)).toEqual(['/api/tasks/add', '/api/tasks/add']);
                    });
                } finally {
                    await server.stop();
                }
            } finally {
                await workspace.dispose();
            }
        });
    }

    it('preserves draft sprint removal while project configuration is pending', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'sprint-create-config-', seedFiles: seedFiles() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/sprints?project=WEB`, async (page) => {
                    await page.locator('[data-sprint-id="1"]').getByTestId('sprint-add-task').click();
                    const panel = page.getByRole('dialog', { name: 'Create task', exact: true });
                    await expect.poll(() => panel.locator('.task-panel__sprint-chip').count()).toBe(1);
                    let releaseConfig!: () => void;
                    const pending = new Promise<void>((resolve) => {
                        releaseConfig = resolve;
                    });
                    let observeRequest!: () => void;
                    const requested = new Promise<void>((resolve) => {
                        observeRequest = resolve;
                    });
                    await page.route('**/api/config/show*', async (route) => {
                        if (new URL(route.request().url()).searchParams.get('project') === 'ALT') {
                            observeRequest();
                            await pending;
                        }
                        await route.continue();
                    });
                    try {
                        await panel.locator('select').first().selectOption('ALT');
                        await requested;
                        expect(await panel.getByRole('button', { name: 'Create task', exact: true }).isDisabled()).toBe(
                            true,
                        );
                        await panel.locator('.task-panel__sprint-chip-remove').click();
                        expect(await panel.locator('.task-panel__sprint-chip').count()).toBe(0);
                    } finally {
                        releaseConfig();
                    }
                    await expect
                        .poll(() => panel.getByRole('button', { name: 'Create task', exact: true }).isEnabled())
                        .toBe(true);
                    expect(await panel.locator('.task-panel__sprint-chip').count()).toBe(0);
                    await panel.getByRole('button', { name: 'Close panel', exact: true }).click();
                    await panel.waitFor({ state: 'detached' });
                    expect(await projectTaskFiles(workspace)).toEqual([]);
                    expect(await sprintTasks(workspace, 1)).toEqual([]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('requires the existing closed-sprint opt-in before opening a targeted draft', async ({ expect }) => {
        const seeds = seedFiles();
        seeds['.tasks/@sprints/1.yml'] = stringify({
            created: '2026-10-01T00:00:00Z',
            plan: { label: 'Closed creation sprint' },
            actual: { started_at: '2026-10-01T00:00:00Z', closed_at: '2026-10-02T00:00:00Z' },
            tasks: [],
        });
        const workspace = await SmokeWorkspace.create({ name: 'sprint-create-closed-', seedFiles: seeds });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async (page) => {
                    await page.addInitScript(() => localStorage.setItem('lotar.sprints.window.v2', 'all'));
                    await page.goto(`${server.url}/sprints?project=WEB`);
                    const add = page.locator('[data-sprint-id="1"]').getByTestId('sprint-add-task');
                    await add.waitFor({ state: 'visible' });
                    expect(await add.isDisabled()).toBe(true);
                    await page.getByRole('button', { name: 'Toggle filters', exact: true }).click();
                    await page.getByLabel('Allow editing closed sprints', { exact: true }).check();
                    await expect.poll(() => add.isEnabled()).toBe(true);
                    await add.click();
                    const panel = page.getByRole('dialog', { name: 'Create task', exact: true });
                    await expect.poll(() => panel.locator('.task-panel__sprint-chip').count()).toBe(1);
                    expect(await panel.locator('.task-panel__sprint-field').textContent()).toContain(
                        'Closed creation sprint',
                    );
                    await panel.getByRole('button', { name: 'Close panel', exact: true }).click();
                    await panel.waitFor({ state: 'detached' });
                    expect(await projectTaskFiles(workspace)).toEqual([]);
                    expect(await sprintTasks(workspace, 1)).toEqual([]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('does not silently create in the backlog when the selected sprint disappears', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'sprint-create-missing-', seedFiles: seedFiles() });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/sprints?project=WEB`, async (page) => {
                    await page.locator('[data-sprint-id="1"]').getByTestId('sprint-add-task').click();
                    const panel = page.getByRole('dialog', { name: 'Create task', exact: true });
                    await expect.poll(() => panel.locator('.task-panel__sprint-chip').count()).toBe(1);
                    await panel.locator('input[placeholder="Title"]').fill('Stale sprint draft');
                    await fs.remove(path.join(workspace.tasksDir, '@sprints', '1.yml'));
                    const pending = page.waitForResponse(
                        (response) => new URL(response.url()).pathname === '/api/tasks/add',
                    );
                    await panel.getByRole('button', { name: 'Create task', exact: true }).click();
                    const response = await pending;
                    expect(response.status()).toBeGreaterThanOrEqual(400);
                    expect(response.status()).toBeLessThan(500);
                    expect(response.request().postDataJSON().sprints).toEqual([1]);
                    await expect
                        .poll(() => panel.getByRole('button', { name: 'Create task', exact: true }).isEnabled())
                        .toBe(true);
                    expect(await panel.isVisible()).toBe(true);
                    expect(await panel.locator('input[placeholder="Title"]').inputValue()).toBe('Stale sprint draft');
                    expect(await projectTaskFiles(workspace)).toEqual([]);
                    expect(await sprintTasks(workspace, 2)).toEqual([]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
