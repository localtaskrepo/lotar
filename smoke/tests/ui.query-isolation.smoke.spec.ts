import fs from 'fs-extra';
import type { Page, Route } from '@playwright/test';
import { describe, expect, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-65 durable browser coverage: the keyed query layer must keep project/
// tag/assignee/status filters valid across live SSE updates, reject stale or
// failed query completions, and converge membership on the mounted consumers
// (Tasks/Board/Calendar) without any client-side predicate engine.
//
// All runtime mutations go through the REST API boundary (task add/update),
// which emits explicit task_created/task_updated SSE events with full DTOs —
// deterministic, independent of the filesystem watcher. Waits are event-driven
// (the SSE-triggered authoritative list refresh observed through page network
// traffic): no sleeps, no timing assertions, no manual-refresh fallback — every
// convergence below must happen automatically with no user reload.
//
// Separate evidence, kept out of these assertions (optional source followup,
// coordinator informed): the server-side file watcher very rarely drops a
// disk-write event (~1/20 writes in a standalone probe), which would stall
// watcher-driven live convergence until the next event. REST-emitted SSE is
// not affected. Pre-boot fixtures are seeded through the CLI before the server
// starts; every mutation after the server is live uses the REST boundary.
describe('UI DEV-65 query isolation and live convergence', () => {
    interface SeedSpec {
        readonly project: string;
        readonly title: string;
        readonly tags?: readonly string[];
        readonly status?: string;
        readonly assignee?: string;
        readonly dueDate?: string;
    }

    /** Seeds a task through the CLI in an isolated workspace and patches its
     * YAML so every field under test is set deterministically. */
    async function seedTask(workspace: SmokeWorkspace, spec: SeedSpec) {
        const created = await workspace.addTask(spec.title, {
            args: [`--project=${spec.project}`],
        });
        const file = created.filePath;
        const yaml = parse(await fs.readFile(file, 'utf8')) as Record<string, unknown>;
        if (spec.tags) yaml.tags = [...spec.tags];
        if (spec.status) yaml.status = spec.status;
        if (spec.assignee) yaml.assignee = spec.assignee;
        if (spec.dueDate) yaml.due_date = spec.dueDate;
        await fs.writeFile(file, stringify(yaml));
        return created;
    }

    /**
     * Performs a task mutation through the REST API boundary — the same one
     * the UI's own panel saves use — which emits an explicit SSE event with
     * the full updated DTO. Returns the API's response envelope.
     */
    async function apiMutate(server: { url: string }, path: string, body: Record<string, unknown>) {
        const res = await fetch(`${server.url}${path}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
            body: JSON.stringify(body),
        });
        const text = await res.text();
        if (!res.ok) {
            throw new Error(`POST ${path} failed: ${res.status} ${text}`);
        }
        return text ? (JSON.parse(text) as { data?: unknown }) : {};
    }

    /** Tracks /api/tasks/list traffic so tests wait for fully settled
     * hydrations and can assert the exact scope of every request. */
    function trackListRequests(page: Page) {
        const urls: string[] = [];
        const pending = new Set<unknown>();
        page.on('request', (request) => {
            if (new URL(request.url()).pathname === '/api/tasks/list') {
                urls.push(request.url());
                pending.add(request);
            }
        });
        page.on('response', (response) => {
            if (new URL(response.url()).pathname === '/api/tasks/list') {
                response
                    .finished()
                    .catch(() => {})
                    .finally(() => pending.delete(response.request()));
            }
        });
        page.on('requestfailed', (request) => pending.delete(request));
        return {
            urls,
            countWith(param: string, value: string): number {
                return urls.filter((url) => new URL(url).searchParams.get(param) === value).length;
            },
            /** Waits until at least `minTotal` list requests were observed and
             * none is still in flight (a pending response is not a hydration). */
            async settle(minTotal: number) {
                await expect
                    .poll(() => pending.size === 0 && urls.length >= minTotal, { timeout: 15_000 })
                    .toBe(true);
            },
            /**
             * Waits for the next authoritative list refresh after an API
             * mutation, proving the automatic SSE → retained-query refresh
             * path actually ran before any absence assertion (no vacuous
             * passes, no manual refresh).
             */
            async waitForNextRequest(afterCount: number) {
                await expect
                    .poll(() => urls.length, { timeout: 15_000 })
                    .toBeGreaterThan(afterCount);
                await this.settle(urls.length);
            },
        };
    }

    async function openFilterPanel(page: Page) {
        await page.click('[data-testid="filter-toggle"]');
        const panel = page.locator('[data-testid="filter-panel"]');
        await panel.waitFor({ state: 'visible', timeout: 10_000 });
        return panel.locator('input[placeholder="Tags"]');
    }

    function localDateKey(date = new Date()): string {
        return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
    }

    it('keeps a foreign-project task out of the active filtered view while it stays reachable via its own project', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            // The CLI truncates project names to storage prefixes (FRESH ->
            // FRES), so every assertion uses the prefix the tasks actually got.
            const baseSeed = await seedTask(workspace, { project: 'BASE', title: 'Dev65 base anchor' });
            const freshSeed = await seedTask(workspace, { project: 'FRESH', title: 'Dev65 fresh anchor' });
            const basePrefix = baseSeed.project;
            const freshPrefix = freshSeed.project;

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(`${server.url}/?project=${basePrefix}`, async (page) => {
                    const lists = trackListRequests(page);
                    await page.waitForSelector('tbody tr:has-text("Dev65 base anchor")', { timeout: 20_000 });
                    await lists.settle(1);

                    // A task is created in FRESH (outside the active BASE
                    // filter) through the REST API boundary while the BASE
                    // view is live; its task_created SSE (full DTO) must
                    // trigger the automatic refresh of the retained BASE query.
                    const beforeCreate = lists.urls.length;
                    await apiMutate(server, '/api/tasks/add', {
                        title: 'Dev65 foreign late task',
                        project: freshPrefix,
                    });
                    await lists.waitForNextRequest(beforeCreate);

                    // The foreign task never joins the BASE-filtered view…
                    await expect
                        .poll(() => page.locator('tbody tr', { hasText: 'Dev65 foreign late task' }).count())
                        .toBe(0);
                    await expect.poll(() => page.locator('h1').textContent()).toContain('(1)');

                    // …but switching the filter to FRESH reveals it next to
                    // its project sibling, and the BASE row does not leak in.
                    await openFilterPanel(page);
                    const projectFilter = page.locator('[data-testid="filter-project"]');
                    await projectFilter.waitFor({ state: 'visible', timeout: 10_000 });
                    await expect
                        .poll(() => projectFilter.locator(`option[value="${freshPrefix}"]`).count())
                        .toBeGreaterThan(0);
                    await projectFilter.selectOption(freshPrefix);
                    await page.waitForSelector('tbody tr:has-text("Dev65 foreign late task")', {
                        timeout: 15_000,
                    });
                    await page.waitForSelector('tbody tr:has-text("Dev65 fresh anchor")', {
                        timeout: 15_000,
                    });
                    await expect.poll(() => page.locator('h1').textContent()).toContain('(2)');
                    await expect
                        .poll(() => page.locator('tbody tr', { hasText: 'Dev65 base anchor' }).count())
                        .toBe(0);

                    // Exact request scoping: every observed list query stayed
                    // project-scoped — the SSE refresh never broadened BASE.
                    expect(lists.countWith('project', basePrefix)).toBeGreaterThan(0);
                    expect(lists.countWith('project', freshPrefix)).toBeGreaterThan(0);
                    for (const url of lists.urls) {
                        expect([basePrefix, freshPrefix, null]).toContain(
                            new URL(url).searchParams.get('project'),
                        );
                    }
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('rejects a late stale project query response after the filter switched', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const baseSeed = await seedTask(workspace, { project: 'BASE', title: 'Dev65 stale base anchor' });
            const freshSeed = await seedTask(workspace, { project: 'FRESH', title: 'Dev65 stale fresh anchor' });
            const basePrefix = baseSeed.project;
            const freshPrefix = freshSeed.project;

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(`${server.url}/?project=${basePrefix}`, async (page) => {
                    const lists = trackListRequests(page);
                    await page.waitForSelector('tbody tr:has-text("Dev65 stale base anchor")', {
                        timeout: 20_000,
                    });
                    await lists.settle(1);

                    // Hold every subsequent BASE-scoped list response so its
                    // completion is deterministically ordered AFTER the switch.
                    const held: Array<{ route: Route; url: string }> = [];
                    let holdBase = true;
                    await page.route('**/api/tasks/list*', async (route) => {
                        const params = new URL(route.request().url()).searchParams;
                        if (holdBase && params.get('project') === basePrefix) {
                            held.push({ route, url: route.request().url() });
                            return; // released manually below
                        }
                        await route.continue();
                    });

                    await page.getByRole('button', { name: 'Refresh tasks' }).click();
                    await expect.poll(() => held.length, { timeout: 10_000 }).toBeGreaterThan(0);
                    // Rows from the last completed response stay visible while
                    // the refresh is merely in flight.
                    await page.waitForSelector('tbody tr:has-text("Dev65 stale base anchor")');

                    // Switch the filtered view to FRESH while BASE is held.
                    await openFilterPanel(page);
                    const projectFilter = page.locator('[data-testid="filter-project"]');
                    await projectFilter.waitFor({ state: 'visible', timeout: 10_000 });
                    await expect
                        .poll(() => projectFilter.locator(`option[value="${freshPrefix}"]`).count())
                        .toBeGreaterThan(0);
                    await projectFilter.selectOption(freshPrefix);
                    // The stale BASE rows must leave the view immediately…
                    await expect
                        .poll(() => page.locator('tbody tr', { hasText: 'Dev65 stale base anchor' }).count())
                        .toBe(0);

                    // …and letting the late BASE response finally land must not
                    // publish its rows over the FRESH view.
                    await Promise.all(held.map((entry) => entry.route.continue()));
                    await page.waitForSelector('tbody tr:has-text("Dev65 stale fresh anchor")', {
                        timeout: 15_000,
                    });
                    await lists.settle(lists.urls.length);
                    await expect
                        .poll(() => page.locator('tbody tr', { hasText: 'Dev65 stale base anchor' }).count())
                        .toBe(0);
                    await expect.poll(() => page.locator('h1').textContent()).toContain('(1)');

                    // Exact requests: every held response was BASE-scoped, and
                    // the view-serving query after the switch was FRESH-scoped.
                    for (const entry of held) {
                        expect(new URL(entry.url).searchParams.get('project')).toBe(basePrefix);
                    }
                    expect(lists.countWith('project', freshPrefix)).toBeGreaterThan(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('converges tag, assignee, and status filter membership after out-of-band edits', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const keeper = await seedTask(workspace, {
                project: 'BASE',
                title: 'Dev65 flux keeper',
                tags: ['flux'],
                status: 'Todo',
            });
            const entrant = await seedTask(workspace, {
                project: 'BASE',
                title: 'Dev65 flux entrant',
                status: 'Todo',
            });
            const amy = await seedTask(workspace, {
                project: 'BASE',
                title: 'Dev65 flux amy done',
                tags: ['flux'],
                status: 'Done',
                assignee: 'amy',
            });

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(`${server.url}/?project=BASE`, async (page) => {
                    const lists = trackListRequests(page);
                    await page.waitForSelector('tbody tr:has-text("Dev65 flux keeper")', {
                        timeout: 20_000,
                    });
                    await lists.settle(1);

                    // Tag filter: only tagged rows are members.
                    const tagsInput = await openFilterPanel(page);
                    await tagsInput.fill('flux');
                    await expect
                        .poll(() => lists.countWith('tags', 'flux'), { timeout: 10_000 })
                        .toBeGreaterThan(0);
                    await lists.settle(lists.urls.length);
                    await page.waitForSelector('tbody tr:has-text("Dev65 flux amy done")', { timeout: 15_000 });
                    await expect
                        .poll(() => page.locator('tbody tr', { hasText: 'Dev65 flux entrant' }).count())
                        .toBe(0);
                    await expect.poll(() => page.locator('h1').textContent()).toContain('(2)');

                    // Outgoing: the keeper loses the tag on disk.
                    let seen = lists.urls.length;
                    await apiMutate(server, '/api/tasks/update', { id: keeper.id, tags: [] });
                    await lists.waitForNextRequest(seen);
                    await expect
                        .poll(() => page.locator('tbody tr', { hasText: 'Dev65 flux keeper' }).count())
                        .toBe(0);
                    await expect.poll(() => page.locator('h1').textContent()).toContain('(1)');

                    // Incoming: the entrant gains the tag on disk.
                    seen = lists.urls.length;
                    await apiMutate(server, '/api/tasks/update', { id: entrant.id, tags: ['flux'] });
                    await lists.waitForNextRequest(seen);
                    await page.waitForSelector('tbody tr:has-text("Dev65 flux entrant")', { timeout: 15_000 });
                    await expect.poll(() => page.locator('h1').textContent()).toContain('(2)');

                    // Assignee narrowing stacked on the tag filter.
                    const search = page.locator('[data-testid="filter-search"]');
                    await search.fill('assignee:amy');
                    await search.press('Enter');
                    await expect
                        .poll(() => lists.countWith('assignee', 'amy'), { timeout: 10_000 })
                        .toBeGreaterThan(0);
                    await lists.settle(lists.urls.length);
                    await page.waitForSelector('tbody tr:has-text("Dev65 flux amy done")', { timeout: 15_000 });
                    await expect
                        .poll(() => page.locator('tbody tr', { hasText: 'Dev65 flux entrant' }).count())
                        .toBe(0);
                    await expect.poll(() => page.locator('h1').textContent()).toContain('(1)');

                    // Outgoing: the assignee is removed on disk — the filtered
                    // view converges to the empty state, not to stale rows.
                    seen = lists.urls.length;
                    await apiMutate(server, '/api/tasks/update', { id: amy.id, assignee: null });
                    await lists.waitForNextRequest(seen);
                    await page.waitForSelector('text=No tasks match your filters', { timeout: 15_000 });
                    await expect
                        .poll(() => page.locator('tbody tr', { hasText: 'Dev65 flux amy done' }).count())
                        .toBe(0);

                    // Status filter convergence after clearing. The toolbar
                    // button (title attr) — the "No tasks" empty state renders
                    // a second Clear-filters button with the same name.
                    await page.locator('button[title="Clear filters"]').click();
                    await page.waitForSelector('tbody tr:has-text("Dev65 flux amy done")', { timeout: 15_000 });
                    await lists.settle(lists.urls.length);
                    await page.click('[data-testid="filter-status"]');
                    await page
                        .locator('.filter-bar__menu-item', { hasText: 'Todo' })
                        .locator('input')
                        .check();
                    await expect
                        .poll(() => lists.countWith('status', 'Todo'), { timeout: 10_000 })
                        .toBeGreaterThan(0);
                    await lists.settle(lists.urls.length);
                    await page.waitForSelector('tbody tr:has-text("Dev65 flux keeper")', { timeout: 15_000 });
                    await page.waitForSelector('tbody tr:has-text("Dev65 flux entrant")', { timeout: 15_000 });
                    await expect
                        .poll(() => page.locator('tbody tr', { hasText: 'Dev65 flux amy done' }).count())
                        .toBe(0);
                    await expect.poll(() => page.locator('h1').textContent()).toContain('(2)');

                    // Outgoing: keeper leaves the status filter on disk.
                    seen = lists.urls.length;
                    await apiMutate(server, '/api/tasks/update', { id: keeper.id, status: 'Done' });
                    await lists.waitForNextRequest(seen);
                    await expect
                        .poll(() => page.locator('tbody tr', { hasText: 'Dev65 flux keeper' }).count())
                        .toBe(0);
                    await expect.poll(() => page.locator('h1').textContent()).toContain('(1)');
                    await page.waitForSelector('tbody tr:has-text("Dev65 flux entrant")', { timeout: 15_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('moves board cards in and out of a tag-filtered board without an Other partition', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const keeper = await seedTask(workspace, {
                project: 'BASE',
                title: 'Dev65 board keeper',
                tags: ['boardset'],
                status: 'Todo',
            });
            const entrant = await seedTask(workspace, {
                project: 'BASE',
                title: 'Dev65 board entrant',
                status: 'Todo',
            });
            await seedTask(workspace, {
                project: 'BASE',
                title: 'Dev65 board done card',
                tags: ['boardset'],
                status: 'Done',
            });

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(`${server.url}/boards?project=BASE`, async (page) => {
                    const lists = trackListRequests(page);
                    await page.waitForSelector('[data-status="Todo"] article:has-text("Dev65 board keeper")', {
                        timeout: 20_000,
                    });
                    await lists.settle(1);

                    // Tag filter: membership is decided by the server query.
                    const tagsInput = await openFilterPanel(page);
                    await tagsInput.fill('boardset');
                    await expect
                        .poll(() => lists.countWith('tags', 'boardset'), { timeout: 10_000 })
                        .toBeGreaterThan(0);
                    await lists.settle(lists.urls.length);
                    await page.waitForSelector('[data-status="Done"] article:has-text("Dev65 board done card")', {
                        timeout: 15_000,
                    });
                    expect(
                        await page.locator('[data-status="Todo"] article', { hasText: 'Dev65 board keeper' }).count(),
                    ).toBe(1);
                    await expect
                        .poll(() => page.locator('[data-status] article', { hasText: 'Dev65 board entrant' }).count())
                        .toBe(0);
                    // Seeded statuses are canonical, so nothing partitions into
                    // the DEV-68 "Other" column.
                    expect(await page.locator('[data-status="__other__"]').count()).toBe(0);

                    // Incoming: the entrant gains the tag on disk.
                    let seen = lists.urls.length;
                    await apiMutate(server, '/api/tasks/update', { id: entrant.id, tags: ['boardset'] });
                    await lists.waitForNextRequest(seen);
                    await page.waitForSelector('[data-status="Todo"] article:has-text("Dev65 board entrant")', {
                        timeout: 15_000,
                    });
                    expect(
                        await page.locator('[data-status="Todo"] article', { hasText: 'Dev65 board keeper' }).count(),
                    ).toBe(1);

                    // Outgoing: the keeper loses the tag and leaves the board.
                    seen = lists.urls.length;
                    await apiMutate(server, '/api/tasks/update', { id: keeper.id, tags: [] });
                    await lists.waitForNextRequest(seen);
                    await expect
                        .poll(() => page.locator('[data-status] article', { hasText: 'Dev65 board keeper' }).count())
                        .toBe(0);
                    await expect
                        .poll(() => page.locator('[data-status="Todo"] article').count())
                        .toBe(1);
                    expect(
                        await page.locator('[data-status="Done"] article', { hasText: 'Dev65 board done card' }).count(),
                    ).toBe(1);

                    // The whole sequence stayed live: no refresh failure was
                    // surfaced and no error state replaced the board.
                    expect(await page.getByText("We couldn't load board tasks").count()).toBe(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('surfaces failed calendar query switches without stale rows and recovers via retry', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const today = localDateKey();
            const anchor = await seedTask(workspace, {
                project: 'BASE',
                title: 'Dev65 calendar anchor',
                tags: ['calalpha'],
                dueDate: today,
            });
            const relief = await seedTask(workspace, {
                project: 'BASE',
                title: 'Dev65 calendar relief',
                tags: ['calbeta'],
                dueDate: today,
            });

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(`${server.url}/calendar?project=BASE`, async (page) => {
                    const lists = trackListRequests(page);
                    await page.waitForSelector(
                        `[data-date="${today}"] .task-item:has-text("Dev65 calendar anchor")`,
                        { timeout: 20_000 },
                    );
                    await lists.settle(1);

                    // Filter to one tag set; the other task must not render.
                    const tagsInput = await openFilterPanel(page);
                    await tagsInput.fill('calalpha');
                    await expect
                        .poll(() => lists.countWith('tags', 'calalpha'), { timeout: 10_000 })
                        .toBeGreaterThan(0);
                    await lists.settle(lists.urls.length);
                    await expect
                        .poll(() => page.locator('.task-item', { hasText: 'Dev65 calendar relief' }).count())
                        .toBe(0);

                    // The next query switch fails at the transport level.
                    let abortTags: string | null = 'calbeta';
                    await page.route('**/api/tasks/list*', async (route) => {
                        const params = new URL(route.request().url()).searchParams;
                        if (abortTags && params.get('tags') === abortTags) {
                            await route.abort('failed');
                            return;
                        }
                        await route.continue();
                    });
                    await tagsInput.fill('calbeta');
                    // Failed NEW query: a refresh failure leaves membership
                    // untouched — empty for the fresh calbeta key — so the
                    // banner surfaces while the grid stays mounted as a
                    // stable, row-less background.
                    const banner = page.locator('.refresh-error');
                    await banner.waitFor({ state: 'visible', timeout: 15_000 });
                    await expect
                        .poll(() => banner.textContent())
                        .toContain('Calendar refresh failed:');
                    await expect
                        .poll(() => page.locator('.task-item', { hasText: 'Dev65 calendar anchor' }).count())
                        .toBe(0);
                    await expect.poll(() => page.locator('.calendar').count()).toBe(1);
                    const retry = page.getByRole('button', { name: 'Retry' });
                    await expect.poll(() => retry.isEnabled(), { timeout: 10_000 }).toBe(true);

                    // Retry succeeds once the transport recovers.
                    abortTags = null;
                    await retry.click();
                    await page.waitForSelector(
                        `[data-date="${today}"] .task-item:has-text("Dev65 calendar relief")`,
                        { timeout: 15_000 },
                    );
                    expect(await banner.count()).toBe(0);

                    // Live membership moves on the recovered view: outgoing…
                    let seen = lists.urls.length;
                    await apiMutate(server, '/api/tasks/update', { id: relief.id, tags: ['calalpha'] });
                    await lists.waitForNextRequest(seen);
                    await expect
                        .poll(() => page.locator('.task-item', { hasText: 'Dev65 calendar relief' }).count())
                        .toBe(0);

                    // …and incoming.
                    seen = lists.urls.length;
                    await apiMutate(server, '/api/tasks/update', { id: anchor.id, tags: ['calbeta'] });
                    await lists.waitForNextRequest(seen);
                    await page.waitForSelector(
                        `[data-date="${today}"] .task-item:has-text("Dev65 calendar anchor")`,
                        { timeout: 15_000 },
                    );
                    expect(await banner.count()).toBe(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
