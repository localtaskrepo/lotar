import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { html5DragAndDrop, withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-102 durable browser coverage for UI no-op mutation guards:
// - Board same-column drops, untouched custom-field blurs, re-selected
//   statuses, and same-sprint drops must send ZERO mutation requests and
//   leave the task/sprint YAML bytes, filesystem mtime, task `modified`,
//   and history untouched;
// - the genuine counterparts (cross-column drop, custom-field edit,
//   sprint move) must still send their real requests and persist.
//
// These specs exercise the real embedded UI against the real server; they
// intentionally assert ZERO client requests for no-ops so they hold even
// before/after any server-side durable no-op work lands.

interface TaskSnapshot {
    modified: string | null;
    historyLength: number;
}

async function taskSnapshot(serverUrl: string, id: string): Promise<TaskSnapshot> {
    const response = await fetch(`${serverUrl}/api/tasks/get?id=${encodeURIComponent(id)}`);
    const payload = (await response.json()) as { data?: { modified?: string | null; history?: unknown[] } };
    return {
        modified: payload.data?.modified ?? null,
        historyLength: Array.isArray(payload.data?.history) ? payload.data!.history!.length : 0,
    };
}

async function fileState(file: string): Promise<{ bytes: string; mtimeMs: number }> {
    const [bytes, stat] = await Promise.all([fs.readFile(file, 'utf8'), fs.stat(file)]);
    return { bytes, mtimeMs: stat.mtimeMs };
}

interface MutationPost {
    path: string;
    body: string | null;
}

/** Records every mutation POST path + body (GETs and SSE are not mutations). */
function trackMutationPosts(page: {
    on: (
        event: 'request',
        listener: (request: { method(): string; url(): string; postData(): string | null }) => void,
    ) => void;
}): MutationPost[] {
    const posts: MutationPost[] = [];
    page.on('request', (request) => {
        if (request.method() === 'POST' && new URL(request.url()).pathname.startsWith('/api/')) {
            posts.push({ path: new URL(request.url()).pathname, body: request.postData() });
        }
    });
    return posts;
}

describe.concurrent('UI no-op mutations (DEV-102)', () => {
    it('board same-column drop is request-free and byte-identical; a cross-column drop posts exactly once', async ({
        expect,
    }) => {
        const files: Record<string, string> = {
            '.tasks/config.yml': '{}\n',
            '.tasks/BRD/config.yml': stringify({
                project_name: 'Board no-op fixtures',
                issue: {
                    states: ['Todo', 'Doing', 'Done'],
                    priorities: ['Low', 'High'],
                    types: ['Task'],
                    done_states: ['Done'],
                },
            }),
            '.tasks/BRD/101.yml': stringify({
                title: 'No-op drop target',
                status: 'Todo',
                priority: 'Low',
                type: 'Task',
                reporter: 'Smoke',
                assignee: 'Ada',
                created: '2026-09-01T00:00:00Z',
                modified: '2026-10-01T00:00:00Z',
            }),
        };
        const workspace = await SmokeWorkspace.create({ name: 'board-noop-', seedFiles: files });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/boards?project=BRD`, async (page) => {
                    const card = page.locator('article.task').filter({ hasText: 'No-op drop target' });
                    await expect.poll(() => card.count()).toBe(1);

                    const file = path.join(workspace.tasksDir, 'BRD/101.yml');
                    const beforeFile = await fileState(file);
                    const beforeTask = await taskSnapshot(server.url, 'BRD-101');

                    // Same-column drop: zero mutation POSTs, no file change.
                    const posts = trackMutationPosts(page);
                    await card.dispatchEvent('dragstart');
                    await page.locator('.column[data-column-kind="status"][data-status="Todo"]').dispatchEvent('drop');
                    await page.waitForTimeout(750);
                    expect(posts).toEqual([]);

                    const afterNoopFile = await fileState(file);
                    expect(afterNoopFile.bytes).toBe(beforeFile.bytes);
                    expect(afterNoopFile.mtimeMs).toBe(beforeFile.mtimeMs);
                    const afterNoopTask = await taskSnapshot(server.url, 'BRD-101');
                    expect(afterNoopTask).toEqual(beforeTask);

                    // Genuine cross-column change: exactly one status POST,
                    // persisted status, and a real modification signal.
                    await card.dispatchEvent('dragstart');
                    await page.locator('.column[data-column-kind="status"][data-status="Doing"]').dispatchEvent('drop');
                    await expect.poll(() => posts.filter((p) => p.path === '/api/tasks/status').length).toBe(1);
                    expect(posts.map((p) => p.path)).toEqual(['/api/tasks/status']);
                    await expect
                        .poll(async () => (await taskSnapshot(server.url, 'BRD-101')).modified !== beforeTask.modified)
                        .toBe(true);
                    await expect
                        .poll(async () => {
                            const response = await fetch(`${server.url}/api/tasks/get?id=BRD-101`);
                            return ((await response.json()) as { data: { status: string } }).data.status;
                        })
                        .toBe('Doing');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('panel custom-field blur without edits and status re-selection send nothing; real edits post once', async ({
        expect,
    }) => {
        const workspace = await SmokeWorkspace.create({ name: 'panel-noop-' });
        try {
            await workspace.runLotar(['-p', 'PNP', 'config', 'set', 'issue_states', 'Queued,Active,Review,Complete']);
            await workspace.runLotar(['-p', 'PNP', 'config', 'set', 'custom_fields', 'product,points']);
            const seeded = await workspace.addTask('Panel no-op seed', {
                args: ['--project=PNP', '--status=Active', '--field=product=Core', '--field=points=5'],
            });

            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=PNP`, async (page) => {
                    await page.waitForSelector('text=Panel no-op seed', { timeout: 15_000 });
                    await page.locator('tr', { hasText: 'Panel no-op seed' }).click();
                    const panel = page.locator('.task-panel');
                    await panel.waitFor({ state: 'visible', timeout: 10_000 });
                    await expect
                        .poll(() => panel.locator('input[placeholder="Title"]').inputValue(), { timeout: 10_000 })
                        .toBe('Panel no-op seed');

                    const posts = trackMutationPosts(page);
                    const file = workspace.getTaskFilePath(seeded.id);
                    const beforeFile = await fileState(file);
                    const beforeTask = await taskSnapshot(server.url, seeded.id);

                    // The server DTO serializes custom_fields alphabetically
                    // (e.g. points before product), so rows must be addressed
                    // by their current field name — never by position.
                    const nameInputs = panel.locator('.task-panel__custom-row input[placeholder="Field name"]');
                    const valueInputs = panel.locator('.task-panel__custom-row input[placeholder="Value"]');
                    const productIndex = await nameInputs.evaluateAll((nodes) =>
                        nodes.findIndex((node) => (node as HTMLInputElement).value === 'product'),
                    );
                    expect(productIndex).toBeGreaterThanOrEqual(0);
                    const customName = nameInputs.nth(productIndex);
                    const customValue = valueInputs.nth(productIndex);

                    // Untouched custom-field blur (name and value inputs): no patch.
                    await customName.fill('product');
                    await customName.blur();
                    await customValue.fill('Core');
                    await customValue.blur();

                    // Re-selecting the current status in the dropdown: no patch.
                    const statusSelect = panel.locator('select').nth(2);
                    expect(await statusSelect.inputValue()).toBe('Active');
                    await statusSelect.selectOption({ label: 'Active' });

                    await page.waitForTimeout(750);
                    expect(posts).toEqual([]);
                    const afterNoopFile = await fileState(file);
                    expect(afterNoopFile.bytes).toBe(beforeFile.bytes);
                    expect(afterNoopFile.mtimeMs).toBe(beforeFile.mtimeMs);
                    expect(await taskSnapshot(server.url, seeded.id)).toEqual(beforeTask);

                    // A real custom-field edit: exactly one update POST, persisted.
                    await customValue.fill('Edited');
                    await customValue.blur();
                    await expect.poll(() => posts.filter((p) => p.path === '/api/tasks/update').length).toBe(1);
                    expect(posts.map((p) => p.path)).toEqual(['/api/tasks/update']);
                    await expect
                        .poll(() => workspace.readTaskYaml(seeded.id), { timeout: 10_000 })
                        .toContain('product: Edited');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('same-sprint drop sends no membership request; a real sprint move persists', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'sprints-noop-' });
        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'No-op Alpha']);
            await workspace.runLotar(['sprint', 'create', '--label', 'No-op Beta']);
            const task = await workspace.addTask('Sprint no-op seed', { args: ['--project', 'SNP'] });
            await workspace.runLotar(['sprint', 'add', task.id, '--sprint', '1']);

            const server = await startLotarServer(workspace);
            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.click('a[href="/sprints"]');
                    await page.waitForSelector('text=Sprints', { timeout: 15_000 });

                    const row = page.locator(`[data-sprint-id="1"] tr.task-row[data-task-id="${task.id}"]`);
                    await row.waitFor({ state: 'visible', timeout: 15_000 });

                    const taskFile = workspace.getTaskFilePath(task.id);
                    const sprintOne = path.join(workspace.tasksDir, '@sprints', '1.yml');
                    const beforeTaskFile = await fileState(taskFile);
                    const beforeSprintOne = await fileState(sprintOne);
                    const beforeTask = await taskSnapshot(server.url, task.id);

                    // Same-sprint drop: zero membership posts, nothing rewritten.
                    const posts = trackMutationPosts(page);
                    await html5DragAndDrop(
                        page,
                        `[data-sprint-id="1"] tr.task-row[data-task-id="${task.id}"]`,
                        '[data-sprint-id="1"]',
                    );
                    await page.waitForTimeout(750);
                    expect(posts).toEqual([]);
                    expect(await fileState(taskFile)).toEqual(beforeTaskFile);
                    expect(await fileState(sprintOne)).toEqual(beforeSprintOne);
                    expect(await taskSnapshot(server.url, task.id)).toEqual(beforeTask);

                    // Genuinely different sprint: membership posts flow and
                    // the move persists on both sides.
                    await html5DragAndDrop(
                        page,
                        `[data-sprint-id="1"] tr.task-row[data-task-id="${task.id}"]`,
                        '[data-sprint-id="2"]',
                    );
                    await expect.poll(() => posts.filter((p) => p.path === '/api/sprints/add').length).toBe(1);
                    await expect.poll(() => posts.filter((p) => p.path === '/api/sprints/remove').length).toBe(1);
                    await page.waitForSelector(`[data-sprint-id="2"] tr.task-row[data-task-id="${task.id}"]`, {
                        timeout: 10_000,
                    });
                    const sprintTwoRaw = await fs.readFile(path.join(workspace.tasksDir, '@sprints', '2.yml'), 'utf8');
                    expect(sprintTwoRaw).toContain(task.id);
                    const sprintOneAfter = await fs.readFile(sprintOne, 'utf8');
                    expect(sprintOneAfter).not.toContain(task.id);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
