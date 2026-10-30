import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { createFixtureAgents, createGateFixture } from '../helpers/agent-fixtures.js';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-249: a task an agent is working on shows an indicator on the task list,
// boards and in the task panel; hovering says what the agent reported last.

describe.concurrent('UI agent activity indicator', () => {
    it('shows while a job runs and disappears when it finishes', async ({ expect }) => {
        const gate = await createGateFixture('lotar-smoke-gate-activity-');
        const agents = await createFixtureAgents(
            { worker: { script: gate.script, args: gate.agentArgs({ mode: 'copilot', label: 'activity' }) } },
            'lotar-smoke-agents-activity-',
        );
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': `default:\n  project: ACT\nissue:\n  states: [Todo, InProgress, Done]\nagent:\n  logs_dir: .logs\n  worktree:\n    enabled: false\n\n${agents.yaml}\n`,
            },
        });
        try {
            const task = await workspace.addTask('Agent activity target', { args: ['-p', 'ACT'] });
            await workspace.addTask('Quiet task', { args: ['-p', 'ACT'] });
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=ACT`, async (page) => {
                    const row = page.locator('tr', { hasText: 'Agent activity target' });
                    await row.waitFor({ timeout: 20_000 });
                    expect(await page.getByTestId('agent-activity').count()).toBe(0);

                    const response = await fetch(`${server.url}/api/jobs`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ ticket_id: task.id, agent: 'worker', prompt: 'Work slowly' }),
                    });
                    expect(response.ok).toBe(true);
                    await gate.waitForStarted('activity', 20_000);

                    const indicator = row.getByTestId('agent-activity');
                    await indicator.waitFor({ timeout: 15_000 });
                    await expect
                        .poll(async () => indicator.getAttribute('title'))
                        .toMatch(/^worker is working on this/);
                    expect(
                        await page.locator('tr', { hasText: 'Quiet task' }).getByTestId('agent-activity').count(),
                    ).toBe(0);

                    const artifacts = path.resolve('target/smoke-artifacts/agent-activity');
                    await fs.ensureDir(artifacts);
                    await row.screenshot({ path: path.join(artifacts, 'task-row.png') });
                    await page.goto(`${server.url}/boards?project=ACT`);
                    await page
                        .locator('[data-board-card]', { hasText: 'Agent activity target' })
                        .getByTestId('agent-activity')
                        .waitFor({
                            timeout: 15_000,
                        });

                    await page.goto(`${server.url}/?project=ACT&taskPanel=${task.id}`);
                    const panelIndicator = page.locator('.task-panel__title').getByTestId('agent-activity');
                    await panelIndicator.waitFor({ timeout: 15_000 });
                    expect(await panelIndicator.textContent()).toContain('Working');
                    await page
                        .locator('.task-panel__header')
                        .screenshot({ path: path.join(artifacts, 'panel-header.png') });

                    await gate.release();
                    await gate.waitForCompleted('activity', 20_000);
                    await panelIndicator.waitFor({ state: 'detached', timeout: 15_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await gate.drain(10_000);
            await workspace.dispose();
            await gate.dispose();
            await agents.dispose();
        }
    });
});
