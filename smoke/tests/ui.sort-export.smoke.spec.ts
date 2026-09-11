import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

/**
 * DEV-57 browser coverage for server-authoritative sorting and export
 * parity. The backend ships the unified query executor, so `sort_by` support
 * is asserted up front (non-skippable): a regression must fail here, not
 * silently degrade to a skip.
 */

interface ListPayload {
    data: { tasks: Array<{ id: string }>; total: number };
}

/**
 * Asserts the served backend honors `sort_by` queries over the seeded
 * fixtures. Fails the test with a targeted message when the executor
 * regresses instead of skipping.
 */
async function expectBackendSupportsSortBy(serverUrl: string, fixtureIds: string[]): Promise<void> {
    const response = await fetch(`${serverUrl}/api/tasks/list?sort_by=modified&order=desc&limit=1`);
    const payload = response.ok ? ((await response.json()) as ListPayload) : undefined;
    const served = payload?.data?.tasks?.[0]?.id;
    expect(
        response.ok
            && payload?.data?.total === fixtureIds.length
            && fixtureIds.includes(served ?? ''),
        `backend must serve the seeded fixtures through a sort_by query (status=${response.status}, total=${payload?.data?.total ?? 'n/a'}, first=${served ?? 'n/a'})`,
    ).toBe(true);
}

async function rewriteTask(
    workspace: SmokeWorkspace,
    filePath: string,
    mutate: (payload: Record<string, unknown>) => void,
): Promise<void> {
    const payload = parse(await fs.readFile(filePath, 'utf8')) as Record<string, unknown>;
    mutate(payload);
    await fs.writeFile(filePath, stringify(payload));
}

async function rowNumericIds(page: import('@playwright/test').Page): Promise<string[]> {
    return page.locator('tbody tr td:first-child strong').allTextContents();
}

