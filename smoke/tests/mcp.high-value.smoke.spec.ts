import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { callTool, extractToolPayload, initializeFramedMcp, withFramedMcpClient } from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const BASIC_CONFIG = `default:\n  project: CLI\n  reporter: me@example.com\nissue:\n  states: [Todo, Done]\n  priorities: [Low]\n  types: [Feature]\n`;

async function withSeededWorkspace<T>(
    seedFiles: Record<string, string>,
    body: (workspace: SmokeWorkspace) => Promise<T>,
): Promise<T> {
    const workspace = await SmokeWorkspace.create({ seedFiles });
    try {
        return await body(workspace);
    } finally {
        await workspace.dispose();
    }
}

describe.concurrent('MCP high-value tools', () => {
    it('resolves whoami with explain', async () => {
        await withSeededWorkspace({ '.tasks/config.yml': BASIC_CONFIG }, async (workspace) => {
            await withFramedMcpClient(workspace, async (client) => {
                const init = await initializeFramedMcp(client);
                expect(init.message?.error).toBeUndefined();

                const frame = await callTool(client, 2, 'whoami', { explain: true });
                expect(frame.message?.error).toBeUndefined();
                const payload = extractToolPayload(frame) as any;
                expect(payload.status).toBe('ok');
                expect(payload.user).toBe('me@example.com');
                expect(payload.explain).toBeDefined();
            });
        });
    });

    it('adds and updates task comments via MCP', async () => {
        await withSeededWorkspace({ '.tasks/config.yml': BASIC_CONFIG }, async (workspace) => {
            const created = await workspace.addTask('MCP comment task');
            await withFramedMcpClient(workspace, async (client) => {
                const init = await initializeFramedMcp(client);
                expect(init.message?.error).toBeUndefined();

                const added = await callTool(client, 2, 'task_comment_add', {
                    id: created.id,
                    text: 'First comment',
                });
                expect(added.message?.error).toBeUndefined();

                const updated = await callTool(client, 3, 'task_comment_update', {
                    id: created.id,
                    index: 0,
                    text: 'Updated comment',
                });
                expect(updated.message?.error).toBeUndefined();

                const yaml = parse(await workspace.readTaskYaml(created.id)) as Record<string, any>;
                const comments = Array.isArray(yaml.comments) ? yaml.comments : [];
                expect(comments).toHaveLength(1);
                const first = comments[0] as Record<string, any>;
                expect(first.text).toBe('Updated comment');
            });
        });
    });

    it('returns numeric nextCursor for task_list pagination', async () => {
        await withSeededWorkspace({ '.tasks/config.yml': BASIC_CONFIG }, async (workspace) => {
            await workspace.addTask('MCP list A');
            await workspace.addTask('MCP list B');

            await withFramedMcpClient(workspace, async (client) => {
                const init = await initializeFramedMcp(client);
                expect(init.message?.error).toBeUndefined();

                const page1 = await callTool(client, 2, 'task_list', {
                    limit: 1,
                    cursor: 0,
                });
                expect(page1.message?.error).toBeUndefined();
                const payload1 = extractToolPayload(page1) as any;

                expect(payload1.status).toBe('ok');
                expect(payload1.count).toBe(1);
                expect(payload1.hasMore).toBe(true);
                expect(typeof payload1.nextCursor).toBe('number');
                expect(payload1.nextCursor).toBeGreaterThanOrEqual(1);

                const page2 = await callTool(client, 3, 'task_list', {
                    limit: 1,
                    cursor: payload1.nextCursor,
                });
                expect(page2.message?.error).toBeUndefined();
                const payload2 = extractToolPayload(page2) as any;
                expect(payload2.status).toBe('ok');
                expect(payload2.count).toBe(1);
            });
        });
    });

    it('supports sprint list/create/update + analytics tools', async () => {
        await withSeededWorkspace({ '.tasks/config.yml': BASIC_CONFIG }, async (workspace) => {
            const first = await workspace.addTask('MCP sprint task A');
            const second = await workspace.addTask('MCP sprint task B');

            await withFramedMcpClient(workspace, async (client) => {
                const init = await initializeFramedMcp(client);
                expect(init.message?.error).toBeUndefined();

                const created = await callTool(client, 2, 'sprint_create', {
                    label: 'MCP Sprint',
                    starts_at: '2025-01-01T00:00:00Z',
                    ends_at: '2025-01-08T00:00:00Z',
                });
                expect(created.message?.error).toBeUndefined();
                const createPayload = extractToolPayload(created) as any;
                expect(createPayload.status).toBe('ok');

                const sprintId = createPayload.sprint?.id;
                expect(typeof sprintId).toBe('number');

                const updated = await callTool(client, 3, 'sprint_update', {
                    sprint_id: sprintId,
                    label: 'MCP Sprint Updated',
                });
                expect(updated.message?.error).toBeUndefined();
                const updatePayload = extractToolPayload(updated) as any;
                expect(updatePayload.status).toBe('ok');
                expect(updatePayload.sprint?.label).toBe('MCP Sprint Updated');

                const list = await callTool(client, 4, 'sprint_list', {
                    limit: 1,
                    cursor: 0,
                });
                expect(list.message?.error).toBeUndefined();
                const listPayload = extractToolPayload(list) as any;
                expect(listPayload.status).toBe('ok');
                expect(listPayload.count).toBe(1);
                expect(typeof listPayload.nextCursor === 'number' || listPayload.nextCursor === null).toBe(true);

                const added = await callTool(client, 5, 'sprint_add', {
                    tasks: [first.id, second.id],
                    sprint_id: sprintId,
                });
                expect(added.message?.error).toBeUndefined();

                const bulkUpdated = await callTool(client, 6, 'task_bulk_update', {
                    ids: [first.id],
                    patch: { status: 'Done' },
                });
                expect(bulkUpdated.message?.error).toBeUndefined();

                const summary = await callTool(client, 7, 'sprint_summary', {
                    sprint_id: sprintId,
                });
                expect(summary.message?.error).toBeUndefined();
                const summaryPayload = extractToolPayload(summary) as any;
                expect(summaryPayload.status).toBe('ok');

                const burndown = await callTool(client, 8, 'sprint_burndown', {
                    sprint_id: sprintId,
                });
                expect(burndown.message?.error).toBeUndefined();
                const burndownPayload = extractToolPayload(burndown) as any;
                expect(burndownPayload.status).toBe('ok');
                expect(Array.isArray(burndownPayload.series)).toBe(true);

                const velocity = await callTool(client, 9, 'sprint_velocity', {
                    include_active: true,
                    metric: 'tasks',
                });
                expect(velocity.message?.error).toBeUndefined();
                const velocityPayload = extractToolPayload(velocity) as any;
                expect(velocityPayload.status).toBe('ok');
                expect(Array.isArray(velocityPayload.entries)).toBe(true);
            });
        });
    });
});
