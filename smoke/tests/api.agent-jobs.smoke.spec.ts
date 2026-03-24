import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

interface JobResponse {
    readonly id: string;
    readonly ticket_id: string;
    readonly runner: string;
    readonly status: string;
    readonly last_message?: string;
    readonly summary?: string;
    readonly exit_code?: number;
    readonly worktree_path?: string;
    readonly worktree_branch?: string;
}

interface JobEnvelope {
    readonly data: {
        readonly job: JobResponse;
    };
}

interface JobsListEnvelope {
    readonly data: {
        readonly jobs: JobResponse[];
        readonly queue_stats: {
            readonly queued: number;
            readonly running: number;
            readonly max_parallel?: number;
        };
    };
}

interface JobLogEntry {
    readonly type: string;
    readonly job_id?: string;
    readonly kind?: string;
    readonly message?: string;
    readonly at?: string;
}

describe.concurrent('Agent job smoke tests', () => {
    it('runs a mock agent job and captures progress events', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            // Create config with mock agent using custom command
            const mockAgentPath = path.resolve(__dirname, '../helpers/mock-agent.sh');
            const config = `
default:
  project: TEST
statuses: [Todo, InProgress, Testing, Done]
priorities: [Low, Medium, High]
types: [Feature, Bug]

agent:
  logs_dir: .logs
  worktree:
    enabled: false

agents:
  mock:
    runner: copilot
    command: "${mockAgentPath}"
    description: Mock agent for testing
`;

            await workspace.write('.tasks/config.yml', config);

            // Create a test task
            const task = await workspace.addTask('Mock agent test task');

            const server = await startLotarServer(workspace);

            try {
                // Start a job with the mock agent
                const createResponse = await fetch(`${server.url}/api/jobs`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        ticket_id: task.id,
                        agent: 'mock',
                        prompt: 'Test prompt',
                    }),
                });

                expect(createResponse.ok).toBe(true);
                const createPayload = (await createResponse.json()) as JobEnvelope;
                const jobId = createPayload.data.job.id;
                expect(jobId).toBeTruthy();
                expect(createPayload.data.job.runner).toBe('copilot');
                expect(createPayload.data.job.status).toBe('queued');

                // Wait for job to complete (max 10 seconds)
                let job: JobResponse | null = null;
                let lastListPayload: JobsListEnvelope | null = null;
                for (let i = 0; i < 20; i++) {
                    await new Promise((resolve) => setTimeout(resolve, 500));

                    const listResponse = await fetch(`${server.url}/api/jobs`);
                    expect(listResponse.ok).toBe(true);
                    lastListPayload = (await listResponse.json()) as JobsListEnvelope;
                    job = lastListPayload.data.jobs.find((j) => j.id === jobId) ?? null;

                    if (process.env.SMOKE_DEBUG === '1') {
                        console.debug('[smoke] job status poll', {
                            iteration: i,
                            jobId,
                            status: job?.status,
                            jobs: lastListPayload.data.jobs.map((j) => ({ id: j.id, status: j.status })),
                        });
                    }

                    if (job?.status === 'completed' || job?.status === 'errored') {
                        break;
                    }
                }

                if (process.env.SMOKE_DEBUG === '1') {
                    console.debug('[smoke] final job state', job);
                }

                expect(job).toBeTruthy();
                expect(job!.status).toBe('completed');
                expect(job!.exit_code).toBe(0);
                expect(job!.summary).toContain('mock agent');

                // Verify log file was created and contains expected events
                // logs_dir is relative to workspace root, not tasks dir
                const logsDir = path.join(workspace.root, '.logs');

                // Wait for logs directory to be created (it should exist by now since job completed)
                let logsDirExists = await fs.pathExists(logsDir);
                if (!logsDirExists) {
                    // Give a bit more time if needed
                    await new Promise((resolve) => setTimeout(resolve, 500));
                    logsDirExists = await fs.pathExists(logsDir);
                }

                if (process.env.SMOKE_DEBUG === '1') {
                    console.debug('[smoke] checking logs', {
                        logsDir,
                        exists: logsDirExists,
                    });
                }

                expect(logsDirExists).toBe(true);

                const logFiles = await fs.readdir(logsDir);
                const logFile = logFiles.find((f) => f.startsWith(jobId));
                expect(logFile).toBeTruthy();

                const logContent = await fs.readFile(path.join(logsDir, logFile!), 'utf8');
                const logLines = logContent.trim().split('\n');
                const entries = logLines.map((line) => JSON.parse(line) as JobLogEntry);

                // Verify expected event types are present
                const eventKinds = entries
                    .filter((e) => e.type === 'event')
                    .map((e) => e.kind);

                expect(eventKinds).toContain('agent_job_started');
                expect(eventKinds).toContain('agent_job_init');
                expect(eventKinds).toContain('agent_job_progress');
                expect(eventKinds).toContain('agent_job_message');
                expect(eventKinds).toContain('agent_job_result');
                expect(eventKinds).toContain('agent_job_completed');

                // Verify progress events contain expected text
                const progressEvents = entries.filter(
                    (e) => e.type === 'event' && e.kind === 'agent_job_progress',
                );
                expect(progressEvents.length).toBeGreaterThan(0);

                // Verify message event contains expected text
                const messageEvents = entries.filter(
                    (e) => e.type === 'event' && e.kind === 'agent_job_message',
                );
                expect(messageEvents.length).toBe(1);
                expect(messageEvents[0].message).toContain('mock agent');

                // Verify header contains job metadata
                const header = entries.find((e) => e.type === 'header');
                expect(header).toBeTruthy();
                expect(header!.job_id).toBe(jobId);

                // Verify status entry at end
                const status = entries.find((e) => e.type === 'status');
                expect(status).toBeTruthy();
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('lists jobs and queue stats via the REST API', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            // Create config with mock agent
            const mockAgentPath = path.resolve(__dirname, '../helpers/mock-agent.sh');
            const config = `
default:
  project: TEST
statuses: [Todo, InProgress, Done]
priorities: [Low, Medium, High]
types: [Feature]

agent:
  logs_dir: .logs
  worktree:
    enabled: false
    max_parallel_jobs: 2

agents:
  mock:
    runner: copilot
    command: "${mockAgentPath}"
`;

            await workspace.write('.tasks/config.yml', config);

            // Create a test task
            const task = await workspace.addTask('Queue stats test');

            const server = await startLotarServer(workspace);

            try {
                // Start a job
                const createResponse = await fetch(`${server.url}/api/jobs`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        ticket_id: task.id,
                        agent: 'mock',
                        prompt: 'Test',
                    }),
                });

                expect(createResponse.ok).toBe(true);

                // List jobs and verify queue stats are present
                const listResponse = await fetch(`${server.url}/api/jobs`);
                expect(listResponse.ok).toBe(true);

                const listPayload = (await listResponse.json()) as JobsListEnvelope;
                expect(listPayload.data.queue_stats).toBeDefined();
                expect(typeof listPayload.data.queue_stats.queued).toBe('number');
                expect(typeof listPayload.data.queue_stats.running).toBe('number');

                // Verify job is in the list
                expect(listPayload.data.jobs.length).toBeGreaterThan(0);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
