import { readFile, stat } from 'node:fs/promises';
import { describe, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { callTool, extractToolPayload, withMcpClient } from '../helpers/mcp-harness.js';
import { startLotarServer } from '../helpers/server.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

type Surface = 'REST' | 'MCP';
interface Order { status: string; changed: boolean; tasks: string[]; sprint_id?: number }
type Move = (args: Record<string, unknown>) => Promise<{ ok: boolean; order?: Order }>;

function seedFiles(corrupt = false): Record<string, string> {
    const files: Record<string, string> = {
        '.tasks/config.yml': 'default.project: ORDER\ndefault.reporter: Fixture\nauto.set_reporter: false\n',
        '.tasks/ORDER/config.yml': 'project.name: Central ordering\n',
        '.tasks/@sprints/backlog.yml': corrupt ? 'tasks: [broken\n' : stringify({
            tasks: ['ORDER-3', 'ORDER-4', 'ORDER-99', 'ORDER-1'],
            fixture_extension: { preserve: ['nested', 'values'] },
        }),
    };
    for (const sprint of [1, 2]) {
        files[`.tasks/@sprints/${sprint}.yml`] = stringify({
            created: '2026-10-01T00:00:00Z', plan: { label: `Ordered sprint ${sprint}` },
            ...(sprint === 2 ? { actual: { started_at: '2026-10-01T00:00:00Z', closed_at: '2026-10-02T00:00:00Z' } } : {}),
            tasks: sprint === 1 ? ['ORDER-4', 'ORDER-5'] : ['ORDER-6', 'ORDER-7'],
            fixture_extension: { preserve: true },
        });
    }
    for (let index = 1; index <= 7; index++) {
        files[`.tasks/ORDER/${index}.yml`] = '# Task bytes must never change during ordering.\n' + stringify({
            title: `Order sentinel ${index}`, status: 'Todo', priority: 'Medium', type: 'Feature',
            reporter: 'Fixture', created: '2026-10-01T00:00:00Z', modified: '2026-10-01T00:00:00Z',
            sprints: index < 4 ? [] : [index < 6 ? 1 : 2], history: [],
        });
    }
    return files;
}

async function snapshot(workspace: SmokeWorkspace) {
    return Promise.all((await workspace.listTaskFiles()).sort().map(async file => ({
        file, bytes: await readFile(file, 'utf8'), mtimeMs: (await stat(file)).mtimeMs,
    })));
}

async function taskSnapshot(workspace: SmokeWorkspace) {
    return Promise.all(Array.from({ length: 7 }, async (_, index) => {
        const id = `ORDER-${index + 1}`;
        const bytes = await workspace.readTaskYaml(id);
        return { id, bytes, mtimeMs: (await stat(workspace.getTaskFilePath(id))).mtimeMs, history: parse(bytes).history };
    }));
}

async function withSurface(surface: Surface, workspace: SmokeWorkspace, body: (move: Move, read: () => Promise<Order>) => Promise<void>) {
    const server = await startLotarServer(workspace);
    try {
        const read = async (): Promise<Order> => {
            const response = await fetch(`${server.url}/api/sprints/backlog/order`);
            if (!response.ok) throw new Error(`Order read failed: ${response.status} ${await response.text()}`);
            return (await response.json() as { data: Order }).data;
        };
        if (surface === 'MCP') {
            await withMcpClient(workspace, async client => {
                let requestId = 0;
                await body(async args => {
                    const response = await callTool(client, ++requestId, 'sprint_reorder', args);
                    if (response.error || response.result?.isError) return { ok: false };
                    return { ok: true, order: extractToolPayload(response) as Order };
                }, read);
            });
        } else {
            await body(async args => {
                const response = await fetch(`${server.url}/api/sprints/order`, {
                    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(args),
                });
                if (!response.ok) {
                    return { ok: false };
                }
                return { ok: true, order: (await response.json() as { data: Order }).data };
            }, read);
        }
    } finally { await server.stop(); }
}

describe.each<Surface>(['REST', 'MCP'])('%s central task order (DEV-109)', surface => {
    it('moves relative to fresh anchors, preserves metadata and task files, and rejects invalid membership', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'task-order-', seedFiles: seedFiles() });
        try {
            await withSurface(surface, workspace, async (move, read) => {
                const tasksBefore = await taskSnapshot(workspace);
                const filesBeforeRead = await snapshot(workspace);
                expect(await read()).toMatchObject({ status: 'ok', changed: false, tasks: ['ORDER-3', 'ORDER-1', 'ORDER-2'] });
                expect(await snapshot(workspace)).toEqual(filesBeforeRead);

                expect(await move({ task: 'ORDER-2', before: 'ORDER-3' })).toMatchObject({
                    ok: true, order: { changed: true, tasks: ['ORDER-2', 'ORDER-3', 'ORDER-1'] },
                });
                let metadata = parse(await workspace.read('.tasks/@sprints/backlog.yml'));
                expect(metadata.tasks).toEqual(['ORDER-2', 'ORDER-3', 'ORDER-4', 'ORDER-99', 'ORDER-1']);
                expect(metadata.fixture_extension).toEqual({ preserve: ['nested', 'values'] });
                expect(await move({ task: 'ORDER-2', sprint: null, after: 'ORDER-1' })).toMatchObject({
                    ok: true, order: { changed: true, tasks: ['ORDER-3', 'ORDER-1', 'ORDER-2'] },
                });
                metadata = parse(await workspace.read('.tasks/@sprints/backlog.yml'));
                expect(metadata.tasks).toEqual(['ORDER-3', 'ORDER-4', 'ORDER-99', 'ORDER-1', 'ORDER-2']);
                expect(metadata.fixture_extension).toEqual({ preserve: ['nested', 'values'] });

                for (const args of [
                    { task: 'ORDER-2', after: 'ORDER-1' },
                    { task: 'ORDER-2', before: 'ORDER-2' },
                ]) {
                    const before = await snapshot(workspace);
                    expect(await move(args)).toMatchObject({ ok: true, order: { changed: false } });
                    expect(await snapshot(workspace)).toEqual(before);
                }
                for (const args of [
                    { task: 'ORDER-1' },
                    { task: 'ORDER-1', before: 'ORDER-2', after: 'ORDER-3' },
                    { task: 'ORDER-1', before: 'ORDER-99' },
                    { task: 'ORDER-99', after: 'ORDER-1' },
                    { task: 'ORDER-4', before: 'ORDER-1' },
                    { task: 'ORDER-1', before: 'ORDER-4' },
                    { task: '../ORDER-1', before: 'ORDER-2' },
                    { task: 'FOREIGN-1', before: 'ORDER-1' },
                    { task: 'ORDER-4', sprint: 1, before: 'ORDER-1' },
                    { task: 'ORDER-1', sprint: 1, before: 'ORDER-4' },
                    { task: 'ORDER-4', sprint: 0, before: 'ORDER-5' },
                    { task: 'ORDER-6', sprint: 2, after: 'ORDER-7' },
                ]) {
                    const before = await snapshot(workspace);
                    expect((await move(args)).ok).toBe(false);
                    expect(await snapshot(workspace)).toEqual(before);
                }

                const backlogBeforeSprint = await workspace.read('.tasks/@sprints/backlog.yml');
                const closedBefore = await workspace.read('.tasks/@sprints/2.yml');
                expect(await move({ task: 'ORDER-4', sprint: 1, after: 'ORDER-5' })).toMatchObject({
                    ok: true, order: { changed: true, sprint_id: 1, tasks: ['ORDER-5', 'ORDER-4'] },
                });
                const sprint = parse(await workspace.read('.tasks/@sprints/1.yml'));
                expect(sprint.tasks).toEqual(['ORDER-5', 'ORDER-4']);
                expect(sprint.fixture_extension).toEqual({ preserve: true });
                expect(await workspace.read('.tasks/@sprints/backlog.yml')).toBe(backlogBeforeSprint);
                expect(await workspace.read('.tasks/@sprints/2.yml')).toBe(closedBefore);
                const beforeNoop = await snapshot(workspace);
                expect(await move({ task: 'ORDER-4', sprint: 1, after: 'ORDER-5' })).toMatchObject({ ok: true, order: { changed: false } });
                expect(await snapshot(workspace)).toEqual(beforeNoop);
                expect(await move({ task: 'ORDER-6', sprint: 2, after: 'ORDER-7', allow_closed: true })).toMatchObject({
                    ok: true, order: { changed: true, sprint_id: 2, tasks: ['ORDER-7', 'ORDER-6'] },
                });
                expect(await read()).toMatchObject({ changed: false, tasks: ['ORDER-3', 'ORDER-1', 'ORDER-2'] });
                expect(await taskSnapshot(workspace)).toEqual(tasksBefore);
            });
        } finally { await workspace.dispose(); }
    });

    it('rejects corrupt central metadata without rewriting any task or order file', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'corrupt-task-order-', seedFiles: seedFiles(true) });
        try {
            await withSurface(surface, workspace, async move => {
                const before = await snapshot(workspace);
                expect((await move({ task: 'ORDER-1', before: 'ORDER-2' })).ok).toBe(false);
                expect(await snapshot(workspace)).toEqual(before);
            });
        } finally { await workspace.dispose(); }
    });
});