describe('UI sort and export smoke scenarios (DEV-57)', () => {
    it('orders every page of a global custom-field sort, not just the slice', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const seeded = [] as Array<{ id: string; filePath: string }>;
            for (let i = 1; i <= 30; i += 1) {
                const created = await workspace.addTask(`Ranked task ${String(i).padStart(2, '0')}`);
                await rewriteTask(workspace, created.filePath, (payload) => {
                    payload.custom_fields = { Rank: String(i).padStart(2, '0') };
                });
                seeded.push(created);
            }

            const server = await startLotarServer(workspace);
            try {
                await expectBackendSupportsSortBy(server.url, seeded.map((task) => task.id));

                await withBrowser({}, async (browserContext) => {
                    await browserContext.addInitScript(() => {
                        window.localStorage.setItem('lotar.preferences.tasks.pageSize', '25');
                    });
                    const page = await browserContext.newPage();
                    await page.goto(`${server.url}/?sort_by=custom:Rank&order=asc`, {
                        waitUntil: 'domcontentloaded',
                    });

                    await page.waitForSelector('tbody tr td:first-child strong', { timeout: 20_000 });
                    await page.waitForFunction(
                        () => document.querySelectorAll('tbody tr').length === 25,
                        undefined,
                        { timeout: 20_000 },
                    );
                    const pageOne = await rowNumericIds(page);
                    expect(pageOne).toEqual(Array.from({ length: 25 }, (_, i) => String(i + 1)));

                    await page.click('button[aria-label="Next page"]');
                    await page.waitForFunction(
                        () => document.querySelectorAll('tbody tr').length === 5,
                        undefined,
                        { timeout: 20_000 },
                    );
                    const pageTwo = await rowNumericIds(page);
                    expect(pageTwo).toEqual(['26', '27', '28', '29', '30']);
                    await page.close();
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('restores the saved sort per project without cross-project leakage', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const frontendDir = path.join('apps', 'frontend');
            const apiDir = path.join('services', 'api');
            await workspace.write(
                path.join(frontendDir, 'package.json'),
                JSON.stringify({ name: 'frontend-app', private: true }, null, 2),
            );
            await workspace.write(
                path.join(apiDir, 'Cargo.toml'),
                ['[package]', 'name = "api-service"', 'version = "0.1.0"'].join('\n'),
            );

            // Lexical priority order in `asc` is high < low < medium.
            const frontendHigh = await workspace.addTask('Frontend high priority', {
                cwd: path.join(workspace.root, frontendDir),
                args: ['--priority', 'high'],
            });
            const frontendLow = await workspace.addTask('Frontend low priority', {
                cwd: path.join(workspace.root, frontendDir),
                args: ['--priority', 'low'],
            });
            const frontendMedium = await workspace.addTask('Frontend medium priority', {
                cwd: path.join(workspace.root, frontendDir),
                args: ['--priority', 'medium'],
            });

            const server = await startLotarServer(workspace);
            try {
                const unrelated = await workspace.addTask('API unrelated task', {
                    cwd: path.join(workspace.root, apiDir),
                });
                await expectBackendSupportsSortBy(server.url, [
                    frontendHigh.id,
                    frontendLow.id,
                    frontendMedium.id,
                    unrelated.id,
                ]);

                await withBrowser({}, async (browserContext) => {
                    const page = await browserContext.newPage();
                    await page.goto(server.url, { waitUntil: 'domcontentloaded' });
                    await page.waitForSelector('text=Frontend high priority', { timeout: 20_000 });

                    // Open the filters panel and scope to the frontend project.
                    await page.click('[data-testid="filter-toggle"]');
                    const projectSelect = page.locator('[data-testid="filter-project"]');
                    await projectSelect.waitFor({ state: 'visible', timeout: 10_000 });
                    await projectSelect.selectOption(frontendHigh.project);
                    await page.waitForFunction(
                        (excluded: string) => !document.body.innerText.includes(excluded),
                        'API unrelated task',
                        { timeout: 20_000 },
                    );

                    // Sort the frontend project by priority (asc) via the header.
                    await page.click('th:has-text("Priority") button.header-button');
                    await page.waitForFunction(
                        () => window.location.search.includes('sort_by=priority'),
                        undefined,
                        { timeout: 20_000 },
                    );
                    const expectedFirst = String(Number(frontendHigh.id.split('-').pop()));
                    await page.waitForFunction(
                        (first: string) => {
                            const cell = document.querySelector('tbody tr td:first-child strong')
                            return cell?.textContent === first
                        },
                        expectedFirst,
                        { timeout: 20_000 },
                    );
                    const sortedIds = await rowNumericIds(page);
                    expect(sortedIds).toEqual([
                        String(Number(frontendHigh.id.split('-').pop())),
                        String(Number(frontendLow.id.split('-').pop())),
                        String(Number(frontendMedium.id.split('-').pop())),
                    ]);
                    const savedSort = await page.evaluate(
                        (project: string) => window.localStorage.getItem(`lotar.tasks.sort::${project}`),
                        frontendHigh.project,
                    );
                    expect(savedSort).toContain('"priority"');

                    // Switching projects drops the sort to that project's default.
                    await projectSelect.selectOption('');
                    await page.waitForSelector('text=API unrelated task', { timeout: 20_000 });
                    const sortKeys = await page.evaluate(() =>
                        Object.keys(window.localStorage).filter((key) => key.startsWith('lotar.tasks.sort')),
                    );
                    expect(sortKeys).toEqual([`lotar.tasks.sort::${frontendHigh.project}`]);

                    // Switching back restores the saved sort.
                    await projectSelect.selectOption(frontendHigh.project);
                    await page.waitForFunction(
                        () => window.location.search.includes('sort_by=priority'),
                        undefined,
                        { timeout: 20_000 },
                    );
                    await page.waitForFunction(
                        (excluded: string) => !document.body.innerText.includes(excluded),
                        'API unrelated task',
                        { timeout: 20_000 },
                    );
                    const restoredIds = await rowNumericIds(page);
                    expect(restoredIds).toEqual(sortedIds);
                    await page.close();
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps CSV export aligned with the filtered list under the full filter grammar', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const today = new Date();
            const isoDate = [
                today.getFullYear(),
                String(today.getMonth() + 1).padStart(2, '0'),
                String(today.getDate()).padStart(2, '0'),
            ].join('-');

            const rankA = await workspace.addTask('Export parity ranked A', {
                args: ['--tag', 'export', '--assignee', 'parity@example.com'],
            });
            const rankB = await workspace.addTask('Export parity ranked B', {
                args: ['--tag', 'export'],
            });
            const noDue = await workspace.addTask('Export parity needs due', {
                args: ['--tag', 'export'],
            });
            await rewriteTask(workspace, rankA.filePath, (payload) => {
                payload.custom_fields = { Rank: '01' };
                payload.due_date = isoDate;
            });
            await rewriteTask(workspace, rankB.filePath, (payload) => {
                payload.custom_fields = { Rank: '02' };
                payload.due_date = isoDate;
            });

            const server = await startLotarServer(workspace);
            try {
                // needs=due keeps only tasks without a due date.
                const dueOnly = await fetch(`${server.url}/api/tasks/list?tags=export&needs=due`);
                expect(dueOnly.ok).toBe(true);
                const duePayload = (await dueOnly.json()) as ListPayload;
                expect(duePayload.data.tasks.map((task) => task.id)).toEqual([noDue.id]);

                // due=today keeps the two dated tasks; assignee narrows to one.
                const todayList = await fetch(`${server.url}/api/tasks/list?tags=export&due=today&order=asc`);
                expect(todayList.ok).toBe(true);
                const todayPayload = (await todayList.json()) as ListPayload;
                expect(todayPayload.data.tasks.map((task) => task.id)).toEqual([rankA.id, rankB.id]);

                const assigneeList = await fetch(
                    `${server.url}/api/tasks/list?tags=export&assignee=parity%40example.com`,
                );
                expect(assigneeList.ok).toBe(true);
                const assigneePayload = (await assigneeList.json()) as ListPayload;
                expect(assigneePayload.data.tasks.map((task) => task.id)).toEqual([rankA.id]);

                // Export mirrors the list exactly for the same query, including
                // the smart filters and the requested global order.
                const parityQuery = 'tags=export&due=today&order=asc';
                const listResponse = await fetch(`${server.url}/api/tasks/list?${parityQuery}`);
                expect(listResponse.ok).toBe(true);
                const listPayload = (await listResponse.json()) as ListPayload;
                const exportResponse = await fetch(`${server.url}/api/tasks/export?${parityQuery}`);
                expect(exportResponse.ok).toBe(true);
                const csvIds = (await exportResponse.text())
                    .trim()
                    .split('\n')
                    .slice(1)
                    .map((line) => line.split(',')[0]!.replace(/"/g, ''));
                expect(csvIds).toEqual(listPayload.data.tasks.map((task) => task.id));
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('downloads the CSV for the current display query and matches every page', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const seeded = [] as Array<{ id: string; filePath: string }>;
            for (let i = 1; i <= 30; i += 1) {
                const created = await workspace.addTask(`Download parity task ${String(i).padStart(2, '0')}`);
                await rewriteTask(workspace, created.filePath, (payload) => {
                    payload.custom_fields = { Rank: String(i).padStart(2, '0') };
                });
                seeded.push(created);
            }

            const server = await startLotarServer(workspace);
            try {
                await expectBackendSupportsSortBy(server.url, seeded.map((task) => task.id));

                await withBrowser({}, async (browserContext) => {
                    await browserContext.addInitScript(() => {
                        window.localStorage.setItem('lotar.preferences.tasks.pageSize', '25');
                    });
                    const page = await browserContext.newPage();
                    await page.goto(`${server.url}/?sort_by=custom:Rank&order=asc`, {
                        waitUntil: 'domcontentloaded',
                    });

                    await page.waitForSelector('tbody tr td:first-child strong', { timeout: 20_000 });
                    await page.waitForFunction(
                        () => document.querySelectorAll('tbody tr').length === 25,
                        undefined,
                        { timeout: 20_000 },
                    );
                    const pageOneIds = await rowNumericIds(page);

                    await page.click('button[aria-label="Next page"]');
                    await page.waitForFunction(
                        () => document.querySelectorAll('tbody tr').length === 5,
                        undefined,
                        { timeout: 20_000 },
                    );
                    const pageTwoIds = await rowNumericIds(page);

                    // The Export button must be enabled for the settled query.
                    const exportButton = page.locator('button[aria-label="Export CSV"]');
                    expect(await exportButton.isEnabled()).toBe(true);

                    // Capture the CSV body via route interception: the
                    // single-process sandbox headless shell accepts the
                    // download event but cancels blob persistence and detaches
                    // in-flight response bodies, so the wire bytes are read at
                    // the protocol level while the UI click path stays real.
                    const exportBodies: string[] = [];
                    await browserContext.route('**/api/tasks/export*', async (route) => {
                        const response = await route.fetch()
                        exportBodies.push(await response.text())
                        await route.fulfill({ response })
                    });

                    const [download] = await Promise.all([
                        page.waitForEvent('download', { timeout: 20_000 }),
                        exportButton.click(),
                    ]);
                    expect(download.suggestedFilename()).toMatch(/^lotar-tasks-\d{4}-\d{2}-\d{2}\.csv$/);
                    for (let i = 0; i < 50 && exportBodies.length === 0; i += 1) {
                        await page.waitForTimeout(100)
                    }
                    const csv = exportBodies[0] ?? '';
                    expect(csv).toContain('id,title')
                    const csvIds = csv
                        .trim()
                        .split('\n')
                        .slice(1)
                        .map((line) => line.split(',')[0]!.replace(/"/g, ''));
                    // CSV covers BOTH pages in the same global order.
                    const projectPrefix = seeded[0]!.id.slice(0, seeded[0]!.id.lastIndexOf('-'));
                    expect(csvIds).toEqual([
                        ...pageOneIds.map((n) => `${projectPrefix}-${Number(n)}`),
                        ...pageTwoIds.map((n) => `${projectPrefix}-${Number(n)}`),
                    ]);
                    await page.close();
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('orders mixed-format due dates identically for list and export', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const a = await workspace.addTask('Due date only', { args: ['--tag', 'mixdue'] });
            const b = await workspace.addTask('Due zulu instant', { args: ['--tag', 'mixdue'] });
            const c = await workspace.addTask('Due offset instant', { args: ['--tag', 'mixdue'] });
            const d = await workspace.addTask('Due naive datetime', { args: ['--tag', 'mixdue'] });
            await rewriteTask(workspace, a.filePath, (payload) => {
                payload.due_date = '2026-03-05';
            });
            await rewriteTask(workspace, b.filePath, (payload) => {
                payload.due_date = '2026-03-05T12:00:00Z';
            });
            await rewriteTask(workspace, c.filePath, (payload) => {
                payload.due_date = '2026-03-05T20:00:00+05:00';
            });
            await rewriteTask(workspace, d.filePath, (payload) => {
                payload.due_date = '2026-03-06 09:30:00';
            });

            const server = await startLotarServer(workspace);
            try {
                await expectBackendSupportsSortBy(server.url, [a.id, b.id, c.id, d.id]);

                // Same instant written two ways (b and c) must tie-break by
                // canonical ID ascending, and every format must order
                // consistently between the list and the export.
                const query = 'tags=mixdue&sort_by=due&order=asc';
                const listResponse = await fetch(`${server.url}/api/tasks/list?${query}`);
                expect(listResponse.ok).toBe(true);
                const listPayload = (await listResponse.json()) as ListPayload;
                const listIds = listPayload.data.tasks.map((task) => task.id);
                expect(listIds.length).toBe(4);
                const zulu = listIds.indexOf(b.id);
                const offset = listIds.indexOf(c.id);
                expect(zulu).toBeGreaterThanOrEqual(0);
                expect(Math.abs(zulu - offset)).toBe(1);

                const exportResponse = await fetch(`${server.url}/api/tasks/export?${query}`);
                expect(exportResponse.ok).toBe(true);
                const csvIds = (await exportResponse.text())
                    .trim()
                    .split('\n')
                    .slice(1)
                    .map((line) => line.split(',')[0]!.replace(/"/g, ''));
                expect(csvIds).toEqual(listIds);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('matches the custom-field global order between list and export', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const rankA = await workspace.addTask('Export order A', { args: ['--tag', 'order'] });
            const rankB = await workspace.addTask('Export order B', { args: ['--tag', 'order'] });
            const noRank = await workspace.addTask('Export order unranked', { args: ['--tag', 'order'] });
            await rewriteTask(workspace, rankA.filePath, (payload) => {
                payload.custom_fields = { Rank: '01' };
            });
            await rewriteTask(workspace, rankB.filePath, (payload) => {
                payload.custom_fields = { Rank: '02' };
            });

            const server = await startLotarServer(workspace);
            try {
                await expectBackendSupportsSortBy(server.url, [rankA.id, rankB.id, noRank.id]);

                // Missing custom values sort first in asc (Option ordering),
                // then the ranked tasks by value.
                const sortedQuery = 'tags=order&sort_by=custom:Rank&order=asc';
                const sortedList = await fetch(`${server.url}/api/tasks/list?${sortedQuery}`);
                expect(sortedList.ok).toBe(true);
                const sortedPayload = (await sortedList.json()) as ListPayload;
                expect(sortedPayload.data.tasks.map((task) => task.id)).toEqual([noRank.id, rankA.id, rankB.id]);

                const sortedExport = await fetch(`${server.url}/api/tasks/export?${sortedQuery}`);
                expect(sortedExport.ok).toBe(true);
                const sortedCsvIds = (await sortedExport.text())
                    .trim()
                    .split('\n')
                    .slice(1)
                    .map((line) => line.split(',')[0]!.replace(/"/g, ''));
                expect(sortedCsvIds).toEqual([noRank.id, rankA.id, rankB.id]);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
