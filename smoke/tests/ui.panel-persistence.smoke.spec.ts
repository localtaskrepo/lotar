import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-54 durable browser coverage for the panel mutation contract:
// - task creation sends the project-scoped status atomically in the single
//   /api/tasks/add call (no add-then-status follow-up),
// - panel autosaves serialize per task and coalesce fields queued while a
//   response is in flight, with explicit null clears and custom fields riding
//   the same queue,
// - the results persist to the task YAML on disk and survive a page reload.
describe('UI task panel persistence contract', () => {
    it('creates a task with a custom project status in one atomic request that persists and survives reload', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            await workspace.addTask('Seed for project listing', { args: ['--project=PPS'] });
            await workspace.runLotar(['-p', 'PPS', 'config', 'set', 'issue_states', 'Queued,Active,Review,Complete']);

            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=PPS`, async page => {
                    const addBodies: Record<string, unknown>[] = [];
                    const statusCalls: unknown[] = [];
                    let createdId = '';
                    page.on('request', request => {
                        const pathname = new URL(request.url()).pathname;
                        if (pathname === '/api/tasks/add') addBodies.push(request.postDataJSON());
                        if (pathname === '/api/tasks/status') statusCalls.push(request.postDataJSON());
                    });
                    page.on('response', async response => {
                        if (new URL(response.url()).pathname === '/api/tasks/add' && response.ok()) {
                            try {
                                const payload = await response.json();
                                createdId = String(payload?.data?.id ?? '');
                            } catch {
                                // ignore body parse issues; id polled below
                            }
                        }
                    });

                    await page.getByRole('button', { name: 'Add task' }).click();
                    const panel = page.locator('.task-panel');
                    const create = panel.getByRole('button', { name: 'Create task', exact: true });
                    await expect.poll(() => create.isEnabled(), { timeout: 10_000 }).toBe(true);
                    await panel.locator('input[placeholder="Title"]').fill('Atomic status created');

                    const summarySelects = panel.locator('select');
                    await summarySelects.nth(2).selectOption({ label: 'Active' });

                    await create.click();
                    await expect.poll(() => addBodies.length, { timeout: 10_000 }).toBe(1);
                    expect(addBodies[0]).toMatchObject({
                        project: 'PPS',
                        title: 'Atomic status created',
                        status: 'Active',
                    });
                    // The status is part of creation; no follow-up status call.
                    await panel.waitFor({ state: 'hidden', timeout: 10_000 });
                    expect(statusCalls).toEqual([]);

                    // Durable: the YAML on disk carries the custom status.
                    await expect.poll(() => createdId, { timeout: 10_000 }).toBeTruthy();
                    expect(createdId).toMatch(/^PPS-/);
                    await expect
                        .poll(async () => workspace.readTaskYaml(createdId), { timeout: 10_000 })
                        .toContain('status: Active');

                    // Durable across reload.
                    await page.reload({ waitUntil: 'domcontentloaded' });
                    await page.waitForSelector('text=Atomic status created', { timeout: 15_000 });
                    expect(await page.locator('tr', { hasText: 'Atomic status created' }).locator('td', { hasText: 'Active' }).count()).toBeGreaterThan(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('serializes autosaves, coalesces queued null clears and custom fields, and persists them', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            await workspace.addTask('Seed for project listing', { args: ['--project=PPS'] });
            await workspace.runLotar(['-p', 'PPS', 'config', 'set', 'issue_states', 'Queued,Active,Review,Complete']);
            await workspace.runLotar(['-p', 'PPS', 'config', 'set', 'custom_fields', 'product']);
            const seeded = await workspace.addTask('Panel persistence seed', {
                args: ['--project=PPS', '--due=2030-01-02', '--effort=3h', '--field=product=Core'],
            });
            expect(seeded.project).toBe('PPS');

            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=PPS`, async page => {
                    let releaseFirst!: () => void;
                    const firstGate = new Promise<void>(resolve => { releaseFirst = resolve; });
                    let updateCount = 0;
                    const updates: Record<string, unknown>[] = [];
                    // Track in-flight route handlers so teardown can wait for
                    // their completion; the no-op catch keeps a late
                    // page-teardown abort from surfacing as an unhandled
                    // rejection while allSettled still observes the outcome.
                    const routeJobs = new Set<Promise<void>>();
                    const runRouteJob = (job: Promise<void>) => {
                        routeJobs.add(job);
                        job.catch(() => {});
                        job.finally(() => routeJobs.delete(job));
                    };
                    await page.route('**/api/tasks/update', async route => {
                        runRouteJob((async () => {
                            updateCount += 1;
                            updates.push(route.request().postDataJSON());
                            const response = await route.fetch();
                            if (updateCount === 1) await firstGate;
                            await route.fulfill({ response });
                        })());
                    });

                    try {
                    await page.waitForSelector('text=Panel persistence seed', { timeout: 15_000 });
                    await page.locator('tr', { hasText: 'Panel persistence seed' }).click();
                    const panel = page.locator('.task-panel');
                    await panel.waitFor({ state: 'visible', timeout: 10_000 });
                    await expect.poll(() => panel.locator('input[placeholder="Title"]').inputValue(), { timeout: 10_000 }).toBe('Panel persistence seed');

                    // First autosave: title edit. Its response is held back.
                    const title = panel.locator('input[placeholder="Title"]');
                    await title.fill('Panel persistence edited');
                    await title.blur();
                    await expect.poll(() => updateCount, { timeout: 10_000 }).toBe(1);
                    expect(updates[0]).toMatchObject({ id: seeded.id, title: 'Panel persistence edited' });

                    // While the first response is pending, queue a nullable
                    // clear and a custom-field edit; they must not be sent yet.
                    const dueDate = panel.locator('input[type="date"]');
                    await dueDate.fill('');
                    await dueDate.blur();
                    const effort = panel.locator('input[placeholder="Effort (e.g., 3d, 5h)"]');
                    await effort.fill('');
                    await effort.blur();
                    const customValue = panel.locator('.task-panel__custom-row input[placeholder="Value"]');
                    await customValue.fill('Edited');
                    await customValue.blur();
                    await page.waitForTimeout(500);
                    expect(updateCount).toBe(1);

                    // Releasing the first response flushes one coalesced patch.
                    releaseFirst();
                    await expect.poll(() => updateCount, { timeout: 10_000 }).toBe(2);
                    expect(updates[1]).toEqual({
                        id: seeded.id,
                        due_date: null,
                        effort: null,
                        custom_fields: { product: 'Edited' },
                    });

                    // Durable: YAML reflects every change once the queue drains.
                    // Poll the FULL end state: the title alone becomes durable
                    // while patch #1's response is still held (its server-side
                    // fetch ran before the gate), so polling only the title
                    // could pass before patch #2 is even sent and the one-shot
                    // reads below would race its server-side write.
                    await expect
                        .poll(async () => {
                            const yaml = await workspace.readTaskYaml(seeded.id);
                            return {
                                title: /title: Panel persistence edited/.test(yaml),
                                dueCleared: !yaml.includes('due_date:'),
                                effortCleared: !yaml.includes('effort:'),
                                productEdited: yaml.includes('product: Edited'),
                            };
                        }, { timeout: 10_000 })
                        .toEqual({ title: true, dueCleared: true, effortCleared: true, productEdited: true });
                    const yaml = await workspace.readTaskYaml(seeded.id);
                    expect(yaml).not.toContain('due_date:');
                    expect(yaml).not.toContain('effort:');
                    expect(yaml).toContain('product: Edited');

                    // Let any in-flight route handler finish and unregister the
                    // route before navigation so teardown never aborts a fetch.
                    await Promise.allSettled([...routeJobs]);
                    await page.unroute('**/api/tasks/update');
                    await page.locator('button[aria-label="Close panel"]').click();
                    await panel.waitFor({ state: 'hidden', timeout: 10_000 });

                    // Durable across reload.
                    await page.reload({ waitUntil: 'domcontentloaded' });
                    await page.waitForSelector('text=Panel persistence edited', { timeout: 15_000 });
                    expect(await page.locator('tr', { hasText: 'Panel persistence edited' }).locator('td', { hasText: '2030-01-02' }).count()).toBe(0);
                    } finally {
                        releaseFirst();
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
