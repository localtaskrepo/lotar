import { stat } from 'node:fs/promises';
import type { Locator } from 'playwright';
import { describe, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const PAGE_SIZE = 25;
const BACKLOG_IDS = Array.from({ length: 27 }, (_, index) => `ORDER-${index + 1}`);
const INITIAL_ORDER = ['OTHER-1', 'ORDER-27', ...BACKLOG_IDS.slice(0, 26)];

function seedFiles(): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': 'default.project: ORDER\ndefault.reporter: Fixture\nauto.set_reporter: false\n',
        '.tasks/ORDER/config.yml': 'project.name: Ordered backlog\n',
        '.tasks/OTHER/config.yml': 'project.name: Hidden project\n',
        '.tasks/@sprints/backlog.yml': stringify({ tasks: INITIAL_ORDER, fixture_extension: { preserve: true } }),
        '.tasks/@sprints/1.yml': stringify({
            created: '2026-10-01T00:00:00Z',
            plan: { label: 'Reorder existing sprint rows' },
            tasks: ['ORDER-28', 'ORDER-29', 'ORDER-30'],
        }),
    };
    for (let index = 1; index <= 30; index++) {
        files[`.tasks/ORDER/${index}.yml`] =
            '# Ordering must not rewrite task files.\n' +
            stringify({
                title: `Order task ${String(index).padStart(2, '0')}`,
                status: 'Todo',
                priority: 'Medium',
                type: 'Feature',
                reporter: 'Fixture',
                created: '2026-10-01T00:00:00Z',
                modified: '2026-10-01T00:00:00Z',
                sprints: index > 27 ? [1] : [],
                history: [],
            });
    }
    files['.tasks/OTHER/1.yml'] = stringify({
        title: 'Other project task',
        status: 'Todo',
        priority: 'Medium',
        type: 'Feature',
        reporter: 'Fixture',
        created: '2026-10-01T00:00:00Z',
        modified: '2026-10-01T00:00:00Z',
        sprints: [],
        history: [],
    });
    return files;
}

async function taskSnapshot(workspace: SmokeWorkspace) {
    return Promise.all(
        [...Array.from({ length: 30 }, (_, index) => `ORDER-${index + 1}`), 'OTHER-1'].map(async (id) => {
            const bytes = await workspace.readTaskYaml(id);
            return {
                id,
                bytes,
                mtimeMs: (await stat(workspace.getTaskFilePath(id))).mtimeMs,
                history: parse(bytes).history,
            };
        }),
    );
}

function moved(order: string[], task: string, anchor: string, position: 'before' | 'after') {
    const result = order.filter((id) => id !== task);
    result.splice(result.indexOf(anchor) + Number(position === 'after'), 0, task);
    return result;
}

