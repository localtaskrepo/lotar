import { describe, expect, it } from 'vitest';
import { callTool, extractToolPayload, initializeFramedMcp, withFramedMcpClient } from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('MCP sprint_add identifiers', () => {
    it('accepts both sprint references and sprint_id', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const first = await workspace.addTask('MCP sprint_add identifier smoke A');
            const second = await workspace.addTask('MCP sprint_add identifier smoke B');

            await workspace.runLotar(['sprint', 'create', '--label', 'MCP Sprint Add Smoke']);

            await withFramedMcpClient(workspace, async (client) => {
                const init = await initializeFramedMcp(client);
                expect(init.message?.error).toBeUndefined();

                const addViaRef = await callTool(client, 2, 'sprint_add', {
                    sprint: '#1',
                    tasks: [first.id],
                });
                expect(addViaRef.message?.error).toBeUndefined();

                const addViaRefContent = addViaRef.message?.result?.content;
                expect(Array.isArray(addViaRefContent)).toBe(true);
                expect(addViaRefContent.length).toBeGreaterThan(0);

                const addViaRefPayload = extractToolPayload(addViaRef) as Record<string, any>;

                expect(addViaRefPayload.status).toBe('ok');
                expect(addViaRefPayload.sprint_id).toBe(1);
                expect(addViaRefPayload.modified).toContain(first.id);

                const addViaId = await callTool(client, 3, 'sprint_add', {
                    sprint_id: 1,
                    tasks: [second.id],
                });
                expect(addViaId.message?.error).toBeUndefined();

                const addViaIdContent = addViaId.message?.result?.content;
                expect(Array.isArray(addViaIdContent)).toBe(true);
                expect(addViaIdContent.length).toBeGreaterThan(0);

                const addViaIdPayload = extractToolPayload(addViaId) as Record<string, any>;

                expect(addViaIdPayload.status).toBe('ok');
                expect(addViaIdPayload.sprint_id).toBe(1);
                expect(addViaIdPayload.modified).toContain(second.id);
            });
        } finally {
            await workspace.dispose();
        }
    });
});
