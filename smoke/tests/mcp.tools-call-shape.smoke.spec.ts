import { describe, expect, it } from 'vitest';
import { callTool, initializeFramedMcp, withFramedMcpClient } from '../helpers/mcp-harness.js';
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
            await withFramedMcpClient(workspace, async (client) => {
                const init = await initializeFramedMcp(client);
                expect(init.message?.error).toBeUndefined();

                const config = await callTool(client, 2, 'config_show', {});
                expect(config.message?.error).toBeUndefined();

                const configContent = config.message?.result?.content;
                expect(Array.isArray(configContent)).toBe(true);
                expect(configContent.length).toBeGreaterThan(0);
                expect(configContent[0]?.type).toBe('text');

                const configWrapped = config.message?.result?.functionResponse?.response?.content;
                expect(Array.isArray(configWrapped)).toBe(true);
                expect(configWrapped.length).toBeGreaterThan(0);

                const projects = await callTool(client, 3, 'project_list', {});
                expect(projects.message?.error).toBeUndefined();

                const projectsContent = projects.message?.result?.content;
                expect(Array.isArray(projectsContent)).toBe(true);
                expect(projectsContent.length).toBeGreaterThan(0);
                expect(projectsContent[0]?.type).toBe('text');

                const projectsWrapped = projects.message?.result?.functionResponse?.response?.content;
                expect(Array.isArray(projectsWrapped)).toBe(true);
                expect(projectsWrapped.length).toBeGreaterThan(0);
            });
        } finally {
            await workspace.dispose();
        }
    });
});
