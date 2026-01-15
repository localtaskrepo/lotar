import { describe, expect, it } from 'vitest';
import { initializeFramedMcp, withFramedMcpClient } from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('MCP framed transport', () => {
    it('frames responses and notifications with Content-Length headers', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml':
                    `default:\n  project: CLI\nissue:\n  states: [Todo]\n  priorities: [Low]\n  types: [Feature]\n`,
            },
        });

        try {
            await withFramedMcpClient(workspace, async (client) => {
                const init = await initializeFramedMcp(client);
                expect(init.headers).toMatch(/Content-Length:\s*\d+/i);
                expect(init.message?.result?.protocolVersion).toBe('2025-06-18');

                await client.send({
                    jsonrpc: '2.0',
                    id: 2,
                    method: 'tools/list',
                });
                const list = await client.readUntil((frame) => frame.message?.id === 2);
                expect(Array.isArray(list.message?.result?.tools)).toBe(true);
                expect(list.headers).toMatch(/Content-Length:\s*\d+/i);

                const configA =
                    `default:\n  project: CLI\nissue:\n  states: [Todo,Doing]\n  priorities: [Low]\n  types: [Feature]\n`;
                const configB =
                    `default:\n  project: CLI\nissue:\n  states: [Todo,Doing,Done]\n  priorities: [Low]\n  types: [Feature]\n`;

                let notification: any = null;
                for (let attempt = 0; attempt < 6; attempt += 1) {
                    await workspace.write('.tasks/config.yml', attempt % 2 === 0 ? configA : configB);
                    try {
                        notification = await client.readUntil(
                            (frame) => frame.message?.method === 'tools/listChanged',
                            10000,
                        );
                        break;
                    } catch {
                        await new Promise((resolve) => setTimeout(resolve, 200));
                    }
                }

                if (!notification) {
                    notification = await client.readUntil(
                        (frame) => frame.message?.method === 'tools/listChanged',
                        60000,
                    );
                }
                expect(notification.headers).toMatch(/Content-Length:\s*\d+/i);
                expect(Array.isArray(notification.message?.params?.hintCategories)).toBe(true);
            });
        } finally {
            await workspace.dispose();
        }
    });
});
