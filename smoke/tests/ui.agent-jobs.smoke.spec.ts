import fs from 'fs-extra';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { ACP_AGENT_SCRIPT, createFixtureAgents, createGateFixture, nodeExecutable } from '../helpers/agent-fixtures.js';
import { gitAvailable } from '../helpers/git.js';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { assigneePickerValues, pickAssignee, withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const reviewedTemplatePath = path.resolve(
    __dirname,
    '../../crates/lotar-lib/src/cli/handlers/config/init/templates/agent-reviewed.yml',
);

interface JobRecord {
    readonly id: string;
    readonly status: string;
    readonly exit_code?: number | null;
}

function configWithAgents(agentsYaml: string, options: { maxParallelJobs?: number } = {}): string {
    const worktreeLines = ['  worktree:', '    enabled: false'];
    if (options.maxParallelJobs !== undefined) {
        worktreeLines.push(`    max_parallel_jobs: ${options.maxParallelJobs}`);
    }
    return `default:
  project: UIAG
issue:
  states: [Todo, InProgress, Done]
  priorities: [Low, Medium, High]
  types: [Feature, Bug]

agent:
  logs_dir: .logs
${worktreeLines.join('\n')}

${agentsYaml}`;
}

async function writeReviewedTemplateWorkflow(
    workspace: SmokeWorkspace,
    mockLauncherCommand: string,
    worktreeDir: string,
) {
    const template = parse(await fs.readFile(reviewedTemplatePath, 'utf8')) as {
        config: Record<string, unknown>;
        automation: Record<string, unknown>;
    };

    const config = template.config as {
        project?: { name?: string };
        agents?: Record<string, Record<string, unknown>>;
        agent?: { worktree?: Record<string, unknown> };
    };
    if (config.project) {
        config.project.name = 'UI Reviewed';
    }
    // The default worktree root is a sibling of the workspace, outside its
    // cleanup; the test owns this one and removes it.
    if (config.agent?.worktree) {
        config.agent.worktree.dir = worktreeDir;
    }
    for (const agentName of ['implement', 'test', 'merge']) {
        const agent = config.agents?.[agentName];
        if (!agent) {
            continue;
        }
        agent.runner = 'command';
        agent.command = mockLauncherCommand;
        agent.args = [];
    }

    await workspace.write('.tasks/UIAG/config.yml', stringify(config));
    await workspace.write('.tasks/automation.yml', stringify({ automation: template.automation }));
}

async function waitUntil(description: string, timeoutMs: number, condition: () => Promise<boolean>): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (await condition()) {
            return;
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`Timed out after ${timeoutMs}ms waiting for ${description}`);
}

// Waits are bounded by time, not by attempts: under full-suite load a single
// round-trip can take seconds, and a fixed attempt count then gives up early.
const WAIT_BUDGET_MS = 45_000;

/**
 * For a wait that must see agent jobs end however busy the machine is: it ends when the jobs do and the limit only
 * catches a hang (DEV-751: the reviewed workflow's jobs outlasted 20 s at a load average of 25 to 50).
 */
const JOB_END_LIMIT_MS = 240_000;

async function waitForTaskRowText(page: import('@playwright/test').Page, taskLabel: string, expected: string) {
    const row = page.locator('tr', { hasText: taskLabel }).first();
    const deadline = Date.now() + JOB_END_LIMIT_MS;
    while (Date.now() < deadline) {
        const text = await row.textContent();
        if (text?.includes(expected)) {
            return text;
        }
        await page.waitForTimeout(250);
    }
    throw new Error(`Timed out after ${JOB_END_LIMIT_MS}ms waiting for task ${taskLabel} to include ${expected}`);
}

async function waitForTicketChip(page: import('@playwright/test').Page, taskId: string) {
    const ticketChip = page.locator('button.chip', { hasText: taskId }).first();
    await ticketChip.waitFor({ state: 'visible', timeout: 20_000 });
    return ticketChip;
}

async function waitForText(page: import('@playwright/test').Page, selector: string, expected: string) {
    const deadline = Date.now() + WAIT_BUDGET_MS;
    while (Date.now() < deadline) {
        const text = await page.textContent(selector).catch(() => null);
        if (text?.includes(expected)) {
            return text;
        }
        await page.waitForTimeout(250);
    }
    throw new Error(`Timed out waiting for ${selector} to include ${expected}`);
}

async function startJobViaApi(serverUrl: string, ticketId: string, agent: string, prompt: string) {
    const response = await fetch(`${serverUrl}/api/jobs`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ ticket_id: ticketId, agent, prompt }),
    });
    expect(response.ok).toBe(true);
}

async function fetchJobsViaApi(serverUrl: string, ticketId?: string): Promise<JobRecord[]> {
    const suffix = ticketId ? `?ticket_id=${ticketId}` : '';
    const response = await fetch(`${serverUrl}/api/jobs${suffix}`);
    expect(response.ok).toBe(true);
    const payload = (await response.json()) as {
        data?: { jobs?: Array<{ id?: string; status?: string; exit_code?: number | null }> };
    };
    return (payload.data?.jobs ?? []).map((job) => ({
        id: job.id ?? '',
        status: job.status ?? '',
        exit_code: job.exit_code,
    }));
}

async function waitForJobStatusViaApi(serverUrl: string, ticketId: string, expected: string): Promise<JobRecord> {
    const deadline = Date.now() + WAIT_BUDGET_MS;
    while (Date.now() < deadline) {
        const job = (await fetchJobsViaApi(serverUrl, ticketId))[0];
        if (job && job.status === expected) {
            return job;
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`Timed out waiting for the job of ${ticketId} to reach status ${expected}`);
}

async function createTaskViaApi(serverUrl: string, title: string, project: string, reporter: string) {
    const response = await fetch(`${serverUrl}/api/tasks/add`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title, project, reporter }),
    });
    expect(response.ok).toBe(true);
    const payload = (await response.json()) as {
        data?: { id?: string } | { task?: { id?: string } };
    };
    const taskId =
        'task' in (payload.data ?? {})
            ? (payload.data as { task?: { id?: string } }).task?.id
            : (payload.data as { id?: string } | undefined)?.id;
    expect(taskId).toBeTruthy();
    return taskId as string;
}

