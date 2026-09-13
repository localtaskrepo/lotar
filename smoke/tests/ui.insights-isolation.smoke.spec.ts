import fs from 'fs-extra';
import type { BrowserContext, Page, Route } from '@playwright/test';
import { describe, expect, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser, withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-66 durable browser coverage: Insights and the activity drawer must own
// query-scoped refreshable state. Opening/refreshing the drawer's global 30d /
// limit-200 feed can never alter the Insights chart (project + window + limit
// 400); rapid project/window switches settle on the latest selection even when
// an older scoped response lands late; Reload after a REST mutation refreshes
// the stats-driven Projects overview totals, not just task-derived tiles.
//
// Backend reality: /api/activity/feed requires a git repository root, and these
// fixtures are forbidden from running git commands, so the real endpoint
// answers 400 "Not inside a git repository" in the isolated workspaces. Tests
// that need deterministic feed DATA therefore intercept only that route with
// fixture payloads shaped exactly like view/api/types.ts ActivityFeedItem and
// are labelled "(mocked feed)" — they still assert the exact request scope of
// every feed call. Task queries, project stats, REST mutations, and SSE-driven
// convergence always run against the actual backend; those tests are labelled
// "(actual backend)". Mutations go exclusively through the REST boundary
// (explicit task_updated SSE with full DTOs), never the filesystem watcher.
//
// Waits are event-driven (observed network traffic and published UI state): no
// sleeps, no timing assertions. The only intentional manual refresh is the
// Reload scenario, which is itself the behavior under test.
describe('UI DEV-66 Insights and activity feed query isolation', () => {
    interface SeedSpec {
        readonly project: string;
        readonly title: string;
        readonly tags?: readonly string[];
        readonly status?: string;
    }

    /** Seeds a task through the CLI in an isolated workspace and patches its
     * YAML so every field under test is set deterministically (pre-boot only;
     * after the server starts, all mutations go through REST). */
    async function seedTask(workspace: SmokeWorkspace, spec: SeedSpec) {
        const created = await workspace.addTask(spec.title, {
            args: [`--project=${spec.project}`],
        });
        const file = created.filePath;
        const yaml = parse(await fs.readFile(file, 'utf8')) as Record<string, unknown>;
        if (spec.tags) yaml.tags = [...spec.tags];
        if (spec.status) yaml.status = spec.status;
        await fs.writeFile(file, stringify(yaml));
        return created;
    }

    /** Performs a task mutation through the REST API boundary — the same one
     * the UI's own panel saves use — which emits an explicit SSE event with
     * the full updated DTO (deterministic, watcher-independent). */
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

    /** Tracks traffic for one API pathname so tests wait for fully settled
     * hydrations and can assert the exact scope of every request. */
    function trackRequests(page: Page, pathname: string) {
        const urls: string[] = [];
        const pending = new Set<unknown>();
        page.on('request', (request) => {
            if (new URL(request.url()).pathname === pathname) {
                urls.push(request.url());
                pending.add(request);
            }
        });
        page.on('response', (response) => {
            if (new URL(response.url()).pathname === pathname) {
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
            countUnscoped(param: string): number {
                return urls.filter((url) => new URL(url).searchParams.get(param) === null).length;
            },
            async settle(minTotal: number) {
                await expect
                    .poll(() => pending.size === 0 && urls.length >= minTotal, { timeout: 15_000 })
                    .toBe(true);
            },
            async waitForNextRequest(afterCount: number) {
                await expect
                    .poll(() => urls.length, { timeout: 15_000 })
                    .toBeGreaterThan(afterCount);
                await this.settle(urls.length);
            },
        };
    }

    /** Opens a page with routing installed on the context BEFORE navigation,
     * so even the mount-time requests are deterministic. */
    async function withRoutedPage<T>(
        url: string,
        setup: (context: BrowserContext) => Promise<void> | void,
        callback: (page: Page) => Promise<T>,
    ): Promise<T> {
        return withBrowser({}, async (context) => {
            await setup(context);
            const page = await context.newPage();
            await page.goto(url, { waitUntil: 'domcontentloaded' });
            try {
                return await callback(page);
            } finally {
                await page.close();
            }
        });
    }

    // ---- Feed interception ---------------------------------------------------

    interface FeedPayload {
        readonly taskId: string;
        readonly title: string;
        readonly changes: number;
    }

    /** Fixture payload shaped from view/api/types.ts ActivityFeedItem. All
     * history entries land "now", so every change counts inside any window
     * and the expected average is exactly changes/windowDays. */
    function feedFixture(taskId: string, title: string, changes: number) {
        const nowIso = new Date().toISOString();
        return {
            commit: `smokefixture-${taskId}`,
            author: 'Smoke Fixture',
            email: 'smoke@example.invalid',
            date: nowIso,
            message: `Fixture feed item for ${taskId}`,
            task_id: taskId,
            task_title: title,
            history: [
                {
                    at: nowIso,
                    actor: 'Smoke Fixture',
                    changes: Array.from({ length: changes }, (_, index) => ({
                        field: `field_${index}`,
                        kind: 'other',
                        old: null,
                        new: 'fixture',
                    })),
                },
            ],
        };
    }

    /** Window length implied by a feed request's since/until params, in days
     * (the browser computes since as the local start of day N-1 days ago). */
    function deriveWindowDays(url: string): number {
        const params = new URL(url).searchParams;
        const since = Date.parse(params.get('since') ?? '');
        const until = Date.parse(params.get('until') ?? '');
        if (!Number.isFinite(since) || !Number.isFinite(until)) return Number.NaN;
        return Math.floor((until - since) / 86_400_000) + 1;
    }

    /** Local calendar day the feed request's `since` must start from for the
     * given window, computed the same way useActivity does it. */
    function expectedSinceDay(windowDays: number): string {
        const start = new Date();
        start.setDate(start.getDate() - (windowDays - 1));
        start.setHours(0, 0, 0, 0);
        return start.toISOString().slice(0, 10);
    }

    /** Intercepts only /api/activity/feed. `payloadFor` decides the fixture
     * data per request scope; `shouldHold` defers selected requests so their
     * responses land late, after the UI moved on. */
    async function installFeedRoute(
        context: BrowserContext,
        payloadFor: (params: URLSearchParams, url: string) => FeedPayload,
        shouldHold: (params: URLSearchParams, url: string) => boolean = () => false,
    ) {
        const held: Array<{ route: Route; url: string }> = [];
        await context.route('**/api/activity/feed*', async (route) => {
            const url = route.request().url();
            const params = new URL(url).searchParams;
            if (shouldHold(params, url)) {
                held.push({ route, url });
                return;
            }
            const payload = payloadFor(params, url);
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify({ data: [feedFixture(payload.taskId, payload.title, payload.changes)] }),
            });
        });
        return {
            held,
            /** Releases every held route with the payload its scope deserves,
             * exactly like a late server response finally arriving. */
            async releaseHeld(): Promise<ReadonlyArray<{ route: Route; url: string }>> {
                const released = held.splice(0);
                for (const entry of released) {
                    const payload = payloadFor(new URL(entry.url).searchParams, entry.url);
                    await entry.route.fulfill({
                        status: 200,
                        contentType: 'application/json',
                        body: JSON.stringify({ data: [feedFixture(payload.taskId, payload.title, payload.changes)] }),
                    });
                }
                return released;
            },
        };
    }

    type FeedRouteControls = Awaited<ReturnType<typeof installFeedRoute>>;

    // ---- Durable UI probes ---------------------------------------------------

    function tile(page: Page, label: string) {
        return page.locator('.summary-tile', { hasText: label }).locator('.summary-value');
    }

    async function tileValue(page: Page, label: string): Promise<string> {
        return ((await tile(page, label).textContent()) ?? '').trim();
    }

    function averageCaption(page: Page) {
        return page.locator('.activity-card').getByText(/Average [\d.]+ updates \/ day/);
    }

    function distributionRow(page: Page, heading: string, label: string) {
        return page.locator(
            `xpath=//h3[normalize-space()="${heading}"]/following::table[1]//tbody/tr[th="${label}"]`,
        );
    }

    async function distributionCount(page: Page, heading: string, label: string): Promise<string> {
        return ((await distributionRow(page, heading, label).locator('td').first().textContent()) ?? '').trim();
    }

    /** Projects overview "Active" (open_count) cell for a project prefix. */
    async function overviewActive(page: Page, prefix: string): Promise<string> {
        const row = page
            .locator('xpath=//h3[normalize-space()="Projects overview"]/following::table[1]')
            .locator('tbody tr')
            .filter({ hasText: prefix });
        return ((await row.locator('td').first().textContent()) ?? '').trim();
    }

    it('drawer open and feed refresh cannot alter the Insights activity chart (mocked feed, isolated scopes)', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const anchor = await seedTask(workspace, { project: 'BASE', title: 'Dev66 drawer anchor', status: 'Todo' });
            await seedTask(workspace, { project: 'BASE', title: 'Dev66 drawer second', status: 'Todo' });
            const basePrefix = anchor.project;

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withRoutedPage(
                    `${server.url}/insights?project=${basePrefix}`,
                    async (context) => {
                        await installFeedRoute(context, (params) => {
                            // Two deliberately different payloads: if the
                            // drawer's global response could leak into the
                            // Insights feed, the average would double.
                            if (params.get('project') === basePrefix) {
                                return { taskId: anchor.id, title: 'Dev66 drawer anchor', changes: 60 };
                            }
                            return { taskId: anchor.id, title: 'Dev66 drawer anchor', changes: 120 };
                        });
                    },
                    async (page) => {
                        const lists = trackRequests(page, '/api/tasks/list');
                        const feeds = trackRequests(page, '/api/activity/feed');

                        // 60 fixture changes over the 30d project window.
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('2.0');
                        await feeds.settle(feeds.urls.length);
                        await lists.settle(1);
                        expect(await averageCaption(page).textContent()).toContain('Average 2.0 updates / day');

                        // Open the activity drawer: its scope is global 30d /
                        // limit 200 and must publish only into its own feed.
                        await page.getByRole('button', { name: 'Activity', exact: true }).click();
                        const drawer = page.locator('[role="dialog"]');
                        await drawer.waitFor({ state: 'visible', timeout: 10_000 });
                        await expect
                            .poll(() => feeds.countUnscoped('project'), { timeout: 10_000 })
                            .toBeGreaterThan(0);
                        await feeds.settle(feeds.urls.length);
                        // The drawer really rendered the mocked global feed…
                        await expect.poll(() => drawer.locator('.feed-item').count()).toBeGreaterThan(0);
                        // …while the Insights average did not move.
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('2.0');
                        expect(await averageCaption(page).textContent()).toContain('Average 2.0 updates / day');

                        // Manual refresh of the drawer feed refetches for real
                        // and still cannot touch the Insights scope.
                        const beforeRefresh = feeds.urls.length;
                        await drawer.locator('button[title="Refresh activity feed"]').click();
                        await feeds.waitForNextRequest(beforeRefresh);
                        await expect.poll(() => drawer.locator('.feed-item').count()).toBeGreaterThan(0);
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('2.0');
                        expect(await averageCaption(page).textContent()).toContain('Average 2.0 updates / day');

                        // Exact request scopes: Insights = project + limit 400,
                        // drawer = global + limit 200, both ~30d windows.
                        expect(feeds.countWith('project', basePrefix)).toBeGreaterThan(0);
                        expect(feeds.countWith('limit', '400')).toBe(feeds.countWith('project', basePrefix));
                        expect(feeds.countWith('limit', '200')).toBe(feeds.countUnscoped('project'));
                        expect(feeds.countWith('limit', '200')).toBeGreaterThanOrEqual(2);
                        for (const url of feeds.urls) {
                            const params = new URL(url).searchParams;
                            expect([basePrefix, null]).toContain(params.get('project'));
                            expect(['200', '400']).toContain(params.get('limit'));
                            expect(Math.abs(deriveWindowDays(url) - 30)).toBeLessThanOrEqual(1);
                            expect(params.get('since')).toContain(expectedSinceDay(30));
                        }
                        // The task query itself stayed project-scoped throughout.
                        expect(lists.urls.length).toBeGreaterThan(0);
                        for (const url of lists.urls) {
                            expect(new URL(url).searchParams.get('project')).toBe(basePrefix);
                        }
                    },
                );
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('rapid project and window switches settle on the latest selection despite held late responses (mocked feed)', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const baseSeed = await seedTask(workspace, { project: 'BASE', title: 'Dev66 switch base anchor', status: 'Todo' });
            const freshSeed = await seedTask(workspace, { project: 'FRESH', title: 'Dev66 switch fresh anchor', status: 'Todo' });
            const basePrefix = baseSeed.project;
            const freshPrefix = freshSeed.project;

            // BASE carries 60 fixture changes, FRESH 30, so every project and
            // window combination yields a distinct expected average.
            const payloadFor = (params: URLSearchParams): FeedPayload => {
                if (params.get('project') === freshPrefix) {
                    return { taskId: freshSeed.id, title: 'Dev66 switch fresh anchor', changes: 30 };
                }
                return { taskId: baseSeed.id, title: 'Dev66 switch base anchor', changes: 60 };
            };
            const holdWindows = new Set<number>();
            const holdProjects = new Set<string>();
            let controls: FeedRouteControls | null = null;

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withRoutedPage(
                    `${server.url}/insights?project=${basePrefix}`,
                    async (context) => {
                        controls = await installFeedRoute(
                            context,
                            payloadFor,
                            (params, url) =>
                                holdWindows.has(deriveWindowDays(url)) ||
                                (params.get('project') !== null && holdProjects.has(params.get('project') as string)),
                        );
                    },
                    async (page) => {
                        if (!controls) throw new Error('feed route controls missing');
                        const lists = trackRequests(page, '/api/tasks/list');
                        const feeds = trackRequests(page, '/api/activity/feed');

                        // Baseline: BASE project, 30d window -> 60/30 = 2.0.
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('2.0');
                        await feeds.settle(feeds.urls.length);

                        // Rapid window switch with a held late 14d response:
                        // select 14d (held), then 60d immediately — the view
                        // must settle on 60d (60/60 = 1.0) and the late 14d
                        // response (60/14 = 4.3) must never publish.
                        holdWindows.add(14);
                        await page.getByRole('button', { name: '14d', exact: true }).click();
                        await page.getByRole('button', { name: '60d', exact: true }).click();
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('1.0');
                        const heldWindow = await controls.releaseHeld();
                        expect(heldWindow.length).toBeGreaterThan(0);
                        for (const entry of heldWindow) {
                            const params = new URL(entry.url).searchParams;
                            expect(params.get('project')).toBe(basePrefix);
                            expect(params.get('limit')).toBe('400');
                            expect(Math.abs(deriveWindowDays(entry.url) - 14)).toBeLessThanOrEqual(1);
                        }
                        await feeds.settle(feeds.urls.length);
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('1.0');

                        // Rapid project switch with a held late BASE response:
                        // Reload starts a BASE refresh (held), then FRESH is
                        // selected while it is in flight. The view must settle
                        // on FRESH/60d (30/60 = 0.5) and stay there when the
                        // late BASE response (60/60 = 1.0) finally lands.
                        holdProjects.add(basePrefix);
                        await page.locator('button[title="Refresh insights"]').click();
                        await expect.poll(() => controls!.held.length, { timeout: 10_000 }).toBeGreaterThan(0);
                        const projectSelect = page.locator('.insights-controls select.ui-select');
                        await expect
                            .poll(() => projectSelect.locator(`option[value="${freshPrefix}"]`).count())
                            .toBeGreaterThan(0);
                        await projectSelect.selectOption(freshPrefix);
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('0.5');
                        const heldProject = await controls.releaseHeld();
                        expect(heldProject.length).toBeGreaterThan(0);
                        for (const entry of heldProject) {
                            const params = new URL(entry.url).searchParams;
                            expect(params.get('project')).toBe(basePrefix);
                            expect(params.get('limit')).toBe('400');
                            expect(Math.abs(deriveWindowDays(entry.url) - 60)).toBeLessThanOrEqual(1);
                            expect(params.get('since')).toContain(expectedSinceDay(60));
                        }
                        await feeds.settle(feeds.urls.length);
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('0.5');
                        expect(await projectSelect.inputValue()).toBe(freshPrefix);
                        await expect
                            .poll(async () =>
                                ((await page.locator('.chart-controls button.active').textContent()) ?? '').trim(),
                            )
                            .toBe('60d');

                        // Every feed and task request stayed project-scoped.
                        expect(feeds.countWith('limit', '400')).toBe(feeds.urls.length);
                        for (const url of feeds.urls) {
                            expect([basePrefix, freshPrefix]).toContain(new URL(url).searchParams.get('project'));
                        }
                        for (const url of lists.urls) {
                            expect([basePrefix, freshPrefix]).toContain(new URL(url).searchParams.get('project'));
                        }
                    },
                );
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('Reload after a REST mutation updates the Projects overview totals, not just task-derived tiles (actual backend)', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const baseTask = await seedTask(workspace, { project: 'BASE', title: 'Dev66 reload base one', status: 'Todo' });
            await seedTask(workspace, { project: 'BASE', title: 'Dev66 reload base two', status: 'Todo' });
            const freshSeed = await seedTask(workspace, { project: 'FRESH', title: 'Dev66 reload fresh one', status: 'Todo' });
            const basePrefix = baseTask.project;
            const freshPrefix = freshSeed.project;

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(`${server.url}/insights`, async (page) => {
                    const lists = trackRequests(page, '/api/tasks/list');
                    const stats = trackRequests(page, '/api/projects/stats');

                    // All-projects overview fills from per-prefix stats.
                    await expect.poll(() => tileValue(page, 'Total tasks')).toBe('3');
                    await stats.settle(2);
                    await expect.poll(() => overviewActive(page, basePrefix)).toBe('2');
                    await expect.poll(() => overviewActive(page, freshPrefix)).toBe('1');
                    expect(await distributionRow(page, 'Status distribution', 'Done').count()).toBe(0);
                    const statsAfterMount = stats.urls.length;
                    const baseStatsAfterMount = stats.countWith('project', basePrefix);

                    // REST mutation (explicit SSE, no watcher dependence): the
                    // task-derived views converge automatically — no manual
                    // refresh — while the cached stats overview must NOT move.
                    let seen = lists.urls.length;
                    await apiMutate(server, '/api/tasks/update', { id: baseTask.id, status: 'Done' });
                    await lists.waitForNextRequest(seen);
                    await expect.poll(() => distributionCount(page, 'Status distribution', 'Done')).toBe('1');
                    await expect.poll(() => tileValue(page, 'Total tasks')).toBe('3');
                    await expect.poll(() => overviewActive(page, basePrefix)).toBe('2');
                    expect(stats.urls.length).toBe(statsAfterMount);

                    // Reload (intentional manual refresh) forces a per-prefix
                    // stats refetch: the overview Active totals update, beyond
                    // the task-derived tiles.
                    await page.locator('button[title="Refresh insights"]').click();
                    await stats.waitForNextRequest(statsAfterMount);
                    expect(stats.countWith('project', basePrefix)).toBeGreaterThan(baseStatsAfterMount);
                    await expect.poll(() => overviewActive(page, basePrefix)).toBe('1');
                    await expect.poll(() => overviewActive(page, freshPrefix)).toBe('1');
                    await expect.poll(() => tileValue(page, 'Total tasks')).toBe('3');
                    await expect.poll(() => distributionCount(page, 'Status distribution', 'Done')).toBe('1');
                    for (const url of stats.urls) {
                        expect([basePrefix, freshPrefix]).toContain(new URL(url).searchParams.get('project'));
                    }
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('Insights requests stay explicitly project-scoped with cross-project data isolation (actual backend)', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            await seedTask(workspace, { project: 'BASE', title: 'Dev66 scope base todo', tags: ['alpha'], status: 'Todo' });
            const freshSeed = await seedTask(workspace, { project: 'FRESH', title: 'Dev66 scope fresh done', status: 'Done' });
            const freshPrefix = freshSeed.project;

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withPage(`${server.url}/insights?project=BASE`, async (page) => {
                    const lists = trackRequests(page, '/api/tasks/list');
                    const feeds = trackRequests(page, '/api/activity/feed');

                    // Only the BASE task is in scope; FRESH's Done task is
                    // invisible in every task-derived surface.
                    await expect.poll(() => tileValue(page, 'Total tasks')).toBe('1');
                    await expect.poll(() => distributionCount(page, 'Status distribution', 'Todo')).toBe('1');
                    expect(await distributionRow(page, 'Status distribution', 'Done').count()).toBe(0);
                    await lists.settle(1);
                    await feeds.settle(1);

                    // Exact request scoping while BASE is selected: every task
                    // query and every activity feed query carried the explicit
                    // project filter (the feed endpoint itself answers 400
                    // without git — the request scope is what is contractual).
                    expect(lists.urls.length).toBeGreaterThan(0);
                    for (const url of lists.urls) {
                        expect(new URL(url).searchParams.get('project')).toBe('BASE');
                    }
                    expect(feeds.countWith('project', 'BASE')).toBe(feeds.urls.length);
                    expect(feeds.countWith('limit', '400')).toBe(feeds.urls.length);
                    for (const url of feeds.urls) {
                        expect(Math.abs(deriveWindowDays(url) - 30)).toBeLessThanOrEqual(1);
                    }
                    // Actual backend, no silent skip: the request happened, the
                    // card stays mounted, and the average stays 0.0 on the
                    // empty (failed) feed while the page keeps working.
                    expect(await page.locator('.activity-card').count()).toBe(1);
                    await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('0.0');

                    // Switching to All projects legitimately broadens scope:
                    // the FRESH task appears through a fresh unscoped query.
                    const projectSelect = page.locator('.insights-controls select.ui-select');
                    await expect
                        .poll(() => projectSelect.locator('option[value=""]').count())
                        .toBeGreaterThan(0);
                    await projectSelect.selectOption('');
                    await expect.poll(() => tileValue(page, 'Total tasks')).toBe('2');
                    await expect.poll(() => distributionCount(page, 'Status distribution', 'Done')).toBe('1');
                    await expect.poll(() => lists.countUnscoped('project')).toBeGreaterThan(0);
                    await expect.poll(() => feeds.countUnscoped('project')).toBeGreaterThan(0);
                    await lists.settle(lists.urls.length);
                    // No query ever asked the server for FRESH alone from here.
                    expect(lists.countWith('project', freshPrefix)).toBe(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('empty tag matches keep the activity average at zero and untagged-only excludes tagged tasks (mocked feed, real task queries)', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const tagged = await seedTask(workspace, { project: 'BASE', title: 'Dev66 tag tagged', tags: ['red'], status: 'Todo' });
            await seedTask(workspace, { project: 'BASE', title: 'Dev66 tag untagged', status: 'Todo' });
            const basePrefix = tagged.project;

            const server = await startLotarServer(workspace, {
                env: { LOTAR_ENABLE_POLL_WATCH: '1' },
            });
            try {
                await withRoutedPage(
                    `${server.url}/insights?project=${basePrefix}`,
                    async (context) => {
                        await installFeedRoute(context, (params) => {
                            if (params.get('project') !== basePrefix) {
                                return { taskId: tagged.id, title: 'Dev66 tag tagged', changes: 0 };
                            }
                            // All fixture activity sits on the TAGGED task.
                            return { taskId: tagged.id, title: 'Dev66 tag tagged', changes: 60 };
                        });
                    },
                    async (page) => {
                        const lists = trackRequests(page, '/api/tasks/list');
                        const tagInput = page.locator('input[placeholder="Filter tags (comma separated)"]');

                        // Baseline: both tasks visible, tagged task's feed
                        // activity counts -> 60/30 = 2.0.
                        await expect.poll(() => tileValue(page, 'Total tasks')).toBe('2');
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('2.0');
                        await lists.settle(1);

                        // Zero tag matches: no visible tasks, so NO feed
                        // activity may be counted even though the feed holds
                        // 60 changes (the old whole-feed bug would show 2.0).
                        await tagInput.fill('nomatch');
                        await expect.poll(() => tileValue(page, 'Total tasks')).toBe('0');
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('0.0');
                        await expect
                            .poll(() => page.getByText('No tasks match the current filters.').count())
                            .toBeGreaterThan(0);

                        // Untagged-only: the tagged task is excluded, so the
                        // average stays at zero despite its feed activity.
                        await tagInput.fill('untagged');
                        await expect.poll(() => tileValue(page, 'Total tasks')).toBe('1');
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('0.0');
                        expect(await page.getByText('No tasks match the current filters.').count()).toBe(0);

                        // Matching tag: the tagged task's activity counts.
                        await tagInput.fill('red');
                        await expect.poll(() => tileValue(page, 'Total tasks')).toBe('1');
                        await expect.poll(() => tileValue(page, 'Activity (avg/day)')).toBe('2.0');
                        expect(await averageCaption(page).textContent()).toContain('Average 2.0 updates / day');

                        // Tag filtering is client-side over the retained
                        // project query: no unscoped or broadened task request.
                        for (const url of lists.urls) {
                            expect(new URL(url).searchParams.get('project')).toBe(basePrefix);
                        }
                    },
                );
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
