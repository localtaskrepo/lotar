import { describe, it } from 'vitest';
import { JiraStub } from '../helpers/jira-stub.js';
import { callTool, extractToolPayload, withMcpClient } from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('MCP contract parity (DEV-64)', () => {
    it('keeps pagination metadata on every empty listing', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();
        try {
            await withMcpClient(workspace, async (client) => {
                for (const [tool, rows] of [
                    ['task_list', 'tasks'], ['sprint_list', 'sprints'],
                    ['sprint_backlog', 'tasks'], ['project_list', 'projects'],
                ] as const) {
                    const response = await callTool(client, tool, tool, { limit: 3, cursor: 7 });
                    expect(response.error).toBeUndefined();
                    expect(response.result.isError).not.toBe(true);
                    const payload = extractToolPayload(response) as Record<string, unknown>;
                    expect(payload).toMatchObject({
                        count: 0, total: 0, cursor: 0, limit: 3, hasMore: false, nextCursor: null,
                    });
                    expect(payload[rows]).toEqual([]);
                }
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('refreshes project hints and validates backlog status against them', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ seedFiles: {
            '.tasks/config.yml': 'default.project: MCP\n',
            '.tasks/MCP/config.yml': 'issue.states: [Ready, Closed]\nissue.priorities: [Urgent]\n',
            '.tasks/MCP/1.yml': 'title: Project task\nstatus: Ready\npriority: Urgent\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n',
        } });
        try {
            await withMcpClient(workspace, async (client) => {
                const first = await callTool(client, 'backlog', 'sprint_backlog', {
                    project: 'MCP', status: 'ready', limit: 1,
                });
                expect(first.error).toBeUndefined();
                const payload = extractToolPayload(first) as Record<string, any>;
                expect(payload).toMatchObject({ count: 1, limit: 1 });
                expect(payload.tasks[0].id).toBe('MCP-1');
                expect(payload.enumHints.statuses).toEqual(['Ready', 'Closed']);

                const invalid = await callTool(client, 'invalid', 'sprint_backlog', {
                    project: 'MCP', status: 'Bogus',
                });
                expect(invalid.error.code).toBe(-32602);
                expect(invalid.error.data.suggestions).toEqual(['Ready', 'Closed']);

                await workspace.write('.tasks/MCP/config.yml',
                    'issue.states: [Ready, Reviewing, Closed]\nissue.priorities: [Immediate]\n');
                const refreshed = await callTool(client, 'refreshed', 'sprint_backlog', { project: 'MCP' });
                const hints = (extractToolPayload(refreshed) as Record<string, any>).enumHints;
                expect(hints.statuses).toEqual(['Ready', 'Reviewing', 'Closed']);
                expect(hints.priorities).toEqual(['Immediate']);
                expect(hints.sprints).toBeUndefined();
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('targets a single task for real pull and dry-run push without changing its sibling', async ({ expect }) => {
        const stub = await JiraStub.start({ projectKey: 'MCP', issues: [
            { key: 'MCP-101', summary: 'First remote task' },
            { key: 'MCP-102', summary: 'Second remote task' },
        ] });
        let workspace: SmokeWorkspace | undefined;
        try {
            workspace = await SmokeWorkspace.create({ seedFiles: {
                '.tasks/config.yml': 'default.project: MCP\nremotes:\n  origin:\n    provider: jira\n    project: MCP\n    auth_profile: fixture\n    mapping:\n      title: summary\n',
                '.lotar': `auth_profiles:\n  fixture:\n    provider: jira\n    method: basic\n    email_env: fixture@example.test\n    token_env: dev64-fake-token\n    api_url: ${stub.url}\n`,
            } });
            const owned = workspace;
            await withMcpClient(owned, async (client) => {
                const initial = await callTool(client, 'seed', 'sync_pull', {
                    remote: 'origin', include_report: true, write_report: false,
                });
                expect(initial.error).toBeUndefined();
                expect(initial.result.isError).not.toBe(true);
                const initialPayload = extractToolPayload(initial) as Record<string, any>;
                expect(initialPayload.summary.created).toBe(2);
                const ids = initialPayload.report_entries.map((entry: { task_id: string }) => entry.task_id);
                expect(new Set(ids).size).toBe(2);
                const siblingBefore = await owned.readTaskYaml(ids[1]);

                for (const tool of ['sync_pull', 'sync_push']) {
                    const response = await callTool(client, tool, tool, {
                        remote: 'origin', task_id: ids[0], dry_run: tool === 'sync_push',
                        include_report: true, write_report: false,
                    });
                    expect(response.error).toBeUndefined();
                    expect(response.result.isError).not.toBe(true);
                    const payload = extractToolPayload(response) as Record<string, any>;
                    expect(payload.report_entries).toHaveLength(1);
                    expect(payload.report_entries[0].task_id).toBe(ids[0]);
                    expect(await owned.readTaskYaml(ids[1])).toBe(siblingBefore);
                }
                const typo = await callTool(client, 'typo', 'sync_push', {
                    remote: 'origin', task_id: ids[0], dryrun: true,
                });
                expect(typo.error.code).toBe(-32602);
            }, { env: { HOME: owned.root, LOTAR_IGNORE_HOME_CONFIG: '0' } });
        } finally {
            await workspace?.dispose();
            await stub.stop();
        }
    });
});
