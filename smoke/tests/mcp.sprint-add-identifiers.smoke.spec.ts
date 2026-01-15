import { execa } from 'execa';
import { describe, expect, it } from 'vitest';
import { ensureBinaryExists } from '../helpers/binary.js';
import { FramedMcpClient } from '../helpers/mcp.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('MCP sprint_add identifiers', () => {
    it('accepts both sprint references and sprint_id', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const first = await workspace.addTask('MCP sprint_add identifier smoke A');
            const second = await workspace.addTask('MCP sprint_add identifier smoke B');

            await workspace.runLotar(['sprint', 'create', '--label', 'MCP Sprint Add Smoke']);

            const binary = await ensureBinaryExists();
            const child = execa(binary, ['mcp'], {
                cwd: workspace.root,
                env: workspace.env,
                stdin: 'pipe',
                stdout: 'pipe',
                stderr: 'pipe',
            });

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
                expect(init.message?.error).toBeUndefined();

                await client.send({
                    jsonrpc: '2.0',
                    id: 2,
                    method: 'tools/call',
                    params: {
                        name: 'sprint_add',
                        arguments: {
                            sprint: '#1',
                            tasks: [first.id],
                        },
                    },
                });
                const addViaRef = await client.readUntil((frame) => frame.message?.id === 2);
                expect(addViaRef.message?.error).toBeUndefined();

                const addViaRefContent = addViaRef.message?.result?.content;
                expect(Array.isArray(addViaRefContent)).toBe(true);
                expect(addViaRefContent.length).toBeGreaterThan(0);

                const addViaRefPayloadText = addViaRefContent.at(-1)?.text ?? '{}';
                const addViaRefPayload = JSON.parse(addViaRefPayloadText) as Record<string, any>;

                expect(addViaRefPayload.status).toBe('ok');
                expect(addViaRefPayload.sprint_id).toBe(1);
                expect(addViaRefPayload.modified).toContain(first.id);

                await client.send({
                    jsonrpc: '2.0',
                    id: 3,
                    method: 'tools/call',
                    params: {
                        name: 'sprint_add',
                        arguments: {
                            sprint_id: 1,
                            tasks: [second.id],
                        },
                    },
                });
                const addViaId = await client.readUntil((frame) => frame.message?.id === 3);
                expect(addViaId.message?.error).toBeUndefined();

                const addViaIdContent = addViaId.message?.result?.content;
                expect(Array.isArray(addViaIdContent)).toBe(true);
                expect(addViaIdContent.length).toBeGreaterThan(0);

                const addViaIdPayloadText = addViaIdContent.at(-1)?.text ?? '{}';
                const addViaIdPayload = JSON.parse(addViaIdPayloadText) as Record<string, any>;

                expect(addViaIdPayload.status).toBe('ok');
                expect(addViaIdPayload.sprint_id).toBe(1);
                expect(addViaIdPayload.modified).toContain(second.id);
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
