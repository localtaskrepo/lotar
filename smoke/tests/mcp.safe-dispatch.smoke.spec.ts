import fs from 'fs-extra';
import { parse } from 'yaml';
import { describe, expect, it } from 'vitest';
import { deadLoopbackPort } from '../helpers/jira-stub.js';
import {
    callTool,
    expectProtocolError,
    extractToolPayload,
    nextMcpRequestId,
    withMcpClient,
} from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const BASIC_CONFIG = `default:\n  project: CLI\n  reporter: me@example.com\n  assignee: me@example.com\nissue:\n  states: [Todo, Done]\n  priorities: [Low]\n  types: [Feature]\n`;

const CONTROL_PLANE_METHODS = ['initialize', 'tools/list', 'notifications/initialized', 'ping', 'logging/setLevel'];

async function reportEntries(workspace: SmokeWorkspace): Promise<string[]> {
    const entries = await fs.readdir(workspace.tasksDir);
    return entries.filter((entry) => entry.startsWith('@reports'));
}

describe.concurrent('MCP safe tool dispatch', () => {
    it('tools/call cannot invoke control-plane methods', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            await withMcpClient(workspace, async (client) => {
                for (const [index, method] of CONTROL_PLANE_METHODS.entries()) {
                    const response = await callTool(client, 100 + index, method, {});
                    expectProtocolError(response, -32602, 'Unknown tool');
                    expect(response.result).toBeUndefined();
                }

                // The failed control-plane attempts left the session ready and
                // unchanged for normal traffic.
                const pingId = nextMcpRequestId('ping');
                await client.send({ jsonrpc: '2.0', id: pingId, method: 'ping' });
                expect((await client.awaitResponse(pingId)).error).toBeUndefined();

                const listId = nextMcpRequestId('list');
                await client.send({ jsonrpc: '2.0', id: listId, method: 'tools/list' });
                const list = await client.awaitResponse(listId);
                expect(Array.isArray(list.result?.tools)).toBe(true);
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('rejects unknown dryrun on sync tools without triggering a live sync', async () => {
        const deadPort = await deadLoopbackPort();
        const homeConfig = [
            'auth_profiles:',
            '  jira.default:',
            '    provider: jira',
            '    method: basic',
            '    email_env: mcp-safe-dispatch@example.test',
            '    token_env: mcp-safe-dispatch-fake-token',
            `    api_url: http://127.0.0.1:${deadPort}`,
            '',
        ].join('\n');
        const projectConfig = `default:\n  project: CLI\n  reporter: me@example.com\nissue:\n  states: [Todo, Done]\n  priorities: [Low]\n  types: [Feature]\nremotes:\n  dead-jira:\n    provider: jira\n    project: CLI\n    auth_profile: jira.default\n`;

        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': projectConfig,
                '.lotar/config.yml': homeConfig,
            },
        });

        try {
            const task = await workspace.addTask('Sync safety sentinel task');
            const taskYamlBefore = await workspace.readTaskYaml(task.id);

            await withMcpClient(
                workspace,
                async (client) => {
                    const pull = await callTool(client, 1, 'sync_pull', { remote: 'dead-jira', dryrun: true });
                    expectProtocolError(pull, -32602, 'Invalid params', 'dryrun');

                    const push = await callTool(client, 2, 'sync_push', { remote: 'dead-jira', dryrun: true });
                    expectProtocolError(push, -32602, 'Invalid params', 'dryrun');
                },
                { env: { LOTAR_IGNORE_HOME_CONFIG: '0' } },
            );

            // No live sync may have started: no report artifacts exist and the
            // task file is byte-identical.
            expect(await reportEntries(workspace)).toEqual([]);
            expect(await workspace.readTaskYaml(task.id)).toBe(taskYamlBefore);
        } finally {
            await workspace.dispose();
        }
    });

    it('direct method aliases enforce the same input checks as tools/call', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            await withMcpClient(workspace, async (client) => {
                const viaWrapper = await callTool(client, 1, 'task_create', {});
                expectProtocolError(viaWrapper, -32602, 'Invalid params');

                const directId = nextMcpRequestId('direct');
                await client.send({
                    jsonrpc: '2.0',
                    id: directId,
                    method: 'task_create',
                    params: {},
                });
                const direct = await client.awaitResponse(directId);
                expectProtocolError(direct, -32602, 'Invalid params');
                expect(direct.error?.code).toBe(viaWrapper.error?.code);
                expect(direct.error?.message).toBe(viaWrapper.error?.message);

                const agentViaWrapper = await callTool(client, 2, 'agent_status', {});
                expectProtocolError(agentViaWrapper, -32602, 'Invalid params');

                const agentDirectId = nextMcpRequestId('direct-agent');
                await client.send({
                    jsonrpc: '2.0',
                    id: agentDirectId,
                    method: 'agent_status',
                    params: {},
                });
                const agentDirect = await client.awaitResponse(agentDirectId);
                expectProtocolError(agentDirect, -32602, 'Invalid params');
                expect(agentDirect.error?.code).toBe(agentViaWrapper.error?.code);
                expect(agentDirect.error?.message).toBe(agentViaWrapper.error?.message);
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps valid status updates and null clears working', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const task = await workspace.addTask('Status and null-clear semantics');

            await withMcpClient(workspace, async (client) => {
                const updated = await callTool(client, 1, 'task_update', {
                    id: task.id,
                    patch: { status: 'Done' },
                });
                expect(updated.error).toBeUndefined();
                const payload = extractToolPayload(updated) as Record<string, any>;
                const dto = payload.task ?? payload;
                expect(dto.status).toBe('Done');

                const cleared = await callTool(client, 2, 'task_update', {
                    id: task.id,
                    patch: { assignee: null },
                });
                expect(cleared.error).toBeUndefined();

                const statusNoOp = await callTool(client, 3, 'task_update', {
                    id: task.id,
                    patch: { status: null },
                });
                expect(statusNoOp.error).toBeUndefined();
            });

            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            expect(yaml.status).toBe('Done');
            expect(yaml.assignee === null || yaml.assignee === undefined || yaml.assignee === '').toBe(true);
        } finally {
            await workspace.dispose();
        }
    });
});
