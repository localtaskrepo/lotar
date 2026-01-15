import { execa } from 'execa';
import { describe, expect, it } from 'vitest';
import { ensureBinaryExists } from '../helpers/binary.js';
import { FramedMcpClient } from '../helpers/mcp.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('MCP tools/call result shape', () => {
    it('returns top-level content arrays for VS Code clients', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml':
                    `default:\n  project: CLI\nissue:\n  states: [Todo]\n  priorities: [Low]\n  types: [Feature]\n`,
            },
        });

        try {
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
                        name: 'config_show',
                        arguments: {},
                    },
                });
                const config = await client.readUntil((frame) => frame.message?.id === 2);
                expect(config.message?.error).toBeUndefined();

                const configContent = config.message?.result?.content;
                expect(Array.isArray(configContent)).toBe(true);
                expect(configContent.length).toBeGreaterThan(0);
                expect(configContent[0]?.type).toBe('text');

                const configWrapped = config.message?.result?.functionResponse?.response?.content;
                expect(Array.isArray(configWrapped)).toBe(true);
                expect(configWrapped.length).toBeGreaterThan(0);

                await client.send({
                    jsonrpc: '2.0',
                    id: 3,
                    method: 'tools/call',
                    params: {
                        name: 'project_list',
                        arguments: {},
                    },
                });
                const projects = await client.readUntil((frame) => frame.message?.id === 3);
                expect(projects.message?.error).toBeUndefined();

                const projectsContent = projects.message?.result?.content;
                expect(Array.isArray(projectsContent)).toBe(true);
                expect(projectsContent.length).toBeGreaterThan(0);
                expect(projectsContent[0]?.type).toBe('text');

                const projectsWrapped = projects.message?.result?.functionResponse?.response?.content;
                expect(Array.isArray(projectsWrapped)).toBe(true);
                expect(projectsWrapped.length).toBeGreaterThan(0);
            } finally {
                await client.dispose();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
