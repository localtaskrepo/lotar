import { describe, expect, it } from 'vitest';
import { SmokeWorkspace } from '../helpers/workspace.js';

const AGENT_CONFIG = `default:
  project: AGCLI
issue:
  states: [Todo, InProgress, Done]
  priorities: [Low, Medium, High]
  types: [Feature, Bug]

agent:
  logs_dir: .logs
  worktree:
    enabled: false

agents:
  test-agent:
    runner: command
    command: "echo done"
    description: Test agent
`;

describe.concurrent('CLI agent commands smoke tests', () => {
    it('agent queue shows no pending entries', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': AGENT_CONFIG },
        });

        try {
            const result = await workspace.runLotar(['agent', 'queue']);
            // With no jobs queued, should report empty
            expect(result.stdout + result.stderr).toMatch(/no pending|empty|0 pending/i);
        } finally {
            await workspace.dispose();
        }
    });

    it('agent worktree list shows no worktrees', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': AGENT_CONFIG },
        });

        try {
            const result = await workspace.runLotar(['agent', 'worktree', 'list']);
            expect(result.stdout + result.stderr).toMatch(/no.*worktree|empty|0 worktree/i);
        } finally {
            await workspace.dispose();
        }
    });

    it('agent worktree cleanup runs without error', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': AGENT_CONFIG },
        });

        try {
            const result = await workspace.runLotar(['agent', 'worktree', 'cleanup']);
            // Should succeed with no worktrees to remove
            expect(result.stdout + result.stderr).toMatch(/no worktree|nothing|removed 0|0 worktree/i);
        } finally {
            await workspace.dispose();
        }
    });
});
