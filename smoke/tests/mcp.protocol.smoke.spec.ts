import { execa } from 'execa';
import { describe, expect, it } from 'vitest';
import { ensureBinaryExists } from '../helpers/binary.js';
import { FramedMcpClient } from '../helpers/mcp.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('MCP framed transport', () => {
    it('frames responses and notifications with Content-Length headers', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const binary = await ensureBinaryExists();
            const child = execa(binary, ['mcp'], {
                cwd: workspace.root,
                env: workspace.env,
                stdin: 'pipe',
                stdout: 'pipe',
                stderr: 'pipe',
            });

            if (!child.stdout || !child.stdin) {
                throw new Error('Expected piped stdio when starting lotar mcp');
            }

            child.stderr?.setEncoding('utf8');
            child.stderr?.resume();

            const client = new FramedMcpClient(child);
            try {
                await client.send({
                    jsonrpc: '2.0',
                    id: 1,
                    method: 'initialize',
                    params: {
                        protocolVersion: '2025-06-18',
                    },
                });
                const init = await client.readUntil((frame) => frame.message?.id === 1);
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

                await workspace.write(
                    '.tasks/config.yml',
                    `default:\n  project: CLI\nissue:\n  states: [Todo]\n  priorities: [Low]\n  types: [Feature]\n`,
                );

                const notification = await client.readUntil(
                    (frame) => frame.message?.method === 'tools/listChanged',
                    10000,
                );
                expect(notification.headers).toMatch(/Content-Length:\s*\d+/i);
                expect(Array.isArray(notification.message?.params?.hintCategories)).toBe(true);
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
