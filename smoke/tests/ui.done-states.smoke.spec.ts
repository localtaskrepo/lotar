import fs from 'fs-extra';
import { parse, stringify } from 'yaml';
import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace, type CreatedTask } from '../helpers/workspace.js';

interface TaskState {
    done_states: string[];
    is_done: boolean;
    due_bucket: 'today' | 'soon' | 'later' | 'overdue' | null;
    calendar_day: string;
}

interface TaskSnapshot {
    id: string;
    title: string;
    status: string;
    task_state?: TaskState;
}

interface TaskPage {
    total: number;
    tasks?: TaskSnapshot[];
}

interface PolicySnapshot {
    effective_done_states: string[];
    done_states_mode: 'explicit' | 'inferred';
    task_calendar_day: string;
}

async function api<T>(url: string, path: string, body?: Record<string, unknown>): Promise<T> {
    const response = await fetch(`${url}${path}`, {
        method: body ? 'POST' : 'GET',
        headers: { Accept: 'application/json', ...(body ? { 'Content-Type': 'application/json' } : {}) },
        body: body ? JSON.stringify(body) : undefined,
    });
    const text = await response.text();
    if (!response.ok) throw new Error(`${path}: ${response.status} ${text}`);
    return (JSON.parse(text) as { data: T }).data;
}

async function withDoneStatesFixture(
    callback: (fixture: {
        workspace: SmokeWorkspace;
        url: string;
        tasks: Record<string, CreatedTask>;
        policy: PolicySnapshot;
    }) => Promise<void>,
) {
    const workspace = await SmokeWorkspace.create({
        name: 'dev21-done-states-',
        seedFiles: {
            '.tasks/config.yml': stringify({
                default: { project: 'ALPHA' },
                issue: { states: ['Todo', 'InProgress', 'Done', 'Closed', 'Shipped'] },
            }),
            '.tasks/ALPHA/config.yml': stringify({
                project: { name: 'Alpha' }, issue: { done_states: ['Closed', 'Shipped'] },
            }),
            '.tasks/BETA/config.yml': stringify({
                project: { name: 'Beta' }, issue: { done_states: ['Done'] },
            }),
        },
    });
    try {
        const tasks: Record<string, CreatedTask> = {};
        for (const [key, project, status, title] of [
            ['closed', 'ALPHA', 'Closed', 'Alpha closed finished'],
            ['shipped', 'ALPHA', 'Shipped', 'Alpha shipped finished'],
            ['done', 'ALPHA', 'Done', 'Alpha Done still open'],
            ['open', 'ALPHA', 'Todo', 'Alpha open overdue'],
            ['today', 'ALPHA', 'Todo', 'Alpha due today'],
            ['betaDone', 'BETA', 'Done', 'Beta Done finished'],
            ['betaClosed', 'BETA', 'Closed', 'Beta Closed still open'],
        ] as const) {
            const task = await workspace.addTask(title, { args: [`--project=${project}`] });
            const data = parse(await fs.readFile(task.filePath, 'utf8')) as Record<string, unknown>;
            data.status = status;
            data.due_date = '2000-01-01';
            await fs.writeFile(task.filePath, stringify(data));
            tasks[key] = task;
        }
        const server = await startLotarServer(workspace);
        try {
            const policy = await api<PolicySnapshot>(server.url, '/api/config/show?project=ALPHA');
            expect(policy.effective_done_states).toEqual(['Closed', 'Shipped']);
            expect(policy.done_states_mode).toBe('explicit');
            expect(policy.task_calendar_day).toMatch(/^\d{4}-\d{2}-\d{2}$/);
            await api(server.url, '/api/tasks/update', {
                id: tasks.today.id, due_date: policy.task_calendar_day,
            });
            await callback({ workspace, url: server.url, tasks, policy });
        } finally {
            await server.stop();
        }
    } finally {
        await workspace.dispose();
    }
}