describe.concurrent('UI durable central task order (DEV-109)', () => {
    for (const viewport of [
        { width: 1440, height: 900 },
        { width: 390, height: 844 },
    ]) {
        it(`drags before/after in backlog and sprint without changing task files at ${viewport.width}px`, async ({
            expect,
        }) => {
            const workspace = await SmokeWorkspace.create({ name: 'ui-task-order-', seedFiles: seedFiles() });
            try {
                const tasksBefore = await taskSnapshot(workspace);
                const server = await startLotarServer(workspace);
                try {
                    await withPage('about:blank', async (page) => {
                        await page.setViewportSize(viewport);
                        await page.addInitScript(() => {
                            if (localStorage.getItem('task-order-fixture') === 'ready') return;
                            localStorage.setItem('task-order-fixture', 'ready');
                            localStorage.setItem('lotar.preferences.tasks.pageSize', '25');
                            localStorage.setItem('lotar.sprints.window.v2', 'all');
                            localStorage.setItem('lotar.sprints.sort', JSON.stringify({ key: 'title', dir: 'asc' }));
                        });
                        await page.goto(`${server.url}/sprints`);
                        const backlog = page.locator('[data-sprint-id="backlog"]');
                        const sprint = page.locator('[data-sprint-id="1"]');
                        const ids = (group: Locator) =>
                            group
                                .locator('tr.task-row')
                                .evaluateAll((rows) => rows.map((row) => row.getAttribute('data-task-id')));
                        const manual = page.getByTestId('backlog-manual-order');
                        const storedManual = () =>
                            page.evaluate(() => localStorage.getItem('lotar.sprints.backlogManualOrder'));
                        const orderFile = async () => parse(await workspace.read('.tasks/@sprints/backlog.yml'));
                        const membershipRequests: string[] = [];
                        const postRequests: string[] = [];
                        page.on('request', (request) => {
                            const pathname = new URL(request.url()).pathname;
                            if (request.method() === 'POST') postRequests.push(pathname);
                            if (
                                request.method() === 'POST' &&
                                /\/api\/(tasks\/(update|status)|sprints\/(add|remove))$/.test(pathname)
                            ) {
                                membershipRequests.push(pathname);
                            }
                        });
                        const blockedDrag = async (group: Locator, task: string, anchor: string) => {
                            const before = await ids(group);
                            const backlogYaml = await workspace.read('.tasks/@sprints/backlog.yml');
                            const sprintYaml = await workspace.read('.tasks/@sprints/1.yml');
                            const postsBefore = [...postRequests];
                            const source = group.locator(`tr[data-task-id="${task}"]`);
                            const target = group.locator(`tr[data-task-id="${anchor}"]`);
                            const message = 'Automatic sorting is on. Switch to Manual order to reorder tasks.';
                            await expect.poll(() => source.getAttribute('draggable')).toBe('true');
                            expect(await source.getAttribute('title')).toBe(message);
                            await source.dragTo(target, {
                                sourcePosition: { x: 25, y: 8 },
                                targetPosition: { x: 25, y: 2 },
                            });
                            const tooltip = page.locator('body > #task-order-tooltip[role="tooltip"]');
                            await tooltip.waitFor({ state: 'visible' });
                            expect(await tooltip.textContent()).toBe(message);
                            await expect.poll(() => ids(group)).toEqual(before);
                            expect(
                                await group
                                    .getByRole('button', { name: 'Manual order', exact: true })
                                    .getAttribute('aria-pressed'),
                            ).toBe('false');
                            expect(await workspace.read('.tasks/@sprints/backlog.yml')).toBe(backlogYaml);
                            expect(await workspace.read('.tasks/@sprints/1.yml')).toBe(sprintYaml);
                            expect(await taskSnapshot(workspace)).toEqual(tasksBefore);
                            expect(postRequests).toEqual(postsBefore);
                        };
                        const drag = async (
                            group: Locator,
                            task: string,
                            anchor: string,
                            position: 'before' | 'after',
                            sprintId: number | null,
                        ) => {
                            const source = group.locator(`tr[data-task-id="${task}"]`);
                            const target = group.locator(`tr[data-task-id="${anchor}"]`);
                            await expect.poll(() => source.getAttribute('draggable')).toBe('true');
                            await source.scrollIntoViewIfNeeded();
                            await target.scrollIntoViewIfNeeded();
                            const bounds = await target.boundingBox();
                            expect(bounds).not.toBeNull();
                            const response = page.waitForResponse((candidate) => {
                                if (new URL(candidate.url()).pathname !== '/api/sprints/order') return false;
                                const body = candidate.request().postDataJSON();
                                return (
                                    body.task === task &&
                                    body[position] === anchor &&
                                    (body.sprint ?? null) === sprintId
                                );
                            });
                            // Left-edge coordinates keep the drag on-screen on a 390px-wide table.
                            await source.dragTo(target, {
                                sourcePosition: { x: 25, y: 8 },
                                targetPosition: { x: 25, y: position === 'before' ? 2 : bounds!.height - 2 },
                            });
                            const saved = await response;
                            expect(saved.ok()).toBe(true);
                            expect((await saved.json()).data).toMatchObject({ changed: true });
                        };

                        await expect.poll(() => ids(backlog)).toEqual(BACKLOG_IDS.slice(0, PAGE_SIZE));
                        expect(await manual.getAttribute('aria-pressed')).toBe('false');
                        await blockedDrag(backlog, 'ORDER-3', 'ORDER-1');
                        await manual.click();
                        await page.locator('#task-order-tooltip').waitFor({ state: 'detached' });
                        await expect.poll(() => ids(backlog)).toEqual(INITIAL_ORDER.slice(0, PAGE_SIZE));
                        await backlog.locator('tr[data-task-id="ORDER-1"]').click();
                        const panel = page.locator('.task-panel');
                        await panel.waitFor({ state: 'visible' });
                        expect(await panel.getByRole('spinbutton', { name: 'Backlog rank', exact: true }).count()).toBe(
                            0,
                        );
                        await panel.getByRole('button', { name: 'Close panel', exact: true }).click();
                        await panel.waitFor({ state: 'detached' });
                        expect(await backlog.getByText(/^Rank \d+$/).count()).toBe(0);
                        const sprintBeforeBacklog = await workspace.read('.tasks/@sprints/1.yml');
                        await drag(backlog, 'ORDER-3', 'ORDER-1', 'before', null);
                        let order = moved(INITIAL_ORDER, 'ORDER-3', 'ORDER-1', 'before');
                        await expect.poll(() => ids(backlog)).toEqual(order.slice(0, PAGE_SIZE));
                        await expect.poll(storedManual).toBe('true');
                        expect((await orderFile()).tasks).toEqual(order);
                        expect((await orderFile()).fixture_extension).toEqual({ preserve: true });
                        expect(await workspace.read('.tasks/@sprints/1.yml')).toBe(sprintBeforeBacklog);
                        expect(await taskSnapshot(workspace)).toEqual(tasksBefore);

                        await drag(backlog, 'ORDER-3', 'ORDER-2', 'after', null);
                        order = moved(order, 'ORDER-3', 'ORDER-2', 'after');
                        await expect.poll(() => ids(backlog)).toEqual(order.slice(0, PAGE_SIZE));
                        await page.reload({ waitUntil: 'domcontentloaded' });
                        await expect.poll(() => ids(backlog)).toEqual(order.slice(0, PAGE_SIZE));
                        expect(await manual.getAttribute('aria-pressed')).toBe('true');
                        expect((await orderFile()).tasks).toEqual(order);

                        await page.goto(`${server.url}/sprints?project=ORDER`);
                        await expect
                            .poll(() => ids(backlog))
                            .toEqual(order.filter((id) => id.startsWith('ORDER-')).slice(0, PAGE_SIZE));
                        // Moving visible rows must not replace the global order with this filtered page.
                        await drag(backlog, 'ORDER-2', 'ORDER-4', 'after', null);
                        order = moved(order, 'ORDER-2', 'ORDER-4', 'after');
                        expect((await orderFile()).tasks).toEqual(order);
                        expect(order.filter((id) => id !== 'ORDER-2')).toEqual(
                            INITIAL_ORDER.filter((id) => id !== 'ORDER-2'),
                        );
                        const projectOrder = order.filter((id) => id.startsWith('ORDER-'));
                        await expect.poll(() => ids(backlog)).toEqual(projectOrder.slice(0, PAGE_SIZE));
                        await backlog.getByRole('button', { name: 'Page 2', exact: true }).click();
                        await expect.poll(() => ids(backlog)).toEqual(projectOrder.slice(PAGE_SIZE));
                        await page.reload({ waitUntil: 'domcontentloaded' });
                        await expect.poll(() => ids(backlog)).toEqual(projectOrder.slice(0, PAGE_SIZE));
                        expect(await backlog.locator('[data-task-id="ORDER-28"]').count()).toBe(0);

                        const titleHeader = backlog.locator('th[data-column="title"]');
                        expect(
                            await page.evaluate(() => JSON.parse(localStorage.getItem('lotar.sprints.sort')!)),
                        ).toEqual({ key: 'title', dir: 'asc' });
                        await titleHeader.locator('button.header-button').click();
                        await expect.poll(storedManual).toBe('false');
                        await expect.poll(() => titleHeader.getAttribute('aria-sort')).toBe('descending');
                        expect((await orderFile()).tasks).toEqual(order);
                        const automaticIds = await ids(backlog);
                        await blockedDrag(backlog, automaticIds[1]!, automaticIds[0]!);
                        await manual.click();
                        await expect.poll(() => ids(backlog)).toEqual(projectOrder.slice(0, PAGE_SIZE));

                        const backlogBeforeSprint = await workspace.read('.tasks/@sprints/backlog.yml');
                        await expect.poll(() => ids(sprint)).toEqual(['ORDER-30', 'ORDER-29', 'ORDER-28']);
                        await blockedDrag(sprint, 'ORDER-29', 'ORDER-28');
                        await sprint.getByTestId('sprint-manual-order').click();
                        await page.locator('#task-order-tooltip').waitFor({ state: 'detached' });
                        await expect.poll(() => ids(sprint)).toEqual(['ORDER-28', 'ORDER-29', 'ORDER-30']);
                        await drag(sprint, 'ORDER-29', 'ORDER-28', 'before', 1);
                        await expect.poll(() => ids(sprint)).toEqual(['ORDER-29', 'ORDER-28', 'ORDER-30']);
                        expect(await sprint.getByTestId('sprint-manual-order').getAttribute('aria-pressed')).toBe(
                            'true',
                        );
                        expect(parse(await workspace.read('.tasks/@sprints/1.yml')).tasks).toEqual([
                            'ORDER-29',
                            'ORDER-28',
                            'ORDER-30',
                        ]);
                        await drag(sprint, 'ORDER-29', 'ORDER-30', 'after', 1);
                        await expect.poll(() => ids(sprint)).toEqual(['ORDER-28', 'ORDER-30', 'ORDER-29']);
                        await page.reload({ waitUntil: 'domcontentloaded' });
                        await expect.poll(() => ids(sprint)).toEqual(['ORDER-28', 'ORDER-30', 'ORDER-29']);
                        expect(parse(await workspace.read('.tasks/@sprints/1.yml')).tasks).toEqual([
                            'ORDER-28',
                            'ORDER-30',
                            'ORDER-29',
                        ]);
                        expect(await workspace.read('.tasks/@sprints/backlog.yml')).toBe(backlogBeforeSprint);
                        expect(await taskSnapshot(workspace)).toEqual(tasksBefore);
                        expect(membershipRequests).toEqual([]);
                    });
                } finally {
                    await server.stop();
                }
            } finally {
                await workspace.dispose();
            }
        });
    }
});
