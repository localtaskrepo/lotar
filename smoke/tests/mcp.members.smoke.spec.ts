import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { callTool, extractToolPayload, initializeFramedMcp, withFramedMcpClient } from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const STRICT_MEMBERS_CONFIG = `default:
  project: CLI
  reporter: allowed@example.com
  assignee: allowed@example.com
  members:
    - allowed@example.com
    - reviewer@example.com
  strict_members: true
auto:
    populate_members: false
issue:
  states: [Todo, InProgress, Done]
  types: [Feature, Bug, Chore]
  priorities: [Low, Medium, High]
`;

describe.concurrent('MCP strict member smoke scenarios', () => {
    it('validates members when creating tasks', async () => {
        const workspace = await SmokeWorkspace.create();
        let createdTaskId = '';

        try {
            await workspace.write('.tasks/config.yml', STRICT_MEMBERS_CONFIG);

            await withFramedMcpClient(workspace, async (client) => {
                const init = await initializeFramedMcp(client);
                expect(init.message?.error).toBeUndefined();

                const invalid = await callTool(client, 2, 'task/create', {
                    title: 'MCP strict members invalid assignee',
                    project: 'CLI',
                    reporter: 'allowed@example.com',
                    assignee: 'intruder@example.com',
                });
                expect(invalid.message?.error?.message).toBe('Task create failed');
                const errorMessage = invalid.message?.error?.data?.message ?? '';
                expect(errorMessage).toContain("Assignee 'intruder@example.com' is not in configured members");

                const success = await callTool(client, 3, 'task/create', {
                    title: 'MCP strict members allowed assignee',
                    project: 'CLI',
                    reporter: 'allowed@example.com',
                    assignee: 'allowed@example.com',
                });
                expect(success.message?.error).toBeUndefined();

                const payload = extractToolPayload(success) as Record<string, any>;
                const dto: Record<string, any> = payload.task ?? payload;

                expect(typeof dto.id).toBe('string');
                expect(dto.id).toMatch(/^CLI-\d+$/);
                expect(dto.assignee).toBe('allowed@example.com');
                expect(dto.reporter).toBe('allowed@example.com');
                createdTaskId = String(dto.id ?? '');
            });

            const files = await workspace.listTaskFiles();
            expect(files).toHaveLength(1);

            const yaml = await workspace.readTaskYaml(createdTaskId);
            const task = parse(yaml) as Record<string, any>;
            expect(task.assignee).toBe('allowed@example.com');
            expect(task.reporter).toBe('allowed@example.com');
        } finally {
            await workspace.dispose();
        }
    });
});