describe.concurrent('DEV-21 done-state policy (actual backend)', () => {
    it('classifies mixed-project tasks before pagination without persisting computed metadata', async ({ expect }) => {
        await withDoneStatesFixture(async ({ url, tasks, policy }) => {
            const result = await api<TaskPage>(url, '/api/tasks/list?limit=200');
            const byId = new Map((result.tasks ?? []).map(task => [task.id, task]));
            expect(result.total).toBe(7);
            for (const [key, isDone, bucket] of [
                ['closed', true, null], ['shipped', true, null],
                ['done', false, 'overdue'], ['open', false, 'overdue'],
                ['today', false, 'today'], ['betaDone', true, null],
                ['betaClosed', false, 'overdue'],
            ] as const) {
                expect(byId.get(tasks[key].id)?.task_state).toEqual({
                    done_states: tasks[key].project === 'BETA' ? ['Done'] : ['Closed', 'Shipped'],
                    is_done: isDone, due_bucket: bucket, calendar_day: policy.task_calendar_day,
                });
            }
            const overdue = await api<TaskPage>(url, '/api/tasks/list?due=overdue&limit=1&sort_by=id&order=asc');
            expect(overdue.total).toBe(3);
            expect(overdue.tasks).toHaveLength(1);
            expect(overdue.tasks?.[0].id).toBe(tasks.done.id);
            const alpha = await api<{ open_count: number; done_count: number }>(url, '/api/projects/stats?project=ALPHA');
            const beta = await api<{ open_count: number; done_count: number }>(url, '/api/projects/stats?project=BETA');
            expect(alpha).toMatchObject({ open_count: 3, done_count: 2 });
            expect(beta).toMatchObject({ open_count: 1, done_count: 1 });
            for (const task of Object.values(tasks)) {
                const stored = parse(await fs.readFile(task.filePath, 'utf8')) as Record<string, unknown>;
                expect(stored).not.toHaveProperty('task_state');
            }
        });
    });

    it('uses explicit mid-workflow terminal states for table overdue styling', async ({ expect }) => {
        await withDoneStatesFixture(async ({ url }) => {
            await withPage(`${url}/?project=ALPHA`, async (page) => {
                const row = (title: string) => page.locator('table tbody tr').filter({ hasText: title });
                await expect.poll(() => row('Alpha Done still open').count()).toBe(1);
                const due = (title: string) => row(title).locator('.task-table__cell--due_date .overdue');
                await expect.poll(() => due('Alpha Done still open').count()).toBe(1);
                expect(await due('Alpha closed finished').count()).toBe(0);
                expect(await due('Alpha shipped finished').count()).toBe(0);
                expect(await due('Alpha due today').count()).toBe(0);
            });
        });
    });

    it('counts the same status label independently in a mixed-project sprint', async ({ expect }) => {
        await withDoneStatesFixture(async ({ url, tasks }) => {
            const created = await api<{ sprint: { id: number } }>(url, '/api/sprints/create', {
                label: 'Mixed project done-state policies',
            });
            expect(created.sprint.id).toBe(1);
            await api(url, '/api/sprints/add', {
                sprint: created.sprint.id, tasks: [tasks.done.id, tasks.betaDone.id],
            });
            const summary = await api<{
                metrics: { tasks: { committed: number; done: number; remaining: number; completion_ratio: number } };
            }>(url, `/api/sprints/summary?sprint=${created.sprint.id}`);
            expect(summary.metrics.tasks).toEqual({
                committed: 2, done: 1, remaining: 1, completion_ratio: 0.5,
            });
        });
    });

    it('does not borrow a homonymous project policy from another task root', async ({ expect }) => {
        const states = ['Todo', 'InProgress', 'Done', 'Shipped'];
        const workspace = await SmokeWorkspace.create({
            name: 'dev21-homonymous-roots-',
            seedFiles: {
                '.tasks/config.yml': stringify({
                    default: { project: 'DUP' }, issue: { states, done_states: ['Shipped'] },
                }),
                'package/.tasks/config.yml': stringify({
                    default: { project: 'DUP' }, issue: { states, done_states: ['Done'] },
                }),
            },
        });
        try {
            const primary = await workspace.addTask('Primary Done is still open', { args: ['--project=DUP'] });
            const data = parse(await fs.readFile(primary.filePath, 'utf8')) as Record<string, unknown>;
            data.status = 'Done';
            data.due_date = '2000-01-01';
            await fs.writeFile(primary.filePath, stringify(data));
            const secondaryFile = `${workspace.root}/package/.tasks/DUP/2.yml`;
            await fs.ensureFile(secondaryFile);
            await fs.writeFile(secondaryFile, stringify({ ...data, title: 'Secondary Done is finished' }));
            const server = await startLotarServer(workspace);
            try {
                const result = await api<TaskPage>(server.url, '/api/tasks/list?project=DUP&limit=200');
                expect(result.total).toBe(2);
                const byId = new Map((result.tasks ?? []).map(task => [task.id, task]));
                expect(byId.get('DUP-1')?.task_state).toMatchObject({
                    done_states: ['Shipped'], is_done: false, due_bucket: 'overdue',
                });
                expect(byId.get('DUP-2')?.task_state).toMatchObject({
                    done_states: ['Done'], is_done: true, due_bucket: null,
                });
                const overdue = await api<TaskPage>(server.url, '/api/tasks/list?project=DUP&due=overdue&limit=200');
                expect(overdue.tasks?.map(task => task.id)).toEqual(['DUP-1']);
                await withPage(`${server.url}/?project=DUP`, async (page) => {
                    const row = (title: string) => page.locator('table tbody tr').filter({ hasText: title });
                    await expect.poll(() => row('Secondary Done is finished').count()).toBe(1);
                    await expect.poll(() => row('Primary Done is still open').locator('.overdue').count()).toBe(1);
                    expect(await row('Secondary Done is finished').locator('.overdue').count()).toBe(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps Insights overdue totals consistent with the task-query drilldown', async ({ expect }) => {
        await withDoneStatesFixture(async ({ url }) => {
            const overdue = await api<TaskPage>(url, '/api/tasks/list?project=ALPHA&due=overdue&limit=200');
            expect(overdue.total).toBe(2);
            await withPage(`${url}/insights?project=ALPHA`, async (page) => {
                const tile = page.locator('.summary-tile').filter({ hasText: 'Overdue' });
                await expect.poll(async () => (await tile.locator('.summary-value').textContent())?.trim()).toBe('2');
                const drilldown = page.locator('xpath=//h3[normalize-space()="Due date outlook"]/following::table[1]//tbody/tr[th="Overdue"]');
                expect((await drilldown.locator('td').first().textContent())?.trim()).toBe('2');
                await drilldown.click();
                await page.waitForURL(next => next.pathname === '/' && next.searchParams.get('due') === 'overdue');
                const rows = page.locator('table tbody tr');
                await expect.poll(() => rows.count()).toBe(2);
                expect(await rows.allTextContents()).toEqual(expect.arrayContaining([
                    expect.stringContaining('Alpha Done still open'),
                    expect.stringContaining('Alpha open overdue'),
                ]));
            });
        });
    });

    it('refreshes filtered membership and policy after a live configuration change', async ({ expect }) => {
        await withDoneStatesFixture(async ({ url, tasks, workspace }) => {
            const taskBytes = new Map(await Promise.all(Object.values(tasks).map(async task => [
                task.filePath, await fs.readFile(task.filePath, 'utf8'),
            ] as const)));
            await withPage(`${url}/?project=ALPHA&due=overdue`, async (page) => {
                const row = (title: string) => page.locator('table tbody tr').filter({ hasText: title });
                await expect.poll(() => row('Alpha Done still open').count()).toBe(1);
                await api(url, '/api/config/set', {
                    project: 'ALPHA', global: false,
                    values: { 'issue.done_states': 'Closed,Shipped,Done' },
                });
                // No page reload: config_updated must invalidate policy and query membership.
                await expect.poll(() => row('Alpha Done still open').count()).toBe(0);
                await expect.poll(() => row('Alpha open overdue').count()).toBe(1);
                const policy = await api<PolicySnapshot>(url, '/api/config/show?project=ALPHA');
                expect(policy.effective_done_states).toEqual(['Closed', 'Shipped', 'Done']);
                const overdue = await api<TaskPage>(url, '/api/tasks/list?project=ALPHA&due=overdue&limit=200');
                expect(overdue.tasks?.map(task => task.id)).toEqual([tasks.open.id]);
            });
            for (const [file, before] of taskBytes) expect(await fs.readFile(file, 'utf8')).toBe(before);
            expect(await workspace.listTaskFiles()).toHaveLength(7);
        });
    });

    it('uses project completion policy for Board badges without persisting visibility defaults', async ({ expect }) => {
        await withDoneStatesFixture(async ({ url }) => {
            await withPage(`${url}/boards?project=ALPHA`, async (page) => {
                const card = (title: string) => page.locator('article.card.task').filter({ hasText: title });
                await expect.poll(() => card('Alpha Done still open').count()).toBe(1);
                await expect.poll(() => card('Alpha Done still open').locator('.is-overdue').count()).toBe(1);
                expect(await card('Alpha closed finished').locator('.is-overdue').count()).toBe(0);
                expect(await card('Alpha shipped finished').locator('.is-overdue').count()).toBe(0);
                expect(await card('Alpha due today').locator('.is-overdue').count()).toBe(0);
                expect(await card('Alpha closed finished').count()).toBe(1);
                expect(await card('Alpha shipped finished').count()).toBe(1);
                expect(await page.evaluate(() => localStorage.getItem('lotar.doneFilters::ALPHA'))).toBeNull();
            });
        });
    });

    it('edits inheritance and preserves an explicit choice equal to the automatic global policy', async ({ expect }) => {
        await withDoneStatesFixture(async ({ url }) => {
            await withPage(`${url}/config`, async (page) => {
                const scope = page.getByLabel('Project scope');
                await scope.selectOption('BETA');
                const mode = page.getByLabel('Done states mode');
                await expect.poll(() => mode.inputValue()).toBe('explicit');
                const save = page.getByRole('button', { name: 'Save changes', exact: true });
                await mode.selectOption('automatic');
                const cleared = page.waitForResponse(response =>
                    new URL(response.url()).pathname === '/api/config/set' && response.request().method() === 'POST');
                await save.click();
                expect((await cleared).ok()).toBe(true);
                await expect.poll(async () => {
                    const inspect = await api<{
                        project_raw: { issue_done_states?: string[] | null };
                        effective: PolicySnapshot;
                    }>(url, '/api/config/inspect?project=BETA');
                    return inspect.project_raw.issue_done_states == null && inspect.effective.done_states_mode === 'inferred';
                }).toBe(true);
                const global = await api<PolicySnapshot>(url, '/api/config/show');
                await expect.poll(() => mode.inputValue()).toBe('automatic');
                await mode.selectOption('explicit');
                const pinned = page.waitForResponse(response =>
                    new URL(response.url()).pathname === '/api/config/set' && response.request().method() === 'POST');
                await save.click();
                expect((await pinned).ok()).toBe(true);
                await expect.poll(async () => {
                    const inspect = await api<{
                        project_raw: { issue_done_states?: string[] | null };
                        effective: PolicySnapshot;
                        sources: Record<string, string>;
                    }>(url, '/api/config/inspect?project=BETA');
                    return {
                        raw: inspect.project_raw.issue_done_states,
                        effective: inspect.effective.effective_done_states,
                        mode: inspect.effective.done_states_mode,
                        source: inspect.sources.issue_done_states,
                    };
                }).toEqual({
                    raw: global.effective_done_states,
                    effective: global.effective_done_states,
                    mode: 'explicit',
                    source: 'project',
                });
            });
        });
    });
});
