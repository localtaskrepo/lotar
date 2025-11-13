import fs from 'fs-extra';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
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

const AUTO_POPULATE_CONFIG = `default:
    project: CLI
    reporter: allowed@example.com
    assignee: allowed@example.com
    members:
        - allowed@example.com
    strict_members: false
issue:
    states: [Todo, InProgress, Done]
    types: [Feature, Bug, Chore]
    priorities: [Low, Medium, High]
`;

describe('UI strict member smoke scenarios', () => {
    it('surfaces member suggestions and enforces strict membership', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', STRICT_MEMBERS_CONFIG);

            await workspace.addTask('Seed task for allowed member', {
                args: ['--reporter=allowed@example.com', '--assignee=allowed@example.com'],
            });
            await workspace.addTask('Seed task for reviewer member', {
                args: ['--reporter=allowed@example.com', '--assignee=reviewer@example.com'],
            });

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.waitForSelector('text=Tasks', { timeout: 15_000 });

                    const addButton = page.getByRole('button', { name: 'Add' });
                    await addButton.waitFor({ state: 'visible' });
                    await addButton.click();

                    const panel = page.locator('[aria-label="Create task"]');
                    await panel.waitFor({ state: 'visible', timeout: 10_000 });

                    const summarySelects = panel.locator('select');
                    const projectSelect = summarySelects.nth(0);
                    await projectSelect.waitFor({ state: 'attached', timeout: 10_000 });
                    const projectOptions = await projectSelect.locator('option').allTextContents();
                    expect(projectOptions.length).toBeGreaterThan(0);
                    const enabledProjectOption = projectSelect
                        .locator('option:not([disabled]):not([value=""])')
                        .first();
                    await enabledProjectOption.waitFor({ state: 'attached', timeout: 10_000 });
                    const projectValue = await enabledProjectOption.getAttribute('value');
                    expect(projectValue).toBeTruthy();
                    await projectSelect.selectOption(projectValue ?? '');

                    await summarySelects.nth(1).selectOption({ label: 'Feature' });
                    await summarySelects.nth(2).selectOption({ label: 'Todo' });
                    await summarySelects.nth(3).selectOption({ label: 'Medium' });

                    const reporterSelect = page.locator('#task-panel-reporter-select');
                    await reporterSelect.waitFor({ state: 'attached', timeout: 10_000 });
                    const reporterOptions = await reporterSelect.locator('option').allTextContents();
                    expect(reporterOptions.some((opt) => opt.includes('allowed@example.com'))).toBe(true);
                    expect(reporterOptions.some((opt) => opt.includes('reviewer@example.com'))).toBe(true);
                    await reporterSelect.selectOption({ value: 'allowed@example.com' });

                    const assigneeSelect = page.locator('#task-panel-assignee-select');
                    const assigneeOptions = await assigneeSelect.locator('option').allTextContents();
                    expect(assigneeOptions.some((opt) => opt.includes('allowed@example.com'))).toBe(true);
                    expect(assigneeOptions.some((opt) => opt.includes('reviewer@example.com'))).toBe(true);

                    await assigneeSelect.selectOption({ value: '__custom' });

                    const assigneeInput = page.getByPlaceholder('Type assignee');
                    let filled = false;
                    let attempts = 0;
                    let lastError: unknown = null;

                    while (!filled && attempts < 5) {
                        attempts += 1;
                        try {
                            await assigneeInput.waitFor({ state: 'visible', timeout: 5_000 });
                            await assigneeInput.fill('intruder@example.com');
                            filled = true;
                        } catch (error) {
                            lastError = error;
                            await assigneeSelect.selectOption({ value: '__custom' });
                            await page.waitForTimeout(200);
                        }
                    }

                    if (!filled) {
                        throw lastError instanceof Error
                            ? lastError
                            : new Error(`Failed to enter custom assignee: ${String(lastError)}`);
                    }

                    await assigneeInput.press('Enter');

                    await page.getByPlaceholder('Title').fill('UI Strict Members Smoke Task');

                    await page.getByRole('button', { name: 'Create task' }).click();

                    await page.waitForSelector(`.card:has-text("Assignee 'intruder@example.com' is not in configured members")`, {
                        timeout: 10_000,
                    });

                    await page.getByRole('button', { name: 'Use list' }).click();
                    await assigneeSelect.selectOption({ value: 'allowed@example.com' });

                    await page.getByRole('button', { name: 'Create task' }).click();

                    await page.waitForSelector('.card:has-text("Task created")', { timeout: 10_000 });
                    await panel.waitFor({ state: 'hidden', timeout: 10_000 });

                    await page.waitForSelector('text=UI Strict Members Smoke Task', { timeout: 10_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('auto-populates members when assigning a new reporter with strict members disabled', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTO_POPULATE_CONFIG);

            await workspace.addTask('Seed task for auto-population', {
                args: ['--project', 'CLI', '--reporter=allowed@example.com', '--assignee=allowed@example.com'],
            });

            const baselineTaskFiles = await workspace.listTaskFiles();
            const server = await startLotarServer(workspace);

            let projectPrefix = 'CLI';

            try {
                await withPage(server.url, async (page) => {
                    const newReporter = 'ui-autopop@example.com';

                    await page.waitForSelector('text=LoTaR', { timeout: 15_000 });
                    await page.waitForSelector('text=Tasks', { timeout: 15_000 });

                    const addButton = page.getByRole('button', { name: 'Add' });
                    await addButton.waitFor({ state: 'visible', timeout: 15_000 });
                    await addButton.click();

                    const panel = page.locator('[aria-label="Create task"]');
                    await panel.waitFor({ state: 'visible', timeout: 15_000 });

                    const summarySelects = panel.locator('select');
                    const projectSelect = summarySelects.nth(0);
                    await projectSelect.waitFor({ state: 'attached', timeout: 10_000 });
                    const projectValue = await projectSelect.evaluate((node) => {
                        if (!(node instanceof HTMLSelectElement)) {
                            return '';
                        }

                        const usable = Array.from(node.options).find((option) => {
                            return !option.disabled && option.value.trim().length > 0;
                        });
                        return usable ? usable.value.trim() : '';
                    });
                    if (projectValue) {
                        await projectSelect.selectOption(projectValue);
                    }

                    await summarySelects.nth(1).selectOption({ label: 'Feature' });
                    await summarySelects.nth(2).selectOption({ label: 'Todo' });
                    await summarySelects.nth(3).selectOption({ label: 'Medium' });

                    await page.selectOption('#task-panel-reporter-select', '__custom');
                    const reporterInput = page.locator('input[placeholder="Type reporter"]');
                    await reporterInput.waitFor({ state: 'visible', timeout: 15_000 });
                    await reporterInput.fill(newReporter);
                    await reporterInput.press('Enter');

                    await page.getByPlaceholder('Title').fill('UI Auto-Pop Members Smoke Task');

                    await page.getByRole('button', { name: 'Create task' }).click();
                    await page.waitForSelector('.card:has-text("Task created")', { timeout: 15_000 });
                    await panel.waitFor({ state: 'hidden', timeout: 15_000 });
                });
            } finally {
                await server.stop();
            }

            const taskDetectionDeadline = Date.now() + 10_000;
            let detectedTaskPath: string | null = null;
            while (Date.now() < taskDetectionDeadline) {
                const currentFiles = await workspace.listTaskFiles();
                const newFiles = currentFiles.filter((file) => !baselineTaskFiles.includes(file));
                if (newFiles.length > 0) {
                    detectedTaskPath = newFiles[0];
                    break;
                }
                await new Promise((resolve) => setTimeout(resolve, 200));
            }

            expect(detectedTaskPath).toBeTruthy();
            if (detectedTaskPath) {
                projectPrefix = path.basename(path.dirname(detectedTaskPath));
            }

            const projectConfigPath = path.join(workspace.tasksDir, projectPrefix, 'config.yml');
            const configDeadline = Date.now() + 10_000;
            let contents = '';
            while (Date.now() < configDeadline) {
                if (await fs.pathExists(projectConfigPath)) {
                    contents = await fs.readFile(projectConfigPath, 'utf8');
                    if (contents.includes('ui-autopop@example.com')) {
                        break;
                    }
                }
                await new Promise((resolve) => setTimeout(resolve, 200));
            }

            expect(contents).toContain('ui-autopop@example.com');

            const projectConfig = parse(contents) as Record<string, any>;
            const members = Array.isArray(projectConfig?.members)
                ? (projectConfig.members as string[])
                : Array.isArray(projectConfig?.default?.members)
                    ? (projectConfig.default.members as string[])
                    : [];
            expect(members).toContain('allowed@example.com');
            expect(members).toContain('ui-autopop@example.com');
        } finally {
            await workspace.dispose();
        }
    });
});
