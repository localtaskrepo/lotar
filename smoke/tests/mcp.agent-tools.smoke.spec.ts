import { describe, expect, it } from 'vitest';
import { callTool, expectProtocolError, expectToolFailure, extractToolPayload, withMcpClient } from '../helpers/mcp-harness.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const CONFIG_WITH_AGENT = `default:
  project: MCP
  reporter: tester@example.com
  statuses: [Todo, InProgress, Done]
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
    description: Test agent for MCP smoke
`;

describe.concurrent('MCP agent tools smoke tests', () => {
    it('lists empty jobs via agent/list_jobs', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': CONFIG_WITH_AGENT },
        });

        try {
            await withMcpClient(workspace, async (client) => {
                const frame = await callTool(client, 2, 'agent_list_jobs', {});
                expect(frame.error).toBeUndefined();

                const payload = extractToolPayload(frame) as Record<string, any>;
                expect(payload.jobs).toBeDefined();
                expect(Array.isArray(payload.jobs)).toBe(true);
                expect(payload.queue_stats).toBeDefined();
                expect(typeof payload.queue_stats.queued).toBe('number');
                expect(typeof payload.queue_stats.running).toBe('number');
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('returns error for agent/status with missing id', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': CONFIG_WITH_AGENT },
        });

        try {
            await withMcpClient(workspace, async (client) => {
                const frame = await callTool(client, 2, 'agent_status', {});
                expectProtocolError(frame, -32602, 'Invalid params');
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('reports a tool failure for agent/status with non-existent job', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': CONFIG_WITH_AGENT },
        });

        try {
            await withMcpClient(workspace, async (client) => {
                const frame = await callTool(client, 2, 'agent_status', { id: 'nonexistent-job-id' });
                expectToolFailure(frame, 'Job not found');
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('reports a tool failure for agent/cancel with non-existent job', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': CONFIG_WITH_AGENT },
        });

        try {
            await withMcpClient(workspace, async (client) => {
                const frame = await callTool(client, 2, 'agent_cancel', { id: 'nonexistent-job-id' });
                expectToolFailure(frame, 'Job not found');
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('returns error for agent/run with missing required params', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': CONFIG_WITH_AGENT },
        });

        try {
            await withMcpClient(workspace, async (client) => {
                // Missing both ticket_id and prompt
                const frame1 = await callTool(client, 2, 'agent_run', {});
                expectProtocolError(frame1, -32602, 'Invalid params');

                // Missing prompt
                const frame2 = await callTool(client, 3, 'agent_run', { ticket_id: 'MCP-1' });
                expectProtocolError(frame2, -32602, 'Invalid params');
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('returns error for agent/send_message with missing params', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': CONFIG_WITH_AGENT },
        });

        try {
            await withMcpClient(workspace, async (client) => {
                // Missing both id and message
                const frame1 = await callTool(client, 2, 'agent_send_message', {});
                expectProtocolError(frame1, -32602, 'Invalid params');

                // Missing message
                const frame2 = await callTool(client, 3, 'agent_send_message', { id: 'some-id' });
                expectProtocolError(frame2, -32602, 'Invalid params');
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('agent tools appear in tools/list', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': CONFIG_WITH_AGENT },
        });

        try {
            await withMcpClient(workspace, async (client) => {
                await client.send({
                    jsonrpc: '2.0',
                    id: 2,
                    method: 'tools/list',
                });
                const list = await client.awaitResponse(2);

                const tools = list.result?.tools as Array<{ name: string }>;
                expect(tools).toBeDefined();
                const toolNames = tools.map((t) => t.name);

                expect(toolNames).toContain('agent_run');
                expect(toolNames).toContain('agent_status');
                expect(toolNames).toContain('agent_list_jobs');
                expect(toolNames).toContain('agent_cancel');
                expect(toolNames).toContain('agent_send_message');
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('agent_run with command runner creates and starts a job', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': CONFIG_WITH_AGENT },
        });

        try {
            const task = await workspace.addTask('MCP agent run test');

            await withMcpClient(workspace, async (client) => {
                const frame = await callTool(client, 2, 'agent_run', {
                    ticket_id: task.id,
                    prompt: 'Do something',
                    agent: 'test-agent',
                });
                expect(frame.error).toBeUndefined();

                const payload = extractToolPayload(frame) as Record<string, any>;
                expect(payload.id).toBeTruthy();
                expect(payload.ticket_id).toBe(task.id);
                // Job should be queued or already running/completed
                expect(['queued', 'running', 'completed']).toContain(payload.status);
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('agent_status returns details for an existing job', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': CONFIG_WITH_AGENT },
        });

        try {
            const task = await workspace.addTask('MCP status test');

            await withMcpClient(workspace, async (client) => {
                // Create a job first
                const runFrame = await callTool(client, 2, 'agent_run', {
                    ticket_id: task.id,
                    prompt: 'Status test',
                    agent: 'test-agent',
                });
                const runPayload = extractToolPayload(runFrame) as Record<string, any>;
                const jobId = runPayload.id as string;
                expect(jobId).toBeTruthy();

                // Wait a bit for the job to potentially complete
                await new Promise((r) => setTimeout(r, 1000));

                // Query status
                const statusFrame = await callTool(client, 3, 'agent_status', { id: jobId });
                expect(statusFrame.error).toBeUndefined();

                const statusPayload = extractToolPayload(statusFrame) as Record<string, any>;
                expect(statusPayload.id).toBe(jobId);
                expect(statusPayload.ticket_id).toBe(task.id);
                expect(statusPayload.status).toBeTruthy();
            });
        } finally {
            await workspace.dispose();
        }
    });
});
