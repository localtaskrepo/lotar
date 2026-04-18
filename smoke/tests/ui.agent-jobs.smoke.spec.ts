import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const mockAgentPath = path.resolve(__dirname, '../helpers/mock-agent.sh');
const reviewedTemplatePath = path.resolve(__dirname, '../../src/config/templates/agent-reviewed.yml');

const AGENT_CONFIG = `default:
  project: UIAG
issue:
  states: [Todo, InProgress, Done]
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
    description: Mock agent for UI tests
`;

async function writeReviewedTemplateWorkflow(workspace: SmokeWorkspace, agentPath: string) {
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
        agent.command = agentPath;
    }

    await workspace.write('.tasks/UIAG/config.yml', stringify(config));
    await workspace.write('.tasks/automation.yml', stringify({ automation: template.automation }));
}

async function writeExecutableScript(
    workspace: SmokeWorkspace,
    relativePath: string,
    contents: string,
): Promise<string> {
    await workspace.write(relativePath, contents);
    const absolutePath = path.join(workspace.root, relativePath);
    await fs.chmod(absolutePath, 0o755);
    return absolutePath;
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

async function waitForJobCards(page: import('@playwright/test').Page, taskId: string, expected: number) {
    const ticketGroup = page.locator('.ticket-group-card', { hasText: taskId }).first();
    for (let attempt = 0; attempt < 80; attempt += 1) {
        const count = await ticketGroup.locator('.job-card').count();
        if (count >= expected) {
            return count;
        }
        await page.waitForTimeout(250);
    }
    throw new Error(`Timed out waiting for ${expected} job cards for ${taskId}`);
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
        const response = await fetch(`${serverUrl}/api/jobs?ticket_id=${ticketId}`);
        const payload = (await response.json()) as {
            data?: { jobs?: Array<{ id?: string; status?: string }> };
        };
        const count = payload.data?.jobs?.length ?? 0;
        if (count >= expected) {
            return count;
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`Timed out waiting for ${expected} jobs for ${ticketId}`);
}

async function waitForCompletedJobCountViaApi(serverUrl: string, ticketId: string, expected: number) {
    for (let attempt = 0; attempt < 80; attempt += 1) {
        const response = await fetch(`${serverUrl}/api/jobs?ticket_id=${ticketId}`);
        const payload = (await response.json()) as {
            data?: { jobs?: Array<{ id?: string; status?: string }> };
        };
        const jobs = payload.data?.jobs ?? [];
        if (jobs.length >= expected && jobs.every((job) => job.status === 'completed')) {
            return jobs.length;
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`Timed out waiting for ${expected} completed jobs for ${ticketId}`);
}

describe.concurrent('UI Agent Jobs page smoke tests', () => {
    it('renders page heading and queue stats', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': AGENT_CONFIG },
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
    });

    it('shows a completed job in the job list', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': AGENT_CONFIG },
        });

        try {
            const task = await workspace.addTask('UI job test');
            const server = await startLotarServer(workspace);
            try {
                // Start a mock agent job via API
                const createRes = await fetch(`${server.url}/api/jobs`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        ticket_id: task.id,
                        agent: 'mock',
                        prompt: 'Test job for UI',
                    }),
                });
                expect(createRes.ok).toBe(true);

                // Wait for job to complete
                let jobCompleted = false;
                for (let i = 0; i < 20; i++) {
                    await new Promise((r) => setTimeout(r, 500));
                    const listRes = await fetch(`${server.url}/api/jobs`);
                    const list = (await listRes.json()) as {
                        data: { jobs: Array<{ status: string }> };
                    };
                    if (list.data.jobs.some((j) => j.status === 'completed')) {
                        jobCompleted = true;
                        break;
                    }
                }
                expect(jobCompleted).toBe(true);

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
    });

    it('filter tabs are present and clickable', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': AGENT_CONFIG },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/agents`, async (page) => {
                    await page.waitForSelector('.filter-tab', { timeout: 15_000 });

                    const tabs = await page.$$eval('.filter-tab', (els) =>
                        els.map((el) => el.textContent?.trim()),
                    );
                    expect(tabs.some((t) => t?.includes('All'))).toBe(true);
                    expect(tabs.some((t) => t?.includes('Running'))).toBe(true);
                    expect(tabs.some((t) => t?.includes('Completed'))).toBe(true);
                    expect(tabs.some((t) => t?.includes('Failed'))).toBe(true);

                    // Click a tab — no errors
                    await page.click('.filter-tab:has-text("Completed")');
                    await page.waitForTimeout(300);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('streams log entries live while a job is running', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': AGENT_CONFIG },
        });

        try {
            const slowAgentPath = await writeExecutableScript(
                workspace,
                'slow-copilot-agent.sh',
                `#!/bin/sh
echo '{"type":"system","subtype":"init","session_id":"slow-copilot"}'
echo '{"type":"message","delta":true,"content":"phase one","session_id":"slow-copilot"}'
sleep 2
echo '{"type":"message","delta":true,"content":" / phase two","session_id":"slow-copilot"}'
sleep 2
echo '{"type":"message","content":"phase one / phase two done","session_id":"slow-copilot"}'
echo '{"type":"result","result":"Completed successfully","session_id":"slow-copilot"}'
exit 0
`,
            );

            await workspace.write(
                '.tasks/config.yml',
                `default:
  project: UIAG
issue:
  states: [Todo, InProgress, Done]
  priorities: [Low, Medium, High]
  types: [Feature, Bug]

agent:
  logs_dir: .logs
  worktree:
    enabled: false

agents:
  slow:
    runner: copilot
    command: "${slowAgentPath}"
`,
            );

            const task = await workspace.addTask('Streaming log test', { args: ['-p', 'UIAG'] });
            const server = await startLotarServer(workspace);

            try {
                await withPage(`${server.url}/agents`, async (page) => {
                    await startJobViaApi(server.url, task.id, 'slow', 'Stream the logs slowly');
                    await page.waitForSelector('.job-card', { timeout: 15_000 });
                    await page.click('button:has-text("Show logs")');

                    const liveLogText = await waitForText(page, '.log-panel', 'phase one');
                    expect(liveLogText).toContain('phase one');

                    const completedLogText = await waitForText(page, '.log-panel', 'phase one / phase two done');
                    expect(completedLogText).toContain('phase one / phase two done');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('groups multiple jobs for the same ticket into one card', { retry: 2 }, async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': AGENT_CONFIG },
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
    });

    it('advances the reviewed template workflow live without reloads', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.initGit({ name: 'tester', email: 'tester@example.com' });
            await writeReviewedTemplateWorkflow(workspace, mockAgentPath);

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
    });

    it('stops queued and running jobs from the Agents page', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const slowAgentPath = await writeExecutableScript(
                workspace,
                'slow-agent.sh',
                `#!/bin/sh
echo '{"type":"system","subtype":"init","session_id":"slow-agent"}'
sleep 5
echo '{"type":"message","content":"slow agent done","session_id":"slow-agent"}'
echo '{"type":"result","result":"Completed successfully","session_id":"slow-agent"}'
exit 0
`,
            );

            await workspace.write(
                '.tasks/config.yml',
                `default:
  project: UIAG
issue:
  states: [Todo, InProgress, Done]
  priorities: [Low, Medium, High]
  types: [Feature, Bug]

agent:
  logs_dir: .logs
  worktree:
    enabled: false
    max_parallel_jobs: 1

agents:
  slow:
    runner: copilot
    command: "${slowAgentPath}"
`,
            );

            const first = await workspace.addTask('Stop all one', { args: ['-p', 'UIAG'] });
            const second = await workspace.addTask('Stop all two', { args: ['-p', 'UIAG'] });
            const server = await startLotarServer(workspace);

            try {
                for (const ticketId of [first.id, second.id]) {
                    const createRes = await fetch(`${server.url}/api/jobs`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({
                            ticket_id: ticketId,
                            agent: 'slow',
                            prompt: 'Run until cancelled',
                        }),
                    });
                    expect(createRes.ok).toBe(true);
                }

                await withPage(`${server.url}/agents`, async (page) => {
                    page.on('dialog', (dialog) => dialog.accept());

                    for (let attempt = 0; attempt < 40; attempt += 1) {
                        const text = await page.textContent('body');
                        if (text?.includes('running') && text?.includes('queued')) {
                            break;
                        }
                        await page.waitForTimeout(250);
                    }

                    await page.click('button:has-text("Stop all")');

                    for (let attempt = 0; attempt < 60; attempt += 1) {
                        const listRes = await fetch(`${server.url}/api/jobs`);
                        const payload = (await listRes.json()) as {
                            data: { jobs: Array<{ status: string }> };
                        };
                        if (payload.data.jobs.every((job) => job.status === 'cancelled')) {
                            return;
                        }
                        await page.waitForTimeout(250);
                    }

                    throw new Error('Timed out waiting for stop-all to cancel every job');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
