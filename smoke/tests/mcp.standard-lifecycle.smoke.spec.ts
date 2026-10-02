import fs from 'fs-extra';
import { describe, expect, it } from 'vitest';
import {
    MCP_CLIENT_INFO,
    expectLifecycleRejection,
    extractToolPayload,
    initializeMcp,
    nextMcpRequestId,
    spawnMcpClient,
    withMcpClient,
} from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const BASIC_CONFIG = `default:\n  project: CLI\n  reporter: me@example.com\nissue:\n  states: [Todo, Done]\n  priorities: [Low]\n  types: [Feature]\n`;

const CONFIG_WITH_EXTRA_STATUS = `default:\n  project: CLI\n  reporter: me@example.com\nissue:\n  states: [Todo, Doing, Done]\n  priorities: [Low]\n  types: [Feature]\n`;

async function workspaceState(workspace: SmokeWorkspace): Promise<{ config: string; taskFiles: string[]; reportEntries: string[] }> {
    const entries = await fs.readdir(workspace.tasksDir);
    return {
        config: await workspace.read('.tasks/config.yml'),
        taskFiles: await workspace.listTaskFiles(),
        reportEntries: entries.filter((entry) => entry.startsWith('@reports')),
    };
}

async function sendRequest(
    client: Awaited<ReturnType<typeof spawnMcpClient>>,
    method: string,
    params: Record<string, unknown>,
): Promise<any> {
    const id = nextMcpRequestId('req');
    await client.send({ jsonrpc: '2.0', id, method, params });
    return await client.awaitResponse(id);
}

async function sendRawLine(client: Awaited<ReturnType<typeof spawnMcpClient>>, line: string): Promise<void> {
    await new Promise<void>((resolve, reject) => {
        client.childProcess.stdin!.write(`${line}\n`, 'utf8', (error) => (error ? reject(error) : resolve()));
    });
}

