import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { html5DragAndDrop, withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

async function readSprintMembership(workspace: SmokeWorkspace, sprintId: string): Promise<string[]> {
    const sprintFile = path.join(workspace.tasksDir, '@sprints', `${sprintId}.yml`);
    let raw: string;
    try {
        raw = await fs.readFile(sprintFile, 'utf8');
    } catch (error) {
        throw new Error(
            `sprint file ${sprintFile} must exist while polling membership (sprints are created before the server starts): ${(error as Error).message}`,
        );
    }
    const sprint = parse(raw) as { tasks?: string[] };
    return sprint.tasks ?? [];
}

async function pollUntil<T>(
    probe: () => Promise<T>,
    done: (value: T) => boolean,
    timeoutMs = 10_000,
    intervalMs = 100,
): Promise<T> {
    const deadline = Date.now() + timeoutMs;
    let last = await probe();
    while (!done(last)) {
        if (Date.now() >= deadline) {
            return last;
        }
        await new Promise((resolve) => setTimeout(resolve, intervalMs));
        last = await probe();
    }
    return last;
}

describe.concurrent('UI sprints smoke scenarios', () => {
    it('renders sprint groups when stored sprints exist', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'Smoke Sprint Smoke Test']);

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.click('a[href="/sprints"]');
                    await page.waitForSelector('text=Sprints', { timeout: 15_000 });
                    await page.waitForSelector('.sprint-group:not(.backlog-group) .group-title h2', {
                        timeout: 15_000,
                    });

                    const headings = await page.$$eval(
                        '.sprint-group:not(.backlog-group) .group-title h2',
                        (nodes: Element[]) =>
                            nodes.map((node) => (node.textContent ? node.textContent.trim() : '')),
                    );

                    expect(headings.some((text: string) => text.includes('Smoke Sprint Smoke Test'))).toBe(true);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('supports dragging tasks between sprints', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'Drag Alpha']);
            await workspace.runLotar(['sprint', 'create', '--label', 'Drag Beta']);

            const task = await workspace.addTask('Drag-and-drop task', {
                args: ['--project', 'WEB'],
            });

            await workspace.runLotar(['sprint', 'add', task.id, '--sprint', '1']);

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.click('a[href="/sprints"]');
                    await page.waitForSelector('text=Sprints', { timeout: 15_000 });

                    const sourceSelector = '[data-sprint-id="1"] tr.task-row[data-task-id="' + task.id + '"]';
                    const targetSelector = '[data-sprint-id="2"]';

                    await page.waitForSelector(sourceSelector, { timeout: 15_000 });
                    await page.waitForSelector(targetSelector, { timeout: 15_000 });

                    await html5DragAndDrop(page, sourceSelector, targetSelector);

                    await page.waitForSelector('[data-sprint-id="2"] tr.task-row[data-task-id="' + task.id + '"]', {
                        timeout: 10_000,
                    });

                    const targetMembership = await pollUntil(
                        () => readSprintMembership(workspace, '2'),
                        (members) => members.includes(task.id),
                    );
                    expect(targetMembership).toEqual([task.id]);
                    const sourceMembership = await readSprintMembership(workspace, '1');
                    expect(sourceMembership).not.toContain(task.id);

                    const remaining = await page.$(
                        '[data-sprint-id="1"] tr.task-row[data-task-id="' + task.id + '"]',
                    );
                    expect(remaining).toBeNull();
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('reorders sprint table columns via drag and drop', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'Columns Sprint']);
            const task = await workspace.addTask('Column reorder task', { args: ['--project', 'WEB'] });
            await workspace.runLotar(['sprint', 'add', task.id, '--sprint', '1']);

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.click('a[href="/sprints"]');
                    await page.waitForSelector('[data-sprint-id="1"] .sprint-table', { timeout: 15_000 });

                    const headerOrder = async () =>
                        page.$$eval(
                            '[data-sprint-id="1"] .sprint-table thead tr th',
                            (nodes: Element[]) =>
                                nodes.map((node) =>
                                    (node as HTMLElement).dataset.column
                                        ? (node as HTMLElement).dataset.column
                                        : '',
                                ),
                        );

                    const before = await headerOrder();
                    expect(before.includes('status')).toBe(true);
                    expect(before.includes('priority')).toBe(true);
                    expect(before.indexOf('priority')).toBeGreaterThan(before.indexOf('status'));

                    await html5DragAndDrop(
                        page,
                        '[data-sprint-id="1"] th[data-column="priority"] button.header-button',
                        '[data-sprint-id="1"] th[data-column="status"] button.header-button',
                    );

                    const after = await pollUntil(
                        headerOrder,
                        (order) => order.indexOf('priority') < order.indexOf('status'),
                    );
                    expect(after.indexOf('priority')).toBeLessThan(after.indexOf('status'));
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps the task in both sprints when copy modifier is active while dragging', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'Copy Alpha']);
            await workspace.runLotar(['sprint', 'create', '--label', 'Copy Beta']);

            const task = await workspace.addTask('Copy drag task', {
                args: ['--project', 'WEB'],
            });

            await workspace.runLotar(['sprint', 'add', task.id, '--sprint', '1']);

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.click('a[href="/sprints"]');
                    await page.waitForSelector('text=Sprints', { timeout: 15_000 });

                    const sourceSelector = '[data-sprint-id="1"] tr.task-row[data-task-id="' + task.id + '"]';
                    const targetSelector = '[data-sprint-id="2"]';

                    await page.waitForSelector(sourceSelector, { timeout: 15_000 });
                    await page.waitForSelector(targetSelector, { timeout: 15_000 });

                    const modifierKey = 'Alt';
                    await page.keyboard.down(modifierKey);

                    try {
                        await html5DragAndDrop(page, sourceSelector, targetSelector, { altKey: true });
                    } finally {
                        await page.keyboard.up(modifierKey);
                    }

                    await page.waitForSelector('[data-sprint-id="2"] tr.task-row[data-task-id="' + task.id + '"]', {
                        timeout: 10_000,
                    });

                    const memberships = await pollUntil(
                        async () => {
                            const [source, target] = await Promise.all([
                                readSprintMembership(workspace, '1'),
                                readSprintMembership(workspace, '2'),
                            ]);
                            return { source, target };
                        },
                        ({ source, target }) => source.includes(task.id) && target.includes(task.id),
                    );
                    expect(memberships.source).toEqual([task.id]);
                    expect(memberships.target).toEqual([task.id]);

                    const sourceStillExists = await page.$(
                        '[data-sprint-id="1"] tr.task-row[data-task-id="' + task.id + '"]',
                    );
                    expect(sourceStillExists).not.toBeNull();
                    const targetStillExists = await page.$(
                        '[data-sprint-id="2"] tr.task-row[data-task-id="' + task.id + '"]',
                    );
                    expect(targetStillExists).not.toBeNull();
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps sprint board in sync when sprint membership changes via the task panel', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'Panel Sync Sprint']);

            const task = await workspace.addTask('Panel sync task', {
                args: ['--project', 'WEB'],
            });

            await workspace.runLotar(['sprint', 'add', task.id, '--sprint', '1']);

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.click('a[href="/sprints"]');
                    await page.waitForSelector('text=Sprints', { timeout: 15_000 });

                    const sprintId = '1';
                    const sprintRowSelector = `[data-sprint-id="${sprintId}"] tr.task-row[data-task-id="${task.id}"]`;
                    const backlogRowSelector = `[data-sprint-id="backlog"] tr.task-row[data-task-id="${task.id}"]`;

                    await page.waitForSelector(sprintRowSelector, { timeout: 15_000 });
                    await page.click(sprintRowSelector);
                    await page.waitForSelector('.task-panel', { timeout: 15_000 });

                    await page.click('.task-panel__sprint-chip-remove');

                    await page.waitForSelector(sprintRowSelector, { state: 'detached', timeout: 15_000 });
                    await page.waitForSelector(backlogRowSelector, { timeout: 15_000 });

                    await page.click('.task-panel__sprint-field .chip-field__add');
                    await page.waitForSelector('.task-panel-dialog__overlay', { timeout: 10_000 });
                    await page.selectOption('.task-panel-dialog__overlay select.input', sprintId);
                    await page.click('.task-panel-dialog__overlay button[type="submit"]');
                    await page.waitForSelector('.task-panel-dialog__overlay', { state: 'detached', timeout: 15_000 });

                    await page.waitForSelector(sprintRowSelector, { timeout: 15_000 });
                    await page.waitForSelector(backlogRowSelector, { state: 'detached', timeout: 15_000 });

                    const finalSprintRow = await page.$(sprintRowSelector);
                    expect(finalSprintRow).not.toBeNull();
                    const finalBacklogRow = await page.$(backlogRowSelector);
                    expect(finalBacklogRow).toBeNull();

                    await page.click('.task-panel__header-actions button');
                    await page.waitForSelector('.task-panel', { state: 'detached', timeout: 10_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('deletes a sprint via the UI and returns tasks to the backlog', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'UI Delete Smoke Sprint']);
            const task = await workspace.addTask('UI Sprint Delete Task');
            await workspace.runLotar(['sprint', 'add', task.id, '--sprint', '1']);

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.click('a[href="/sprints"]');
                    await page.waitForSelector('[data-sprint-id="1"]', { timeout: 15_000 });

                    await page.click('[data-sprint-id="1"] button[data-testid="sprint-delete"]');
                    await page.waitForSelector('.ui-modal__overlay', { timeout: 10_000 });
                    await page.click('.sprint-delete__actions .btn.danger');

                    await page.waitForSelector('.ui-modal__overlay', {
                        state: 'detached',
                        timeout: 15_000,
                    });
                    await page.waitForSelector('[data-sprint-id="1"]', {
                        state: 'detached',
                        timeout: 15_000,
                    });

                    await page.waitForSelector(
                        '[data-sprint-id="backlog"] tr.task-row[data-task-id="' + task.id + '"]',
                        { timeout: 15_000 },
                    );
                });
            } finally {
                await server.stop();
            }

            const sprintFileGone = await pollUntil(
                async () => !(await fs.pathExists(path.join(workspace.tasksDir, '@sprints', '1.yml'))),
                (gone) => gone,
            );
            expect(sprintFileGone).toBe(true);
        } finally {
            await workspace.dispose();
        }
    });

    it('creates a task from the UI after completing several empty sprints', async () => {
        const workspace = await SmokeWorkspace.create();
        const taskTitle = 'Task created after completing sprints';
        const longDescription = 'Long description line exercising large request bodies end to end. '.repeat(20);
        expect(Buffer.byteLength(longDescription, 'utf8')).toBeGreaterThan(1024);

        try {
            await workspace.addTask('Sprint completion seed task', { args: ['--project', 'WEB'] });

            for (const label of ['Complete Alpha', 'Complete Beta', 'Complete Gamma']) {
                await workspace.runLotar(['sprint', 'create', '--label', label]);
            }

            const filesBefore = await workspace.listTaskFiles();

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.click('a[href="/sprints"]');
                    await page.waitForSelector('text=Sprints', { timeout: 15_000 });

                    for (const sprintId of ['1', '2', '3']) {
                        const group = page.locator(`[data-sprint-id="${sprintId}"]`);
                        await group.waitFor({ timeout: 15_000 });
                        await group.getByRole('button', { name: 'Start', exact: true }).click();
                        await group
                            .getByRole('button', { name: 'Complete', exact: true })
                            .waitFor({ timeout: 15_000 });
                        await group.getByRole('button', { name: 'Complete', exact: true }).click();
                        await group.waitFor({ state: 'detached', timeout: 15_000 });
                    }

                    await page.click('a[href="/"]');
                    await page.getByTestId('global-new-task').click();

                    const panel = page.locator('.task-panel');
                    await panel.waitFor({ timeout: 10_000 });
                    await panel.locator('input[placeholder="Title"]').fill(taskTitle);
                    await panel.getByRole('button', { name: 'Edit description' }).click();
                    await panel.locator('.task-panel__description textarea').fill(longDescription);

                    const create = panel.getByRole('button', { name: 'Create task', exact: true });
                    const deadline = Date.now() + 10_000;
                    while (!(await create.isEnabled()) && Date.now() < deadline) {
                        await page.waitForTimeout(100);
                    }
                    expect(await create.isEnabled()).toBe(true);
                    await create.click();
                    await panel.waitFor({ state: 'hidden', timeout: 10_000 });

                    await page.waitForSelector(`text=${taskTitle}`, { timeout: 15_000 });
                });
            } finally {
                await server.stop();
            }

            const filesAfter = await workspace.listTaskFiles();
            const newFiles = filesAfter.filter((file) => !filesBefore.includes(file));
            expect(newFiles).toHaveLength(1);

            const taskYaml = parse(await fs.readFile(newFiles[0]!, 'utf8')) as Record<string, any>;
            expect(taskYaml.title).toBe(taskTitle);
            expect(Buffer.byteLength(String(taskYaml.description ?? ''), 'utf8')).toBeGreaterThan(1024);
        } finally {
            await workspace.dispose();
        }
    });
});
