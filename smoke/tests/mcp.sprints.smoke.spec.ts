import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { callTool, extractToolPayload, withMcpClient } from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('MCP sprint smoke scenarios', () => {
    it('deletes a sprint via MCP tools and cleans memberships', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const first = await workspace.addTask('MCP Sprint Delete Task A');
            const second = await workspace.addTask('MCP Sprint Delete Task B');

            await workspace.runLotar(['sprint', 'create', '--label', 'MCP Delete Smoke Sprint']);
            await workspace.runLotar(['sprint', 'add', first.id, second.id, '--sprint', '1']);

            await withMcpClient(workspace, async (client) => {
                const deletion = await callTool(client, 1, 'sprint/delete', {
                    sprint: '#1',
                    cleanup_missing: true,
                });
                expect(deletion.error).toBeUndefined();

                const content = deletion.result?.functionResponse?.response?.content;
                expect(Array.isArray(content)).toBe(true);

                const summaryText = content?.[0]?.text ?? '';
                expect(summaryText).toContain('Deleted');

                const detailsText = content?.[1]?.text ?? '{}';
                let details: Record<string, any> = {};
                try {
                    details = JSON.parse(detailsText);
                } catch (error) {
                    throw new Error(`Failed to parse MCP delete payload: ${detailsText}\n${String(error)}`);
                }

                expect(details.deleted).toBe(true);
                expect(details.sprint_id).toBe(1);
                expect(details.removed_references).toBeGreaterThanOrEqual(0);
                expect(details.updated_tasks).toBeGreaterThanOrEqual(0);
            });

            const sprintPath = path.join(workspace.tasksDir, '@sprints', '1.yml');
            expect(await fs.pathExists(sprintPath)).toBe(false);

            const firstYaml = parse(await workspace.readTaskYaml(first.id)) as Record<string, any>;
            const secondYaml = parse(await workspace.readTaskYaml(second.id)) as Record<string, any>;

            const firstMembership = Array.isArray(firstYaml.sprints) ? firstYaml.sprints : [];
            const secondMembership = Array.isArray(secondYaml.sprints) ? secondYaml.sprints : [];

            expect(firstMembership).not.toContain(1);
            expect(secondMembership).not.toContain(1);
        } finally {
            await workspace.dispose();
        }
    });
});
