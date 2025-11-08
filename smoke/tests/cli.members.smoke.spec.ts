import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
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

describe.concurrent('CLI strict member smoke scenarios', () => {
    it('rejects unlisted members and allows configured ones', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', STRICT_MEMBERS_CONFIG);

            const invalid = await workspace.runLotar(
                [
                    'task',
                    'add',
                    'Strict members should block intruders',
                    '--reporter=allowed@example.com',
                    '--assignee=intruder@example.com',
                ],
                { acceptExitCodes: [1] },
            );

            const invalidOutput = `${invalid.stdout}\n${invalid.stderr}`;
            expect(invalidOutput).toContain("Assignee 'intruder@example.com' is not in configured members");

            const filesBefore = await workspace.listTaskFiles();
            expect(filesBefore).toHaveLength(0);

            const created = await workspace.addTask('Strict members allow listed users', {
                args: ['--reporter=allowed@example.com', '--assignee=allowed@example.com'],
            });

            const taskYaml = await workspace.readTaskYaml(created.id);
            const task = parse(taskYaml) as Record<string, any>;

            expect(task.reporter).toBe('allowed@example.com');
            expect(task.assignee).toBe('allowed@example.com');
            expect(created.project).toBeTruthy();
        } finally {
            await workspace.dispose();
        }
    });
});
