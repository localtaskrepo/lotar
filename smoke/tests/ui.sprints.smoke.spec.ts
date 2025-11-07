import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

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

                    await page.dragAndDrop(sourceSelector, targetSelector);

                    await page.waitForSelector('[data-sprint-id="2"] tr.task-row[data-task-id="' + task.id + '"]', {
                        timeout: 10_000,
                    });

                    await page.waitForTimeout(250);
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
                        await page.dragAndDrop(sourceSelector, targetSelector);
                    } finally {
                        await page.keyboard.up(modifierKey);
                    }

                    await page.waitForSelector('[data-sprint-id="2"] tr.task-row[data-task-id="' + task.id + '"]', {
                        timeout: 10_000,
                    });

                    await page.waitForTimeout(300);

                    const sourceStillExists = await page.$(
                        '[data-sprint-id="1"] tr.task-row[data-task-id="' + task.id + '"]',
                    );
                    expect(sourceStillExists).not.toBeNull();
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

                    await page.click('.task-panel__sprint-add');
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
                    await page.waitForSelector('.sprint-delete__overlay', { timeout: 10_000 });
                    await page.click('.sprint-delete__actions .btn.danger');

                    await page.waitForSelector('.sprint-delete__overlay', {
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

            const taskYaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            const membership = Array.isArray(taskYaml.sprints) ? taskYaml.sprints : [];
            expect(membership).not.toContain(1);
        } finally {
            await workspace.dispose();
        }
    });
});
