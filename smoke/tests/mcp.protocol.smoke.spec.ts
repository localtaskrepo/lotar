import { describe, expect, it } from 'vitest';
import { initializeFramedMcp, spawnFramedMcp } from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const BASIC_CONFIG = `default:\n  project: CLI\nissue:\n  states: [Todo]\n  priorities: [Low]\n  types: [Feature]\n`;

describe.concurrent('MCP framed transport compatibility', () => {
    it('supports the complete handshake over Content-Length frames', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnFramedMcp(workspace);
            try {
                await client.send({
                    jsonrpc: '2.0',
                    id: 'compat-init',
                    method: 'initialize',
                    params: {
                        protocolVersion: '2025-06-18',
                        capabilities: {},
                        clientInfo: { name: 'lotar-smoke', version: '1.0.0' },
                    },
                });
                const initFrame = await client.readUntil((frame) => frame.message?.id === 'compat-init');
                expect(initFrame.message?.error).toBeUndefined();
                expect(initFrame.headers).toMatch(/Content-Length:\s*\d+/i);
                expect(initFrame.message?.result?.protocolVersion).toBe('2025-06-18');

                await client.send({ jsonrpc: '2.0', method: 'notifications/initialized' });

                await client.send({ jsonrpc: '2.0', id: 'compat-ping', method: 'ping' });
                const pongFrame = await client.readUntil((frame) => frame.message?.id === 'compat-ping');
                expect(pongFrame.message?.error).toBeUndefined();
                expect(pongFrame.message?.result).toEqual({});
                expect(pongFrame.headers).toMatch(/Content-Length:\s*\d+/i);

                await client.send({ jsonrpc: '2.0', id: 'compat-list', method: 'tools/list' });
                const listFrame = await client.readUntil((frame) => frame.message?.id === 'compat-list');
                expect(Array.isArray(listFrame.message?.result?.tools)).toBe(true);
                expect(listFrame.headers).toMatch(/Content-Length:\s*\d+/i);
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('emits framed tools-change notifications with Content-Length headers', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnFramedMcp(workspace);
            try {
                await initializeFramedMcp(client);

                const configA =
                    `default:\n  project: CLI\nissue:\n  states: [Todo,Doing]\n  priorities: [Low]\n  types: [Feature]\n`;
                const configB =
                    `default:\n  project: CLI\nissue:\n  states: [Todo,Doing,Done]\n  priorities: [Low]\n  types: [Feature]\n`;

                let notification = null;
                for (let attempt = 0; attempt < 6; attempt += 1) {
                    await workspace.write('.tasks/config.yml', attempt % 2 === 0 ? configA : configB);
                    try {
                        notification = await client.readUntil(
                            (frame) => frame.message?.method === 'notifications/tools/list_changed',
                            10_000,
                        );
                        break;
                    } catch {
                        await new Promise((resolve) => setTimeout(resolve, 200));
                    }
                }

                if (!notification) {
                    notification = await client.readUntil(
                        (frame) => frame.message?.method === 'notifications/tools/list_changed',
                        60_000,
                    );
                }
                expect(notification.headers).toMatch(/Content-Length:\s*\d+/i);
                expect(notification.message?.params?.hintCategories).toBeDefined();
                expect(Array.isArray(notification.message?.params?.hintCategories)).toBe(true);
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('rejects oversized framed bodies and stays in sync', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnFramedMcp(workspace);
            try {
                await initializeFramedMcp(client);

                const oversizedLength = 10 * 1024 * 1024 + 1;
                const header = `Content-Length: ${oversizedLength}\r\n\r\n`;
                await new Promise<void>((resolve, reject) => {
                    client.childProcess.stdin!.write(header, (error) => (error ? reject(error) : resolve()));
                });
                const chunk = Buffer.alloc(64 * 1024, 0x20);
                let remaining = oversizedLength;
                while (remaining > 0) {
                    const n = Math.min(chunk.length, remaining);
                    remaining -= n;
                    await new Promise<void>((resolve, reject) => {
                        client.childProcess.stdin!.write(chunk.subarray(0, n), (error) =>
                            error ? reject(error) : resolve(),
                        );
                    });
                }

                const parseError = await client.readUntil(
                    (frame) => frame.message?.error?.code === -32700,
                    20_000,
                );
                expect(parseError.message?.id).toBeNull();
                expect(parseError.message?.error?.message).toBe('Parse error');
                expect(String(parseError.message?.error?.data?.details)).toContain('maximum frame size');

                await client.send({ jsonrpc: '2.0', id: 'after-oversize', method: 'tools/list' });
                const listFrame = await client.readUntil((frame) => frame.message?.id === 'after-oversize');
                expect(Array.isArray(listFrame.message?.result?.tools)).toBe(true);
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('reports a parse error when a framed body is truncated by EOF', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASIC_CONFIG },
        });

        try {
            const client = await spawnFramedMcp(workspace);
            try {
                await initializeFramedMcp(client);

                await new Promise<void>((resolve, reject) => {
                    client.childProcess.stdin!.write(
                        'Content-Length: 100\r\n\r\n{"jsonrpc":"2.0","id":1',
                        (error) => (error ? reject(error) : resolve()),
                    );
                });
                client.childProcess.stdin!.end();

                const parseError = await client.readUntil(
                    (frame) => frame.message?.error?.code === -32700,
                    20_000,
                );
                expect(parseError.message?.id).toBeNull();
                expect(parseError.message?.error?.message).toBe('Parse error');
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