describe.concurrent('MCP 2025-06-18 standard stdio lifecycle', () => {
    it('answers ping before initialize but rejects operational requests', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnMcpClient(workspace);
            try {
                const pingId = nextMcpRequestId('pre-ping');
                await client.send({ jsonrpc: '2.0', id: pingId, method: 'ping' });
                const pong = await client.awaitResponse(pingId);
                expect(pong.error).toBeUndefined();
                expect(pong.result).toEqual({});

                const listId = nextMcpRequestId('pre-list');
                await client.send({ jsonrpc: '2.0', id: listId, method: 'tools/list' });
                expectLifecycleRejection(await client.awaitResponse(listId));
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('rejects mutating requests before initialize and before the initialized notice, with no mutations', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const before = await workspaceState(workspace);

            const unInitialized = await spawnMcpClient(workspace);
            try {
                const taskCreate = await sendRequest(unInitialized, 'tools/call', {
                    name: 'task_create',
                    arguments: { title: 'Must not exist' },
                });
                expectLifecycleRejection(taskCreate);

                const configSet = await sendRequest(unInitialized, 'tools/call', {
                    name: 'config_set',
                    arguments: { values: { 'default.reporter': 'attacker@example.com' } },
                });
                expectLifecycleRejection(configSet);

                const syncPush = await sendRequest(unInitialized, 'tools/call', {
                    name: 'sync_push',
                    arguments: { remote: 'nonexistent' },
                });
                expectLifecycleRejection(syncPush);
            } finally {
                await unInitialized.dispose();
            }

            expect(await workspaceState(workspace)).toEqual(before);

            const partiallyInitialized = await spawnMcpClient(workspace);
            try {
                const initId = nextMcpRequestId('init');
                await partiallyInitialized.send({
                    jsonrpc: '2.0',
                    id: initId,
                    method: 'initialize',
                    params: {
                        protocolVersion: '2025-06-18',
                        capabilities: {},
                        clientInfo: MCP_CLIENT_INFO,
                    },
                });
                const initResponse = await partiallyInitialized.awaitResponse(initId);
                expect(initResponse.error).toBeUndefined();

                // initialize alone must not be enough: the initialized notice
                // is still missing, so operational requests stay rejected.
                const taskCreate = await sendRequest(partiallyInitialized, 'tools/call', {
                    name: 'task_create',
                    arguments: { title: 'Must not exist either' },
                });
                expectLifecycleRejection(taskCreate);

                const toolsList = await sendRequest(partiallyInitialized, 'tools/list', {});
                expectLifecycleRejection(toolsList);
            } finally {
                await partiallyInitialized.dispose();
            }

            expect(await workspaceState(workspace)).toEqual(before);
        } finally {
            await workspace.dispose();
        }
    });

    it('an out-of-order initialized notice cannot make the session ready', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnMcpClient(workspace);
            try {
                await client.send({ jsonrpc: '2.0', method: 'notifications/initialized' });

                const initId = nextMcpRequestId('init');
                await client.send({
                    jsonrpc: '2.0',
                    id: initId,
                    method: 'initialize',
                    params: {
                        protocolVersion: '2025-06-18',
                        capabilities: {},
                        clientInfo: MCP_CLIENT_INFO,
                    },
                });
                const initResponse = await client.awaitResponse(initId);
                expect(initResponse.error).toBeUndefined();

                // The notice arrived before initialize, so the session must
                // still refuse operational requests.
                const toolsList = await sendRequest(client, 'tools/list', {});
                expectLifecycleRejection(toolsList);

                // A properly ordered notice makes the session ready.
                await client.send({ jsonrpc: '2.0', method: 'notifications/initialized' });
                const pingId = nextMcpRequestId('ping');
                await client.send({ jsonrpc: '2.0', id: pingId, method: 'ping' });
                expect((await client.awaitResponse(pingId)).error).toBeUndefined();

                const readyList = await sendRequest(client, 'tools/list', {});
                expect(Array.isArray(readyList.result?.tools)).toBe(true);
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('never responds to notifications', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            await withMcpClient(workspace, async (client) => {
                await client.send({ jsonrpc: '2.0', method: 'notifications/initialized' });
                await client.send({
                    jsonrpc: '2.0',
                    method: 'notifications/cancelled',
                    params: { requestId: 999 },
                });
                await client.send({
                    jsonrpc: '2.0',
                    method: 'notifications/progress',
                    params: { progressToken: 'token', progress: 1 },
                });

                // The ping response is the ordering barrier: the server
                // processes input sequentially, so by the time it replies to
                // the ping any bogus notification response would be queued.
                const pingId = nextMcpRequestId('ping');
                await client.send({ jsonrpc: '2.0', id: pingId, method: 'ping' });
                const pong = await client.awaitResponse(pingId);
                expect(pong.error).toBeUndefined();

                const bogusResponses = client.pendingMessages.filter(
                    (message) =>
                        message.method === undefined &&
                        (message.id === undefined || message.id === null) &&
                        (message.result !== undefined || message.error !== undefined),
                );
                expect(bogusResponses).toEqual([]);
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('never executes tool-shaped notifications', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnMcpClient(workspace);
            try {
                await initializeMcp(client);
                const before = await workspaceState(workspace);

                // Valid arguments, deliberately without an id: if the server
                // treated these as requests it would create tasks and persist
                // them, which the snapshot below must refute.
                await client.send({
                    jsonrpc: '2.0',
                    method: 'task/create',
                    params: { title: 'Notification must not create this task' },
                });
                await client.send({
                    jsonrpc: '2.0',
                    method: 'tools/call',
                    params: { name: 'task_create', arguments: { title: 'Notification must not create this tool task' } },
                });
                await client.send({
                    jsonrpc: '2.0',
                    method: 'task_create',
                    params: { title: 'Alias notification must not create this task' },
                });

                // Ordering barrier: once the ping response arrives the server
                // has processed every notification sent above.
                const pingId = nextMcpRequestId('ping');
                await client.send({ jsonrpc: '2.0', id: pingId, method: 'ping' });
                const pong = await client.awaitResponse(pingId);
                expect(pong.error).toBeUndefined();

                const bogusResponses = client.pendingMessages.filter(
                    (message) =>
                        message.method === undefined &&
                        (message.id === undefined || message.id === null) &&
                        (message.result !== undefined || message.error !== undefined),
                );
                expect(bogusResponses).toEqual([]);

                expect(await workspaceState(workspace)).toEqual(before);
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('emits notifications/tools/list_changed when idle and defers delivery until ready', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnMcpClient(workspace);
            try {
                const initId = nextMcpRequestId('init');
                await client.send({
                    jsonrpc: '2.0',
                    id: initId,
                    method: 'initialize',
                    params: {
                        protocolVersion: '2025-06-18',
                        capabilities: {},
                        clientInfo: MCP_CLIENT_INFO,
                    },
                });
                expect((await client.awaitResponse(initId)).error).toBeUndefined();

                // The server is fully booted now. Change the config while the
                // session still awaits the initialized notice, then hold past
                // one 2-second watcher poll cycle so the change is detected
                // pre-ready.
                await workspace.write('.tasks/config.yml', CONFIG_WITH_EXTRA_STATUS);
                await new Promise((resolve) => setTimeout(resolve, 3_000));

                // Ping barrier proves watcher silence in the pre-ready
                // window: a notification emitted before this response would
                // already sit in the queue.
                const preReadyPingId = nextMcpRequestId('ping');
                await client.send({ jsonrpc: '2.0', id: preReadyPingId, method: 'ping' });
                expect((await client.awaitResponse(preReadyPingId)).error).toBeUndefined();
                expect(
                    client.pendingMessages.filter(
                        (message) => message.method === 'notifications/tools/list_changed',
                    ),
                ).toEqual([]);

                await client.send({ jsonrpc: '2.0', method: 'notifications/initialized' });
                const readyPingId = nextMcpRequestId('ping');
                await client.send({ jsonrpc: '2.0', id: readyPingId, method: 'ping' });
                expect((await client.awaitResponse(readyPingId)).error).toBeUndefined();

                // The pre-ready change must not be lost: it is delivered with
                // the standard method name once the session is ready.
                const notification = await client.readUntil(
                    (message) => message.method === 'notifications/tools/list_changed',
                    60_000,
                );
                expect(Array.isArray(notification.params?.hintCategories)).toBe(true);

                // While idle (no requests in flight) another change must be
                // announced with the same standard method name.
                await workspace.write('.tasks/config.yml', BASIC_CONFIG);
                const idleNotification = await client.readUntil(
                    (message) =>
                        message.method === 'notifications/tools/list_changed' && message !== notification,
                    60_000,
                );
                expect(Array.isArray(idleNotification.params?.hintCategories)).toBe(true);
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('negotiates a supported protocol version for unknown client versions', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            for (const requestedVersion of ['2024-11-05', 'x999-future']) {
                const client = await spawnMcpClient(workspace);
                try {
                    const initId = nextMcpRequestId('init');
                    await client.send({
                        jsonrpc: '2.0',
                        id: initId,
                        method: 'initialize',
                        params: {
                            protocolVersion: requestedVersion,
                            capabilities: {},
                            clientInfo: MCP_CLIENT_INFO,
                        },
                    });
                    const initResponse = await client.awaitResponse(initId);
                    expect(initResponse.error).toBeUndefined();
                    expect(initResponse.result?.protocolVersion).toBe('2025-06-18');

                    await client.send({ jsonrpc: '2.0', method: 'notifications/initialized' });
                    const pingId = nextMcpRequestId('ping');
                    await client.send({ jsonrpc: '2.0', id: pingId, method: 'ping' });
                    expect((await client.awaitResponse(pingId)).error).toBeUndefined();
                } finally {
                    await client.dispose();
                }
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('rejects initialize with incomplete params and malformed request ids', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnMcpClient(workspace);
            try {
                const incompleteId = nextMcpRequestId('init');
                await client.send({
                    jsonrpc: '2.0',
                    id: incompleteId,
                    method: 'initialize',
                    params: { protocolVersion: '2025-06-18' },
                });
                const incomplete = await client.awaitResponse(incompleteId);
                expect(incomplete.error?.code).toBe(-32602);
                expect(incomplete.error?.message).toBe('Invalid params');

                for (const malformedId of [true, { an: 'object' }, [1], 1.5]) {
                    await client.send({
                        jsonrpc: '2.0',
                        id: malformedId,
                        method: 'ping',
                    });
                    const rejection = await client.readUntil(
                        (message) => message?.error?.code === -32600,
                        20_000,
                    );
                    expect(rejection.id).toBeNull();
                }

                // The connection still completes a full handshake afterwards.
                const init = await initializeMcp(client);
                expect(init?.result?.protocolVersion).toBe('2025-06-18');
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('classifies malformed envelopes by JSON validity', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnMcpClient(workspace);
            try {
                // Syntactically invalid text is a parse error.
                await sendRawLine(client, 'this is not json at all');
                const parseError = await client.readUntil(
                    (message) => message?.error?.code === -32700,
                    20_000,
                );
                expect(parseError.id).toBeNull();
                expect(parseError.error?.message).toBe('Parse error');

                // Valid JSON with an invalid request envelope is an invalid
                // request, not a parse error.
                for (const line of ['[1, 2, 3]', '"just a string"']) {
                    await sendRawLine(client, line);
                    const invalidRequest = await client.readUntil(
                        (message) => message?.error?.code === -32600,
                        20_000,
                    );
                    expect(invalidRequest.id).toBeNull();
                }

                // After all three rejections a full handshake still succeeds
                // on the same connection.
                const init = await initializeMcp(client);
                expect(init?.result?.protocolVersion).toBe('2025-06-18');
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('round-trips multi-byte UTF-8 payloads over NDJSON stdio', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const title = '默认标题 🎉 ünïcödé — MCP';
            await withMcpClient(workspace, async (client) => {
                const createId = nextMcpRequestId('create');
                await client.send({
                    jsonrpc: '2.0',
                    id: createId,
                    method: 'tools/call',
                    params: { name: 'task_create', arguments: { title } },
                });
                const created = await client.awaitResponse(createId);
                expect(created.error).toBeUndefined();

                const createdPayload = extractToolPayload(created) as Record<string, any>;
                const taskId = createdPayload?.task?.id ?? createdPayload?.id;
                expect(typeof taskId).toBe('string');

                const getId = nextMcpRequestId('get');
                await client.send({
                    jsonrpc: '2.0',
                    id: getId,
                    method: 'tools/call',
                    params: { name: 'task_get', arguments: { id: taskId } },
                });
                const fetched = await client.awaitResponse(getId);
                expect(fetched.error).toBeUndefined();

                const fetchedPayload = extractToolPayload(fetched) as Record<string, any>;
                const fetchedTask = fetchedPayload?.task ?? fetchedPayload;
                expect(fetchedTask?.title).toBe(title);

                const yaml = await workspace.readTaskYaml(taskId);
                expect(yaml).toContain(title);
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('exits cleanly when the client closes stdin', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnMcpClient(workspace);
            await initializeMcp(client);
            await client.dispose();

            const node = client.childProcess.nodeChildProcess;
            expect(node.exitCode).toBe(0);
            expect(node.signalCode).toBeNull();
        } finally {
            await workspace.dispose();
        }
    });
});