async function updateTaskViaApi(serverUrl: string, patch: Record<string, unknown>) {
    const response = await fetch(`${serverUrl}/api/tasks/update`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(patch),
    });
    expect(response.ok).toBe(true);
}

async function waitForJobCountViaApi(serverUrl: string, ticketId: string, expected: number) {
    const deadline = Date.now() + WAIT_BUDGET_MS;
    while (Date.now() < deadline) {
        const jobs = await fetchJobsViaApi(serverUrl, ticketId);
        if (jobs.length >= expected) {
            return jobs.length;
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`Timed out waiting for ${expected} jobs for ${ticketId}`);
}

async function waitForCompletedJobCountViaApi(serverUrl: string, ticketId: string, expected: number) {
    const deadline = Date.now() + WAIT_BUDGET_MS;
    while (Date.now() < deadline) {
        const jobs = await fetchJobsViaApi(serverUrl, ticketId);
        if (jobs.length >= expected && jobs.every((job) => job.status === 'completed')) {
            return jobs.length;
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`Timed out waiting for ${expected} completed jobs for ${ticketId}`);
}

describe.concurrent('UI Agent Jobs page smoke tests', () => {
    it('renders page heading and queue stats', async () => {
        const agents = await createFixtureAgents(
            { mock: { description: 'Mock agent for UI tests' } },
            'lotar-smoke-agents-heading-',
        );

        try {
            const workspace = await SmokeWorkspace.create({
                seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
            });

            try {
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/agents`, async (page) => {
                        await page.waitForSelector('h1', { timeout: 15_000 });
                        const heading = await page.textContent('h1');
                        expect(heading).toContain('Agents');

                        // Queue stats should render
                        await page.waitForSelector('.queue-stats-card', { timeout: 10_000 });
                        const statsText = await page.textContent('.queue-stats-card');
                        expect(statsText).toContain('Running');
                        expect(statsText).toContain('Queued');
                        expect(statsText).toContain('Max parallel');
                    });
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

    it('lists the queue by priority with kinds and moves an entry to the top', async () => {
        const agents = await createFixtureAgents({ fix: {}, review: {} }, 'lotar-smoke-agents-queue-');

        try {
            const workspace = await SmokeWorkspace.create({
                // A job limit of 0 keeps every entry waiting in the queue.
                seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml, { maxParallelJobs: 0 }) },
            });
            const env = { LOTAR_AGENT_QUEUE_DISABLE_WORKER: '1' };

            try {
                const low = await workspace.addTask('Example low fix', { args: ['--priority', 'Low'] });
                const medium = await workspace.addTask('Example medium review', { args: ['--priority', 'Medium'] });
                const high = await workspace.addTask('Example high fix', { args: ['--priority', 'High'] });
                for (const [task, agent] of [
                    [low, 'fix'],
                    [medium, 'review'],
                    [high, 'fix'],
                ] as const) {
                    await workspace.runLotar(['agent', 'run', task.id, `Work ${task.id}`, '--agent', agent], { env });
                }
                const server = await startLotarServer(workspace, { env });
                try {
                    await withPage(`${server.url}/agents`, async (page) => {
                        const rows = page.locator('[data-testid="agent-queue-entry"]');
                        await expect.poll(() => rows.count(), { timeout: 15_000 }).toBe(3);
                        const order = async () =>
                            (await rows.locator('.chip').allTextContents()).map((text) => text.trim());
                        expect(await order()).toEqual([high.id, medium.id, low.id]);
                        // The stats count the same entries the list shows.
                        expect(await page.locator('.stat-value.queued').textContent()).toBe('3');
                        const reviewRow = page.locator(
                            `[data-testid="agent-queue-entry"][data-ticket-id="${medium.id}"]`,
                        );
                        expect(await reviewRow.locator('.queue-kind').textContent()).toBe('review');
                        expect(await reviewRow.textContent()).toContain('Medium');

                        await page
                            .locator(`[data-testid="agent-queue-entry"][data-ticket-id="${low.id}"]`)
                            .getByRole('button', { name: `Move to the top: ${low.id}` })
                            .click();
                        await expect.poll(order, { timeout: 10_000 }).toEqual([low.id, high.id, medium.id]);
                    });
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

    it('shows a completed job in the job list', async () => {
        const agents = await createFixtureAgents({ mock: {} }, 'lotar-smoke-agents-completed-');

        try {
            const workspace = await SmokeWorkspace.create({
                seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
            });

            try {
                const task = await workspace.addTask('UI job test');
                const server = await startLotarServer(workspace);
                try {
                    await startJobViaApi(server.url, task.id, 'mock', 'Test job for UI');
                    await waitForJobStatusViaApi(server.url, task.id, 'completed');

                    await withPage(`${server.url}/agents`, async (page) => {
                        // Wait for the job card to appear
                        await page.waitForSelector('.job-card', { timeout: 15_000 });
                        const cardText = await page.textContent('.job-card');
                        expect(cardText).toContain('completed');
                        expect(await (await waitForTicketChip(page, task.id)).count()).toBe(1);
                    });
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

    it('shows the prompt and settings a finished job ran with behind its Instructions button', async () => {
        const agents = await createFixtureAgents({ mock: {} }, 'lotar-smoke-agents-inputs-');

        try {
            const workspace = await SmokeWorkspace.create({
                seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
            });

            try {
                const task = await workspace.addTask('Run inputs test', { args: ['-p', 'UIAG'] });
                const server = await startLotarServer(workspace);
                try {
                    await startJobViaApi(server.url, task.id, 'mock', 'Inputs smoke request');
                    await waitForJobStatusViaApi(server.url, task.id, 'completed');

                    await withPage(`${server.url}/agents`, async (page) => {
                        await page.waitForSelector('.job-card', { timeout: 15_000 });
                        await page.getByRole('button', { name: 'Show logs', exact: true }).click();
                        // While the logs load, the card shows its own Instructions button; the loaded log replaces
                        // it, closing a panel opened there. Wait for the log and use the button in its header.
                        const header = page.locator('.job-log__header');
                        await header.waitFor({ state: 'visible', timeout: 15_000 });
                        const toggle = header.getByRole('button', { name: 'Instructions', exact: true });
                        expect(await toggle.getAttribute('aria-expanded')).toBe('false');

                        const loaded = page.waitForResponse(
                            (response) => new URL(response.url()).pathname === '/api/jobs/inputs',
                        );
                        await toggle.click();
                        expect((await loaded).ok()).toBe(true);

                        const settings = page.getByRole('table', { name: 'Settings' });
                        await settings.waitFor({ state: 'visible', timeout: 15_000 });
                        // The button sits in the log's header, the panel full width below it (DEV-769).
                        expect(await page.locator('.job-log__header .run-inputs__panel').count()).toBe(0);
                        const headerBox = await header.boundingBox();
                        const panel = await page.locator('.job-log > .run-inputs__panel').boundingBox();
                        expect(headerBox && panel).toBeTruthy();
                        expect(panel!.y).toBeGreaterThanOrEqual(headerBox!.y + headerBox!.height);
                        expect(Math.abs(panel!.width - headerBox!.width)).toBeLessThan(2);
                        const setting = (name: string) =>
                            settings
                                .locator('tr')
                                .filter({ has: page.locator('th', { hasText: new RegExp(`^${name}$`) }) })
                                .locator('td')
                                .textContent();
                        expect(await setting('Runner')).toBe('command');
                        expect(await setting('Agent profile')).toBe('mock');

                        const request = page.locator('details', { hasText: 'User request' });
                        await request.locator('summary').click();
                        expect(await request.locator('pre').textContent()).toContain('Inputs smoke request');
                        expect(await page.locator('.run-inputs__panel details summary').allTextContents()).toEqual(
                            expect.arrayContaining([expect.stringContaining('Agent instructions')]),
                        );
                        await page.getByRole('button', { name: 'Copy prompt', exact: true }).waitFor();
                        await page.getByRole('button', { name: 'Copy all', exact: true }).waitFor();
                    });
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

    it('keeps the Instructions panel opened while the logs load open once they arrive (DEV-888)', async () => {
        const agents = await createFixtureAgents({ mock: {} }, 'lotar-smoke-agents-inputs-wait-');

        try {
            const workspace = await SmokeWorkspace.create({
                seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
            });

            try {
                const task = await workspace.addTask('Run inputs while loading', { args: ['-p', 'UIAG'] });
                const server = await startLotarServer(workspace);
                try {
                    await startJobViaApi(server.url, task.id, 'mock', 'Inputs while loading');
                    await waitForJobStatusViaApi(server.url, task.id, 'completed');

                    await withPage(`${server.url}/agents`, async (page) => {
                        await page.waitForSelector('.job-card', { timeout: 15_000 });
                        let releaseLogs = () => {};
                        const logsHeld = new Promise<void>((resolve) => (releaseLogs = resolve));
                        await page.route('**/api/jobs/logs?**', async (route) => {
                            await logsHeld;
                            await route.continue();
                        });

                        await page.getByRole('button', { name: 'Show logs', exact: true }).click();
                        await page.getByText('Loading logs…').waitFor();
                        await page.getByRole('button', { name: 'Instructions', exact: true }).click();
                        await page.getByRole('table', { name: 'Settings' }).waitFor({ state: 'visible' });

                        releaseLogs();
                        await page.locator('.job-log').waitFor();
                        const toggle = page.locator('.job-log__instructions');
                        expect(await toggle.textContent()).toContain('Hide instructions');
                        expect(await toggle.getAttribute('aria-expanded')).toBe('true');
                        expect(await page.locator('.job-log > .run-inputs__panel').isVisible()).toBe(true);
                    });
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

    it('guides setting up the first agent when none is configured', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': configWithAgents('') },
        });
        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/agents`, async (page) => {
                    const panel = page.getByTestId('agent-setup');
                    await panel.waitFor({ timeout: 15_000 });
                    expect(await page.locator('.filter-tab').count()).toBe(0);
                    await panel.locator('#agent-setup-name').fill('helper');
                    await panel.getByRole('button', { name: 'Add agent', exact: true }).click();
                    await page.locator('.profile-chip', { hasText: '@helper' }).waitFor({ timeout: 10_000 });
                    expect(await panel.count()).toBe(0);
                });
                const profiles = await (await fetch(`${server.url}/api/agents/profiles`)).json();
                expect(profiles.data.profiles.map((p: { name: string }) => p.name)).toEqual(['helper']);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('starts a job from the Run agent dialog and shows the ticket it ran on', async ({ expect }) => {
        const agents = await createFixtureAgents({ mock: {} }, 'lotar-smoke-agents-run-dialog-');

        try {
            const workspace = await SmokeWorkspace.create({
                seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
            });

            try {
                const task = await workspace.addTask('Dialog started job', { args: ['-p', 'UIAG'] });
                const server = await startLotarServer(workspace);
                try {
                    await withPage(`${server.url}/agents`, async (page) => {
                        await page.getByRole('button', { name: 'Run agent', exact: true }).click();
                        const dialog = page.getByTestId('agent-run-dialog');
                        await dialog.locator('#agent-run-ticket').fill(task.id);
                        await expect
                            .poll(() => dialog.locator('#agent-run-agent').getAttribute('data-value'))
                            .toBe('@mock');
                        await dialog.getByRole('button', { name: 'Start job', exact: true }).click();

                        await page.waitForSelector('.job-card.job-completed', { timeout: 20_000 });
                        await expect
                            .poll(() => page.locator('.ticket-group-card__header').first().textContent())
                            .toContain('Dialog started job');
                        expect(await dialog.count()).toBe(0);
                    });
                    const jobs = await fetchJobsViaApi(server.url, task.id);
                    expect(jobs).toHaveLength(1);
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

    it('completed filter removes non-matching jobs and keeps matching ones', async () => {
        const agents = await createFixtureAgents(
            {
                ok: { description: 'Mock agent that completes' },
                broken: { args: ['--exit', '1'], description: 'Mock agent that fails' },
            },
            'lotar-smoke-agents-filter-',
        );

        try {
            const workspace = await SmokeWorkspace.create({
                seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
            });

            try {
                const completedTask = await workspace.addTask('Filter completed task', { args: ['-p', 'UIAG'] });
                const failedTask = await workspace.addTask('Filter failed task', { args: ['-p', 'UIAG'] });
                const server = await startLotarServer(workspace);

                try {
                    await startJobViaApi(server.url, completedTask.id, 'ok', 'Complete the job');
                    await startJobViaApi(server.url, failedTask.id, 'broken', 'Fail the job');

                    // Wait for the deterministic terminal states before touching the UI.
                    const completedJob = await waitForJobStatusViaApi(server.url, completedTask.id, 'completed');
                    expect(completedJob.exit_code).toBe(0);
                    const failedJob = await waitForJobStatusViaApi(server.url, failedTask.id, 'failed');
                    expect(failedJob.exit_code).toBe(1);

                    await withPage(`${server.url}/agents`, async (page) => {
                        await page.waitForSelector('.filter-tab', { timeout: 15_000 });

                        const tabs = await page.$$eval('.filter-tab', (els) => els.map((el) => el.textContent.trim()));
                        expect(tabs.some((t) => t.includes('All'))).toBe(true);
                        expect(tabs.some((t) => t.includes('Running'))).toBe(true);
                        expect(tabs.some((t) => t.includes('Completed'))).toBe(true);
                        expect(tabs.some((t) => t.includes('Failed'))).toBe(true);

                        const completedCards = page.locator('.job-card.job-completed');
                        const failedCards = page.locator('.job-card.job-failed');
                        await waitUntil(
                            'both job cards to render with terminal statuses',
                            20_000,
                            async () => (await completedCards.count()) === 1 && (await failedCards.count()) === 1,
                        );

                        await page.click('.filter-tab:has-text("Completed")');
                        await waitUntil(
                            'the failed job card to be removed by the Completed filter',
                            5_000,
                            async () => (await failedCards.count()) === 0,
                        );
                        expect(await completedCards.count()).toBe(1);
                        expect(
                            await page.locator('button.chip', { hasText: completedTask.id }).first().isVisible(),
                        ).toBe(true);
                        expect(await page.locator('button.chip', { hasText: failedTask.id }).count()).toBe(0);

                        // Switching back to All restores the unfiltered list.
                        await page.click('.filter-tab:has-text("All")');
                        await waitUntil(
                            'both job cards to return on the All filter',
                            5_000,
                            async () => (await completedCards.count()) === 1 && (await failedCards.count()) === 1,
                        );
                    });
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

    it('streams log entries live while a job is running', async () => {
        const gate = await createGateFixture('lotar-smoke-gate-stream-');
        const agents = await createFixtureAgents(
            {
                slow: {
                    script: gate.script,
                    // The job must still be held when the test asserts its
                    // running state; the default 20 s self-release can pass
                    // first under load. The finally block releases the gate.
                    args: gate.agentArgs({ mode: 'lines', label: 'stream', holdMs: 90_000 }),
                },
            },
            'lotar-smoke-agents-stream-',
        );
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
        });
        let bodyFailed = false;

        try {
            const task = await workspace.addTask('Streaming log test', { args: ['-p', 'UIAG'] });
            const server = await startLotarServer(workspace);

            try {
                await withPage(`${server.url}/agents`, async (page) => {
                    const jobStartedAt = Date.now();
                    await startJobViaApi(server.url, task.id, 'slow', 'Stream the logs slowly');
                    const startReturnedAfterMs = Date.now() - jobStartedAt;
                    await page.waitForSelector('.job-card', { timeout: WAIT_BUDGET_MS });
                    const cardObservedAfterMs = Date.now() - jobStartedAt;
                    await page.click('button:has-text("Show logs")');
                    const logsOpenedAfterMs = Date.now() - jobStartedAt;

                    const liveLogText = await waitForText(page, '.log-panel', 'phase one');
                    const phaseObservedAfterMs = Date.now() - jobStartedAt;
                    expect(liveLogText).toContain('phase one');

                    // The agent is provably mid-run: it is held on the gate,
                    // so the running status is real state rather than a race.
                    try {
                        await waitForJobStatusViaApi(server.url, task.id, 'running');
                    } catch (error) {
                        throw new Error(
                            `${(error as Error).message}; start=${startReturnedAfterMs}ms card=${cardObservedAfterMs}ms logs=${logsOpenedAfterMs}ms phase=${phaseObservedAfterMs}ms; ${gate.diagnostics('stream')}; jobs=${JSON.stringify(await fetchJobsViaApi(server.url, task.id))}`,
                            { cause: error },
                        );
                    }

                    const logSnapshot = await fetch(
                        `${server.url}/api/jobs/logs?id=${encodeURIComponent((await fetchJobsViaApi(server.url, task.id))[0]!.id)}`,
                    );
                    expect(logSnapshot.ok).toBe(true);
                    const events = (await logSnapshot.json()).data.events as Array<{
                        sequence: number;
                        at: string;
                        kind: string;
                        message?: string | null;
                    }>;
                    const sequences = events.map((event) => event.sequence);
                    expect(sequences.every((sequence) => Number.isSafeInteger(sequence) && sequence > 0)).toBe(true);
                    expect(sequences.every((sequence, index) => index === 0 || sequence > sequences[index - 1]!)).toBe(
                        true,
                    );
                    expect(events.every((event) => typeof event.at === 'string' && event.at.length > 0)).toBe(true);
                    expect(events.filter((event) => event.message === 'bash: phase one')).toHaveLength(1);

                    await gate.release();

                    const completedLogText = await waitForText(page, '.log-panel', 'phase one / phase two done');
                    expect(completedLogText).toContain('phase one / phase two done');
                    await waitForJobStatusViaApi(server.url, task.id, 'completed');
                    await page.getByRole('button', { name: 'Hide logs', exact: true }).click();
                    const reopenedSnapshot = page.waitForResponse(
                        (response) => new URL(response.url()).pathname === '/api/jobs/logs',
                    );
                    await page.getByRole('button', { name: 'Show logs', exact: true }).click();
                    expect((await reopenedSnapshot).ok()).toBe(true);
                    await waitForText(page, '.log-panel', 'phase one / phase two done');
                    expect(
                        await page.locator('.log-message').filter({ hasText: 'phase one / phase two done' }).count(),
                    ).toBe(1);
                    expect(
                        await page
                            .locator('.log-message')
                            .filter({ hasText: /^Job started$/ })
                            .count(),
                    ).toBe(1);
                });

                // Worker finalization is awaited before any teardown.
                await waitForJobStatusViaApi(server.url, task.id, 'completed');
            } catch (error) {
                bodyFailed = true;
                throw error;
            } finally {
                await server.stop();
            }
        } finally {
            // Children finish before workspace teardown: release the gate and
            // bounded-await the agent child's completion sentinel.
            const unfinished = await gate.drain(10_000);
            await workspace.dispose();
            await gate.dispose();
            await agents.dispose();
            if (!bodyFailed && unfinished.length > 0) {
                // eslint-disable-next-line no-unsafe-finally -- only throws when the test body passed, so no failure is masked
                throw new Error(`gate children did not finish after release: ${unfinished.join(', ')}`);
            }
        }
    });

    it('groups multiple jobs for the same ticket into one card', { retry: 2 }, async () => {
        const agents = await createFixtureAgents({ mock: {} }, 'lotar-smoke-agents-grouped-');

        try {
            const workspace = await SmokeWorkspace.create({
                seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
            });

            try {
                const task = await workspace.addTask('Grouped jobs test', { args: ['-p', 'UIAG'] });
                const server = await startLotarServer(workspace);

                try {
                    await startJobViaApi(server.url, task.id, 'mock', 'Run first grouped job');
                    expect(await waitForCompletedJobCountViaApi(server.url, task.id, 1)).toBeGreaterThanOrEqual(1);

                    await startJobViaApi(server.url, task.id, 'mock', 'Run second grouped job');
                    expect(await waitForCompletedJobCountViaApi(server.url, task.id, 2)).toBeGreaterThanOrEqual(2);

                    await withPage(`${server.url}/agents`, async (page) => {
                        expect(await (await waitForTicketChip(page, task.id)).count()).toBe(1);
                        expect(await page.locator('.job-card').count()).toBe(2);
                    });
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

    it('starts an agent job when the task panel picker assigns an agent profile found by typing', async () => {
        const agents = await createFixtureAgents(
            { fix: {}, 'fix-light': { description: 'Small fixes on a cheaper model' } },
            'lotar-smoke-agents-panel-assign-',
        );

        try {
            const workspace = await SmokeWorkspace.create({
                seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
            });

            try {
                const task = await workspace.addTask('Assign from panel', { args: ['-p', 'UIAG'] });
                const server = await startLotarServer(workspace);

                try {
                    await withPage(`${server.url}/?project=UIAG&taskPanel=${task.id}`, async (page) => {
                        // DEV-768: agents are grouped by role, base profile first, each with its runner.
                        const picker = '#task-panel-assignee-select';
                        await page.locator(picker).waitFor();
                        await expect
                            .poll(() => assigneePickerValues(page, picker))
                            .toEqual(expect.arrayContaining(['', '@fix', '@fix-light']));
                        const values = await assigneePickerValues(page, picker);
                        expect(values).not.toContain('fix');
                        expect(values.indexOf('@fix')).toBe(values.indexOf('@fix-light') - 1);
                        await page.locator(picker).click();
                        const row = page.locator('[role="option"][data-value="@fix-light"]');
                        expect(await row.locator('.assignee-picker__detail').textContent()).toBe('command');
                        expect(await row.locator('.assignee-picker__description').textContent()).toBe(
                            'Small fixes on a cheaper model',
                        );
                        expect(await page.locator('[role="group"][aria-label="fix"]').count()).toBe(1);
                        await page.getByTestId('assignee-picker-search').press('Escape');

                        await pickAssignee(page, picker, 'light', '@fix-light');
                        expect(await waitForJobCountViaApi(server.url, task.id, 1)).toBeGreaterThanOrEqual(1);
                    });
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

    it.skipIf(!gitAvailable())(
        'advances the reviewed template workflow live without reloads',
        async () => {
            const agents = await createFixtureAgents({ mock: {} }, 'lotar-smoke-agents-reviewed-');
            const worktreeDir = await mkdtemp(path.join(tmpdir(), 'lotar-smoke-worktrees-'));

            try {
                const workspace = await SmokeWorkspace.create();

                try {
                    await workspace.initGit({ name: 'tester', email: 'tester@example.com' });
                    await writeReviewedTemplateWorkflow(workspace, agents.command('mock'), worktreeDir);

                    const taskTitle = 'UI reviewed workflow';
                    const server = await startLotarServer(workspace);

                    try {
                        await withPage(`${server.url}/`, async (tasksPage) => {
                            const taskId = await createTaskViaApi(server.url, taskTitle, 'UIAG', 'tester');
                            await tasksPage.waitForSelector(`text=${taskTitle}`, { timeout: 15_000 });
                            await updateTaskViaApi(server.url, { id: taskId, assignee: '@implement' });
                            const reviewRowText = await waitForTaskRowText(tasksPage, taskTitle, 'Review');
                            expect(reviewRowText).toContain(taskTitle);
                            expect(reviewRowText).toContain('ready-for-review');

                            expect(await waitForJobCountViaApi(server.url, taskId, 2)).toBeGreaterThanOrEqual(2);

                            // The mock agent writes nothing: the ticket's work lands on main
                            // here, or the merge fails as nothing was built (DEV-680).
                            await workspace.runGit(['commit', '-q', '--allow-empty', '-m', `${taskId} work`]);
                            await updateTaskViaApi(server.url, { id: taskId, assignee: '@merge' });
                            const doneRowText = await waitForTaskRowText(tasksPage, taskTitle, 'Done');
                            expect(doneRowText).toContain(taskTitle);
                            expect(await waitForJobCountViaApi(server.url, taskId, 3)).toBeGreaterThanOrEqual(3);
                        });
                    } finally {
                        await server.stop();
                    }
                } finally {
                    await workspace.dispose();
                }
            } finally {
                await agents.dispose();
                await fs.remove(worktreeDir);
            }
        },
        2 * JOB_END_LIMIT_MS + 60_000,
    );

    // Both scenarios below wait for an agent to start and then walk the UI:
    // under full-suite load (load average 46) that outlasts the shared 120 s
    // cap and the 15-20 s waits, though nothing hangs (DEV-775).
    it('stops queued and running jobs only after accessible confirmation', { timeout: 300_000 }, async ({ expect }) => {
        const gate = await createGateFixture('lotar-smoke-gate-stopall-');
        const agents = await createFixtureAgents(
            {
                slow: {
                    script: gate.script,
                    // Held past the 20 s default: a child that exits on its own
                    // completes its job before Stop all, which then never
                    // cancels every job. The `finally` releases it (DEV-775).
                    args: gate.agentArgs({ mode: 'hold', label: 'stop-all', holdMs: JOB_END_LIMIT_MS }),
                },
            },
            'lotar-smoke-agents-stopall-',
        );
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': configWithAgents(agents.yaml, { maxParallelJobs: 1 }),
            },
        });

        try {
            const first = await workspace.addTask('Stop all one', { args: ['-p', 'UIAG'] });
            const second = await workspace.addTask('Stop all two', { args: ['-p', 'UIAG'] });
            const server = await startLotarServer(workspace);

            try {
                for (const ticketId of [first.id, second.id]) {
                    await startJobViaApi(server.url, ticketId, 'slow', 'Run until cancelled');
                }

                // Deterministic precondition: with max_parallel_jobs 1 exactly
                // one job runs while the other stays queued.
                await waitUntil('exactly one running and one queued job', WAIT_BUDGET_MS, async () => {
                    const jobs = await fetchJobsViaApi(server.url);
                    return (
                        jobs.filter((job) => job.status === 'running').length === 1 &&
                        jobs.filter((job) => job.status === 'queued').length === 1
                    );
                });

                await withPage(`${server.url}/agents`, async (page) => {
                    let cancelRequests = 0;
                    page.on('request', (request) => {
                        if (request.method() === 'POST' && new URL(request.url()).pathname === '/api/jobs/cancel-all')
                            cancelRequests += 1;
                    });
                    const trigger = page.getByRole('button', { name: 'Stop all', exact: true });
                    for (const viewport of [
                        { width: 1280, height: 900 },
                        { width: 390, height: 844 },
                    ]) {
                        await page.setViewportSize(viewport);
                        await trigger.click();
                        const dialog = page.getByRole('dialog', { name: 'Stop all agent jobs', exact: true });
                        await dialog.waitFor({ state: 'visible' });
                        expect(
                            await dialog
                                .getByRole('button', { name: 'Cancel', exact: true })
                                .evaluate((el) => el === document.activeElement),
                        ).toBe(true);
                        expect(await dialog.ariaSnapshot()).toContain('Stop all queued and running agent jobs?');
                        await page.keyboard.press('Shift+Tab');
                        expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
                        await page.keyboard.press('Tab');
                        await page.keyboard.press('Enter');
                        await dialog.waitFor({ state: 'detached' });
                        expect(cancelRequests).toBe(0);
                        expect(await trigger.evaluate((el) => el === document.activeElement)).toBe(true);

                        await trigger.click();
                        await dialog.waitFor({ state: 'visible' });
                        await page.keyboard.press('Escape');
                        await dialog.waitFor({ state: 'detached' });
                        expect(cancelRequests).toBe(0);
                        await trigger.click();
                        await dialog.waitFor({ state: 'visible' });
                        await page.mouse.click(4, 4);
                        await dialog.waitFor({ state: 'detached' });
                        expect(cancelRequests).toBe(0);
                    }

                    await trigger.click();
                    await page
                        .getByRole('dialog', { name: 'Stop all agent jobs', exact: true })
                        .getByRole('button', { name: 'Stop all', exact: true })
                        .click();

                    await waitUntil('stop-all to cancel every job', WAIT_BUDGET_MS, async () => {
                        const jobs = await fetchJobsViaApi(server.url);
                        return jobs.length === 2 && jobs.every((job) => job.status === 'cancelled');
                    });
                    expect(cancelRequests).toBe(1);
                });
            } finally {
                await server.stop();
            }
        } finally {
            // Cancellation kills the running child (job status `cancelled` is
            // the awaited finalization); releasing the gate is a bounded
            // safety net so a surviving child exits before workspace teardown.
            await gate.dispose();
            await workspace.dispose();
            await agents.dispose();
        }
    });

    it("shows the agent's permission question in the job view and answers it", { timeout: 300_000 }, async () => {
        // The claude runner is the harnessmux claude agent; its agents.toml
        // under HARNESSMUX_HOME runs the fake ACP agent instead (DEV-503).
        const harnessmuxHome = await mkdtemp(path.join(tmpdir(), 'lotar-smoke-harnessmux-'));
        await fs.outputFile(
            path.join(harnessmuxHome, 'config', 'agents.toml'),
            `[agents.claude]\ncommand = [${JSON.stringify(nodeExecutable())}, ${JSON.stringify(ACP_AGENT_SCRIPT)}]\n`,
        );
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': configWithAgents(
                    'agents:\n  asker:\n    runner: claude\n    on_permission: ask\n    env:\n      FAKE_ASK: "1"',
                ),
            },
        });
        try {
            const task = await workspace.addTask('Permission question', { args: ['-p', 'UIAG'] });
            const server = await startLotarServer(workspace, { env: { HARNESSMUX_HOME: harnessmuxHome } });
            try {
                await withPage(`${server.url}/agents`, async (page) => {
                    await startJobViaApi(server.url, task.id, 'asker', 'Edit the notes');
                    const question = page.locator('.job-question');
                    await question.waitFor({ timeout: WAIT_BUDGET_MS });
                    expect(await question.textContent()).toContain('The agent asks: Edit notes.md');
                    await question.getByRole('button', { name: 'Allow', exact: true }).click();
                    await waitForJobStatusViaApi(server.url, task.id, 'completed');
                    await page.locator('.job-question').waitFor({ state: 'detached', timeout: WAIT_BUDGET_MS });
                    await waitForText(page, '.job-card', 'Permission: allow');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await fs.remove(harnessmuxHome);
        }
    });

    it('shows a job another process runs live from its log, without reload', async () => {
        // DEV-765: the queue worker, `agent run --wait` and MCP sessions only
        // append to the job log; `lotar serve` streams the new lines.
        const agents = await createFixtureAgents({ mock: {} }, 'lotar-smoke-agents-elsewhere-');
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
        });
        try {
            const task = await workspace.addTask('Job of another process', { args: ['-p', 'UIAG'] });
            const server = await startLotarServer(workspace);
            const log = path.join(workspace.root, '.logs', 'job-elsewhere-1.jsonl');
            const at = new Date().toISOString();
            const line = (value: Record<string, unknown>) => `${JSON.stringify(value)}\n`;
            const event = (kind: string, sequence: number, message?: string) =>
                line({ type: 'event', kind, at, sequence, ...(message ? { message } : {}) });
            try {
                await withPage(`${server.url}/agents`, async (page) => {
                    await page.waitForSelector('.queue-stats-card', { timeout: 15_000 });
                    await fs.outputFile(
                        log,
                        line({
                            type: 'header',
                            job_id: 'job-elsewhere-1',
                            ticket_id: task.id,
                            runner: 'command',
                            created_at: at,
                            agent_profile: 'mock',
                        }) +
                            event('agent_job_started', 1) +
                            event('agent_job_message', 2, 'Reading the ticket'),
                    );
                    await waitForTicketChip(page, task.id);
                    await page.click('button:has-text("Show logs")');
                    await waitForText(page, '.log-panel', 'Reading the ticket');

                    // Lines appended once the panel shows the log arrive only live.
                    await fs.appendFile(log, event('agent_job_progress', 3, 'Bash: npm test'));
                    await waitForText(page, '.log-panel', 'npm test');
                    await fs.appendFile(
                        log,
                        line({ type: 'status', status: 'completed', at, exit_code: 0 }) +
                            event('agent_job_completed', 4, 'Finished elsewhere'),
                    );
                    await waitForText(page, '.log-panel', 'Finished elsewhere');
                    await waitForText(page, '.job-card', 'completed');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await agents.dispose();
        }
    });

    it('scrolls the log to a search match deep in a long tool output (DEV-880)', async ({ expect }) => {
        const agents = await createFixtureAgents({ mock: {} }, 'lotar-smoke-agents-log-seek-');
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
        });
        try {
            const task = await workspace.addTask('Job log seek', { args: ['-p', 'UIAG'] });
            const server = await startLotarServer(workspace);
            const log = path.join(workspace.root, '.logs', 'job-log-seek-1.jsonl');
            const at = new Date().toISOString();
            const line = (value: Record<string, unknown>) => `${JSON.stringify(value)}\n`;
            const event = (kind: string, sequence: number, fields: Record<string, unknown> = {}) =>
                line({ type: 'event', kind, at, sequence, ...fields });
            const long = [...Array.from({ length: 40 }, (_, index) => `line ${index}`), 'npm notice'].join('\n');
            const messages = Array.from({ length: 20 }, (_, index) =>
                event('agent_job_message', 2 + index, { message: `Step ${index} done.` }),
            ).join('');
            try {
                await fs.outputFile(
                    log,
                    line({
                        type: 'header',
                        job_id: 'job-log-seek-1',
                        ticket_id: task.id,
                        runner: 'claude',
                        created_at: at,
                        agent_profile: 'mock',
                    }) +
                        line({ type: 'status', status: 'completed', at }) +
                        event('agent_job_started', 1) +
                        messages +
                        event('agent_job_tool', 30, {
                            message: 'Terminal',
                            tool: {
                                id: 'a',
                                kind: 'execute',
                                title: 'Terminal',
                                status: 'completed',
                                input: 'make test',
                                started_at: at,
                                output: long,
                            },
                        }) +
                        event('agent_job_message', 31, { message: 'All green.' }),
                );
                await withPage(`${server.url}/agents?job=job-log-seek-1`, async (page) => {
                    await page.setViewportSize({ width: 1440, height: 900 });
                    await waitForText(page, '.log-panel', 'All green.');
                    const panel = page.locator('.log-panel');
                    const list = panel.locator('.log-list');
                    // Scroll to the top, so the match starts below the list's visible part.
                    await list.evaluate((el) => (el.scrollTop = 0));

                    await panel.getByRole('searchbox', { name: 'Search the log' }).fill('notice');
                    await expect.poll(() => panel.locator('.job-log__matches').textContent()).toBe('1 match');
                    // The highlighted word lies inside the list's visible part.
                    await expect
                        .poll(() =>
                            list.evaluate((el) => {
                                const range = [...(CSS.highlights.get('job-log-search') ?? [])][0];
                                if (!(range instanceof Range)) return 'no highlight';
                                const box = el.getBoundingClientRect();
                                const rect = range.getBoundingClientRect();
                                return el.scrollTop > 0 && rect.top >= box.top && rect.bottom <= box.bottom
                                    ? 'visible'
                                    : `hidden at ${rect.top} in ${box.top}-${box.bottom}, scrolled ${el.scrollTop}`;
                            }),
                        )
                        .toBe('visible');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await agents.dispose();
        }
    });

    it('filters the log, folds its tool calls and shows what a running job does (DEV-769)', async ({ expect }) => {
        const agents = await createFixtureAgents({ mock: {} }, 'lotar-smoke-agents-log-view-');
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': configWithAgents(agents.yaml) },
        });
        try {
            const task = await workspace.addTask('Job log views', { args: ['-p', 'UIAG'] });
            const server = await startLotarServer(workspace);
            const log = path.join(workspace.root, '.logs', 'job-log-view-1.jsonl');
            const at = new Date().toISOString();
            const line = (value: Record<string, unknown>) => `${JSON.stringify(value)}\n`;
            const event = (kind: string, sequence: number, fields: Record<string, unknown> = {}) =>
                line({ type: 'event', kind, at, sequence, ...fields });
            const tool = (sequence: number, id: string, status: string, input: string, output?: string) =>
                event('agent_job_tool', sequence, {
                    message: 'Terminal',
                    tool: { id, kind: 'execute', title: 'Terminal', status, input, started_at: at, output },
                });
            try {
                await fs.outputFile(
                    log,
                    line({
                        type: 'header',
                        job_id: 'job-log-view-1',
                        ticket_id: task.id,
                        runner: 'claude',
                        created_at: at,
                        agent_profile: 'mock',
                    }) +
                        line({ type: 'status', status: 'running', at }) +
                        event('agent_job_started', 1) +
                        event('agent_job_progress', 2, { message: 'Session lotar-x (`harnessmux attach lotar-x`).' }) +
                        event('agent_job_progress', 3, { message: 'Wrote .claude/settings.local.json in /tmp/x' }) +
                        tool(4, 'a', 'completed', 'git status', 'nothing to commit') +
                        tool(5, 'b', 'completed', 'cargo build') +
                        tool(6, 'c', 'failed', 'npm test', 'parser test FAILED') +
                        event('agent_job_message', 7, { message: 'One test fails; fixing the parser.' }) +
                        tool(8, 'd', 'running', 'npm run smoke'),
                );
                await withPage(`${server.url}/agents?job=job-log-view-1`, async (page) => {
                    // The link opens the job's log.
                    await waitForText(page, '.log-panel', 'One test fails');
                    const panel = page.locator('.log-panel');
                    await expect
                        .poll(() => panel.locator('.log-group--tools .log-group__text').first().textContent())
                        .toBe('3 commands · 1 failed');
                    await expect.poll(() => panel.locator('.log-group--setup').count()).toBe(1);
                    await expect
                        .poll(() => panel.getByTestId('job-log-live').textContent())
                        .toContain('Running npm run smoke');

                    await panel.getByRole('button', { name: /^Problems/ }).click();
                    await expect.poll(() => panel.locator('.tool-call__what').allTextContents()).toEqual(['npm test']);
                    await expect.poll(() => panel.locator('.tool-call__pre').textContent()).toBe('parser test FAILED');

                    await panel.getByRole('button', { name: /^Conversation/ }).click();
                    await expect.poll(() => panel.locator('.tool-call').count()).toBe(0);
                    await expect.poll(() => panel.locator('.log-list').textContent()).toContain('fixing the parser');

                    await panel.getByRole('button', { name: /^All/ }).click();
                    await panel.getByRole('searchbox', { name: 'Search the log' }).fill('cargo');
                    await expect
                        .poll(() => panel.locator('.tool-call__what').allTextContents())
                        .toEqual(['cargo build']);

                    // A word only in a call's output opens that call and highlights it (DEV-859).
                    await panel.getByRole('searchbox', { name: 'Search the log' }).fill('commit');
                    await expect
                        .poll(() => panel.locator('.tool-call__what').allTextContents())
                        .toEqual(['git status']);
                    const output = panel.locator('.tool-call__pre', { hasText: 'nothing to commit' });
                    await expect.poll(() => output.isVisible()).toBe(true);
                    await expect
                        .poll(() =>
                            page.evaluate(() =>
                                [...(CSS.highlights.get('job-log-search') ?? [])].map((range) =>
                                    range instanceof Range ? range.toString() : '',
                                ),
                            ),
                        )
                        .toEqual(['commit']);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
            await agents.dispose();
        }
    });
});
