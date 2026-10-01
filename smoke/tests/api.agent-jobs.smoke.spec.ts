import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { createFixtureAgents, createGateFixture } from '../helpers/agent-fixtures.js';
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
    readonly status?: string;
    readonly exit_code?: number;
}

async function fetchJobsEnvelope(serverUrl: string): Promise<JobsListEnvelope> {
    const response = await fetch(`${serverUrl}/api/jobs`);
    expect(response.ok).toBe(true);
    return (await response.json()) as JobsListEnvelope;
}

async function waitForJobStatus(
    serverUrl: string,
    jobId: string,
    statuses: readonly string[],
    timeoutMs = 10_000,
): Promise<JobsListEnvelope> {
    const deadline = Date.now() + timeoutMs;
    let lastStatus = '<missing>';
    for (;;) {
        const envelope = await fetchJobsEnvelope(serverUrl);
        const job = envelope.data.jobs.find((j) => j.id === jobId) ?? null;
        lastStatus = job?.status ?? lastStatus;
        if (job && statuses.includes(job.status)) {
            return envelope;
        }
        if (Date.now() >= deadline) {
            throw new Error(
                `Timed out after ${timeoutMs}ms waiting for job ${jobId} to reach ` +
                    `${statuses.join('|')} (last status: ${lastStatus})`,
            );
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
    }
}

// The service publishes the terminal registry status before it appends the
// final status line to the job log (finalize_job ordering), so log
// finalization is awaited on content: the last non-empty line must be a
// status entry matching the observed terminal state.
async function waitForFinalizedLog(
    logsDir: string,
    jobId: string,
    expectedStatus: string,
    timeoutMs = 10_000,
): Promise<JobLogEntry[]> {
    const logPath = path.join(logsDir, `${jobId}.jsonl`);
    const deadline = Date.now() + timeoutMs;
    for (;;) {
        const content = await fs.readFile(logPath, 'utf8').catch(() => null);
        if (content !== null) {
            const lines = content.trim().split('\n').filter((line) => line.length > 0);
            // A torn trailing line (read mid-append) parses to null and the
            // poll simply retries until the status line is fully written.
            const entries = lines.map((line) => {
                try {
                    return JSON.parse(line) as JobLogEntry;
                } catch {
                    return null;
                }
            });
            const last = entries[entries.length - 1];
            if (last?.type === 'status' && last.status === expectedStatus) {
                return entries.filter((entry): entry is JobLogEntry => entry !== null);
            }
        }
        if (Date.now() >= deadline) {
            throw new Error(
                `Timed out after ${timeoutMs}ms waiting for finalized job log ` +
                    `${logPath} with status ${expectedStatus}`,
            );
        }
        await new Promise((resolve) => setTimeout(resolve, 100));
    }
}

function agentJobsConfig(agentsYaml: string, options: { maxParallelJobs?: number } = {}): string {
    const worktreeLines = ['  worktree:', '    enabled: false'];
    if (options.maxParallelJobs !== undefined) {
        worktreeLines.push(`    max_parallel_jobs: ${options.maxParallelJobs}`);
    }
    return `default:
  project: TEST
statuses: [Todo, InProgress, Testing, Done]
priorities: [Low, Medium, High]
types: [Feature, Bug]

agent:
  logs_dir: .logs
${worktreeLines.join('\n')}

${agentsYaml}`;
}

describe.concurrent('Agent job smoke tests', () => {
    it('runs a gated agent job and captures progress events deterministically', async () => {
        const gate = await createGateFixture('lotar-smoke-gate-api-');
        const agents = await createFixtureAgents(
            {
                gated: {
                    script: gate.script,
                    args: gate.agentArgs({ mode: 'copilot', label: 'progress' }),
                },
            },
            'lotar-smoke-agents-gated-',
        );
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': agentJobsConfig(agents.yaml),
            },
        });
        let bodyFailed = false;

        try {
            // Create a test task
            const task = await workspace.addTask('Gated agent test task');

            const server = await startLotarServer(workspace);

            try {
                // Start a job with the gated agent
                const createResponse = await fetch(`${server.url}/api/jobs`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        ticket_id: task.id,
                        agent: 'gated',
                        prompt: 'Test prompt',
                    }),
                });

                expect(createResponse.ok).toBe(true);
                const createPayload = (await createResponse.json()) as JobEnvelope;
                const jobId = createPayload.data.job.id;
                expect(jobId).toBeTruthy();
                expect(createPayload.data.job.runner).toBe('copilot');
                // The creation handler spawns the worker thread before it
                // builds the response DTO, so an immediately started job may
                // already expose `running`. `queued` and `running` are the
                // only legitimate initial snapshots — never a terminal state
                // without a real run.
                expect(['queued', 'running']).toContain(createPayload.data.job.status);

                // Synchronize with the worker: the started sentinel proves the
                // runner child actually launched, and the gate holds it
                // mid-run, so the running snapshot below is real state.
                await gate.waitForStarted('progress');

                const runningEnvelope = await waitForJobStatus(server.url, jobId, ['running']);
                const runningJob = runningEnvelope.data.jobs.find((j) => j.id === jobId);
                expect(runningJob).toBeTruthy();
                expect(runningJob!.exit_code ?? null).toBeNull();
                expect(runningEnvelope.data.queue_stats.running).toBeGreaterThanOrEqual(1);

                await gate.release();

                const finalEnvelope = await waitForJobStatus(server.url, jobId, ['completed', 'failed']);
                const job = finalEnvelope.data.jobs.find((j) => j.id === jobId) ?? null;

                if (process.env.SMOKE_DEBUG === '1') {
                    console.debug('[smoke] final job state', job);
                }

                expect(job).toBeTruthy();
                expect(job!.status).toBe('completed');
                expect(job!.exit_code).toBe(0);
                expect(job!.summary).toContain('phase two');

                // The job log is finalized only after the terminal status is
                // published, so await the final status line before asserting
                // on the full event sequence.
                const logsDir = path.join(workspace.root, '.logs');
                const entries = await waitForFinalizedLog(logsDir, jobId, 'completed');

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
                expect(messageEvents[0].message).toContain('phase one / phase two done');

                // Verify header contains job metadata
                const header = entries.find((e) => e.type === 'header');
                expect(header).toBeTruthy();
                expect(header!.job_id).toBe(jobId);

                // Verify status entry finalized the log
                const status = entries.find((e) => e.type === 'status');
                expect(status).toBeTruthy();
                expect(status!.status).toBe('completed');
                expect(status!.exit_code).toBe(0);
            } catch (error) {
                bodyFailed = true;
                throw error;
            } finally {
                await server.stop();
            }
        } finally {
            // Owned-fixture contract: release the gate and bounded-await the
            // agent child's completion before the workspace is disposed.
            const unfinished = await gate.drain(10_000);
            await workspace.dispose();
            await gate.dispose();
            await agents.dispose();
            if (!bodyFailed && unfinished.length > 0) {
                throw new Error(`gate children did not finish after release: ${unfinished.join(', ')}`);
            }
        }
    });

    it('lists jobs and queue stats via the REST API', async () => {
        const agents = await createFixtureAgents(
            { mock: {} },
            'lotar-smoke-agents-list-',
        );

        try {
            const workspace = await SmokeWorkspace.create({
                seedFiles: {
                    '.tasks/config.yml': agentJobsConfig(agents.yaml, { maxParallelJobs: 2 }),
                },
            });

            try {
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
        } finally {
            await agents.dispose();
        }
    });
});
