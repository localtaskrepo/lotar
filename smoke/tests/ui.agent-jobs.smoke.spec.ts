import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { createFixtureAgents, createGateFixture } from '../helpers/agent-fixtures.js';
import { gitAvailable } from '../helpers/git.js';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const reviewedTemplatePath = path.resolve(__dirname, '../../src/config/templates/agent-reviewed.yml');

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

async function writeReviewedTemplateWorkflow(workspace: SmokeWorkspace, mockLauncherCommand: string) {
    const template = parse(await fs.readFile(reviewedTemplatePath, 'utf8')) as {
        config: Record<string, unknown>;
        automation: Record<string, unknown>;
    };

    const config = template.config as {
        project?: { name?: string };
        agents?: Record<string, Record<string, unknown>>;
    };
    if (config.project) {
        config.project.name = 'UI Reviewed';
    }
    for (const agentName of ['implement', 'test', 'merge', 'merge-retry']) {
        const agent = config.agents?.[agentName];
        if (!agent) {
            continue;
        }
        agent.runner = 'copilot';
        agent.command = mockLauncherCommand;
        agent.args = [];
    }

    await workspace.write('.tasks/UIAG/config.yml', stringify(config));
    await workspace.write('.tasks/automation.yml', stringify({ automation: template.automation }));
}

async function waitUntil(
    description: string,
    timeoutMs: number,
    condition: () => Promise<boolean>,
): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (await condition()) {
            return;
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`Timed out after ${timeoutMs}ms waiting for ${description}`);
}

async function waitForTaskRowText(page: import('@playwright/test').Page, taskLabel: string, expected: string) {
    const row = page.locator('tr', { hasText: taskLabel }).first();
    for (let attempt = 0; attempt < 80; attempt += 1) {
        const text = await row.textContent();
        if (text?.includes(expected)) {
            return text;
        }
        await page.waitForTimeout(250);
    }
    throw new Error(`Timed out waiting for task ${taskLabel} to include ${expected}`);
}

async function waitForTicketChip(page: import('@playwright/test').Page, taskId: string) {
    const ticketChip = page.locator('button.chip', { hasText: taskId }).first();
    await ticketChip.waitFor({ state: 'visible', timeout: 20_000 });
    return ticketChip;
}

async function waitForText(page: import('@playwright/test').Page, selector: string, expected: string) {
    for (let attempt = 0; attempt < 80; attempt += 1) {
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
    for (let attempt = 0; attempt < 80; attempt += 1) {
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
    const taskId = 'task' in (payload.data ?? {})
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
    for (let attempt = 0; attempt < 80; attempt += 1) {
        const jobs = await fetchJobsViaApi(serverUrl, ticketId);
        if (jobs.length >= expected) {
            return jobs.length;
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`Timed out waiting for ${expected} jobs for ${ticketId}`);
}

async function waitForCompletedJobCountViaApi(serverUrl: string, ticketId: string, expected: number) {
    for (let attempt = 0; attempt < 80; attempt += 1) {
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
                        expect(heading).toContain('Agent jobs');

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

    it('shows a completed job in the job list', async () => {
        const agents = await createFixtureAgents(
            { mock: {} },
            'lotar-smoke-agents-completed-',
        );

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

                        const tabs = await page.$$eval('.filter-tab', (els) =>
                            els.map((el) => el.textContent?.trim()),
                        );
                        expect(tabs.some((t) => t?.includes('All'))).toBe(true);
                        expect(tabs.some((t) => t?.includes('Running'))).toBe(true);
                        expect(tabs.some((t) => t?.includes('Completed'))).toBe(true);
                        expect(tabs.some((t) => t?.includes('Failed'))).toBe(true);

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
                    args: gate.agentArgs({ mode: 'copilot', label: 'stream' }),
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
                    await startJobViaApi(server.url, task.id, 'slow', 'Stream the logs slowly');
                    await page.waitForSelector('.job-card', { timeout: 15_000 });
                    await page.click('button:has-text("Show logs")');

                    const liveLogText = await waitForText(page, '.log-panel', 'phase one');
                    expect(liveLogText).toContain('phase one');

                    // The agent is provably mid-run: it is held on the gate,
                    // so the running status is real state rather than a race.
                    await waitForJobStatusViaApi(server.url, task.id, 'running');

                    await gate.release();

                    const completedLogText = await waitForText(page, '.log-panel', 'phase one / phase two done');
                    expect(completedLogText).toContain('phase one / phase two done');
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
                throw new Error(`gate children did not finish after release: ${unfinished.join(', ')}`);
            }
        }
    });

    it('groups multiple jobs for the same ticket into one card', { retry: 2 }, async () => {
        const agents = await createFixtureAgents(
            { mock: {} },
            'lotar-smoke-agents-grouped-',
        );

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

    it.skipIf(!gitAvailable())('advances the reviewed template workflow live without reloads', async () => {
        const agents = await createFixtureAgents(
            { mock: {} },
            'lotar-smoke-agents-reviewed-',
        );

        try {
            const workspace = await SmokeWorkspace.create();

            try {
                await workspace.initGit({ name: 'tester', email: 'tester@example.com' });
                await writeReviewedTemplateWorkflow(workspace, agents.command('mock'));

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
        }
    });

    it('stops queued and running jobs only after accessible confirmation', async ({ expect }) => {
        const gate = await createGateFixture('lotar-smoke-gate-stopall-');
        const agents = await createFixtureAgents(
            {
                slow: {
                    script: gate.script,
                    args: gate.agentArgs({ mode: 'hold', label: 'stop-all' }),
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
                await waitUntil(
                    'exactly one running and one queued job',
                    20_000,
                    async () => {
                        const jobs = await fetchJobsViaApi(server.url);
                        return (
                            jobs.filter((job) => job.status === 'running').length === 1 &&
                            jobs.filter((job) => job.status === 'queued').length === 1
                        );
                    },
                );

                await withPage(`${server.url}/agents`, async (page) => {
                    let cancelRequests = 0;
                    page.on('request', request => {
                        if (request.method() === 'POST' && new URL(request.url()).pathname === '/api/jobs/cancel-all') cancelRequests += 1;
                    });
                    const trigger = page.getByRole('button', { name: 'Stop all', exact: true });
                    for (const viewport of [{ width: 1280, height: 900 }, { width: 390, height: 844 }]) {
                        await page.setViewportSize(viewport);
                        await trigger.click();
                        const dialog = page.getByRole('dialog', { name: 'Stop all agent jobs', exact: true });
                        await dialog.waitFor({ state: 'visible' });
                        expect(await dialog.getByRole('button', { name: 'Cancel', exact: true }).evaluate(el => el === document.activeElement)).toBe(true);
                        expect(await dialog.ariaSnapshot()).toContain('Stop all queued and running agent jobs?');
                        await page.keyboard.press('Shift+Tab');
                        expect(await dialog.evaluate(el => el.contains(document.activeElement))).toBe(true);
                        await page.keyboard.press('Tab');
                        await page.keyboard.press('Enter');
                        await dialog.waitFor({ state: 'detached' });
                        expect(cancelRequests).toBe(0);
                        expect(await trigger.evaluate(el => el === document.activeElement)).toBe(true);

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
                    await page.getByRole('dialog', { name: 'Stop all agent jobs', exact: true })
                        .getByRole('button', { name: 'Stop all', exact: true }).click();

                    await waitUntil(
                        'stop-all to cancel every job',
                        20_000,
                        async () => {
                            const jobs = await fetchJobsViaApi(server.url);
                            return jobs.length === 2 && jobs.every((job) => job.status === 'cancelled');
                        },
                    );
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
});
