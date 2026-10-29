import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const BASE_CONFIG = `default:
  project: UIAUT
issue:
  states: [Todo, InProgress, Review, Done]
  priorities: [Low, Medium, High, Critical]
  types: [Feature, Bug, Chore]
agent:
  worktree:
    enabled: false
`;

const SEED_RULES = `automation:
  rules:
    - name: Auto-tag bugs
      when:
        type: Bug
      on:
        created:
          set:
            priority: High
          add:
            tags: [bug-detected]
    - name: Review notification
      when:
        status: Review
      on:
        updated:
          comment: "Ready for review"
`;

describe.concurrent('UI Automation page smoke tests', () => {
    it('cannot save old rules while a new project load is pending or failed', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASE_CONFIG, '.tasks/automation.yml': SEED_RULES },
        });
        try {
            await workspace.addTask('Scope safety target');
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/automations`, async (page) => {
                    await page.waitForSelector('.rule-card', { timeout: 15_000 });
                    await page.getByText('Advanced YAML and rule engine settings', { exact: true }).click();
                    await page.locator('textarea:not([readonly])').fill(`${SEED_RULES}\n# unsaved global edit\n`);
                    expect(await page.getByRole('button', { name: 'Save rules', exact: true }).isEnabled()).toBe(true);
                    let release!: () => void;
                    const pending = new Promise<void>((resolve) => {
                        release = resolve;
                    });
                    const writes: string[] = [];
                    page.on('request', (request) => {
                        if (request.url().includes('/api/automation/set')) writes.push(request.postData() || '');
                    });
                    await page.route('**/api/automation/show?*', async (route) => {
                        await pending;
                        await route.fulfill({
                            status: 500,
                            contentType: 'application/json',
                            body: JSON.stringify({ error: 'Scope unavailable' }),
                        });
                    });
                    await page.selectOption('select[aria-label="Project scope"]', 'UIAUT');
                    await page.getByText('Loading configuration', { exact: false }).waitFor();
                    expect(await page.getByRole('button', { name: 'Save rules', exact: true }).count()).toBe(0);
                    release();
                    await page.locator('.error').waitFor();
                    expect(await page.getByRole('button', { name: 'Save rules', exact: true }).count()).toBe(0);
                    expect(writes).toEqual([]);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('renders rules from automation.yml', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': SEED_RULES,
            },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/automations`, async (page) => {
                    await page.waitForSelector('h1', { timeout: 15_000 });
                    const heading = await page.textContent('h1');
                    expect(heading).toContain('Automations');

                    // Rules should be rendered
                    await page.waitForSelector('.rule-card', { timeout: 10_000 });
                    const ruleNames = await page.$$eval('.rule-name', (els) => els.map((el) => el.textContent?.trim()));
                    expect(ruleNames).toContain('Auto-tag bugs');
                    expect(ruleNames).toContain('Review notification');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('simulator tab renders and shows empty state for no match', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': SEED_RULES,
            },
        });

        try {
            const task = await workspace.addTask('Simulate target');
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/automations`, async (page) => {
                    await page.waitForSelector('h1', { timeout: 15_000 });

                    // Click Simulator tab
                    await page.click('.filter-tab:has-text("Simulator")');
                    await page.waitForSelector('.simulator-card', { timeout: 5_000 });

                    const simText = await page.textContent('.simulator-card');
                    expect(simText).toContain('Test automation rules');
                    expect(simText).toContain('Simulate');

                    // Fill in the simulator form - use an event that won't match
                    await page.selectOption('select[aria-label="Event type"]', 'cancel');
                    await page.fill('input[placeholder*="PROJ-123"]', task.id);

                    // Click Simulate
                    await page.click('button:has-text("Simulate")');

                    // Wait for result
                    await page.waitForSelector('.result-card', { timeout: 10_000 });
                    const resultText = await page.textContent('.result-card');
                    expect(resultText).toContain('No rules matched');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('simulator previews every set field and lists run-time warnings (DEV-118)', async () => {
        const rules = `automation:
  rules:
    - name: Job start preview
      on:
        job_started:
          set:
            effort: 90m
            due_date: "2026-12-01"
            assignee: "@round_robin"
            title: "Started \${{ticket.title}} after \${{previous.status}}"
`;
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASE_CONFIG, '.tasks/automation.yml': rules },
        });
        try {
            const task = await workspace.addTask('Preview target');
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/automations`, async (page) => {
                    await page.waitForSelector('h1', { timeout: 15_000 });
                    await page.click('.filter-tab:has-text("Simulator")');
                    await page.selectOption('select[aria-label="Event type"]', 'job_start');
                    await page.fill('input[placeholder*="PROJ-123"]', task.id);
                    await page.click('button:has-text("Simulate")');
                    const warnings = page.getByTestId('simulation-warnings');
                    await warnings.waitFor({ timeout: 10_000 });
                    const warningText = (await warnings.textContent()) ?? '';
                    expect(warningText).toContain("Assignee '@round_robin' is chosen when the rule runs");
                    expect(warningText).toContain('previous.status');
                    const results = (await page.locator('.result-card').allTextContents()).join('\n');
                    expect(results).toContain("Set effort to '90m'");
                    expect(results).toContain('Started Preview target after ${{previous.status}}');
                    expect(results).toContain('Due 2026-12-01');
                    expect(results).toContain('1.50h');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('Rules and Simulator tabs toggle content', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': SEED_RULES,
            },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/automations`, async (page) => {
                    await page.waitForSelector('.rule-card', { timeout: 15_000 });

                    // Rules tab is active by default
                    const rulesVisible = await page.isVisible('.rule-card');
                    expect(rulesVisible).toBe(true);

                    // Switch to Simulator
                    await page.click('.filter-tab:has-text("Simulator")');
                    await page.waitForSelector('.simulator-card', { timeout: 5_000 });
                    const simVisible = await page.isVisible('.simulator-card');
                    expect(simVisible).toBe(true);

                    // Switch back to Rules
                    await page.click('.filter-tab:has-text("Rules")');
                    await page.waitForSelector('.rule-card', { timeout: 5_000 });
                    const rulesBack = await page.isVisible('.rule-card');
                    expect(rulesBack).toBe(true);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('creates a rule on one screen, tries it on a task and saves it (DEV-241)', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASE_CONFIG },
        });
        try {
            const task = await workspace.addTask('Try target');
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/automations`, async (page) => {
                    await page.setViewportSize({ width: 1280, height: 1000 });
                    await page.getByTestId('automation-new-rule').click();
                    const dialog = page.getByTestId('automation-rule-dialog');
                    await dialog.waitFor();
                    await dialog.getByTestId('automation-rule-name').fill('Deploy complete');
                    await dialog.getByTestId('automation-trigger-complete').click();
                    await dialog.getByTestId('automation-add-action-complete').selectOption('set:status');
                    const status = dialog.getByTestId('automation-set-status');
                    await status.fill('Done');
                    // Configured statuses are offered as suggestions.
                    const listId = await status.getAttribute('list');
                    const options = await page
                        .locator(`#${listId} option`)
                        .evaluateAll((els) => els.map((el) => el.getAttribute('value')));
                    expect(options).toEqual(expect.arrayContaining(['Todo', 'InProgress', 'Review', 'Done']));
                    await dialog.getByTestId('automation-add-action-complete').selectOption('comment');
                    await dialog.getByTestId('automation-comment').fill('Shipped ${{ticket.title}}');

                    await dialog.getByTestId('automation-try-task').fill(task.id);
                    await dialog.getByTestId('automation-try-task').dispatchEvent('change');
                    await dialog.getByTestId('automation-try').click();
                    const result = dialog.getByTestId('automation-try-result');
                    await result.waitFor({ timeout: 10_000 });
                    expect(await result.textContent()).toContain('Shipped Try target');
                    expect(await dialog.getByTestId('automation-comment-preview').textContent()).toContain(
                        `On ${task.id}: Shipped Try target`,
                    );

                    await dialog.getByTestId('automation-rule-save').click();
                    await dialog.waitFor({ state: 'detached' });
                    await page.getByRole('button', { name: 'Save rules', exact: true }).click();
                    await page.waitForFunction(() =>
                        [...document.querySelectorAll('button')]
                            .find((b) => b.textContent?.includes('Save rules'))
                            ?.hasAttribute('disabled'),
                    );
                    await page.click('button[title="Refresh automations"]');
                    await page.waitForSelector('.rule-card', { timeout: 10_000 });
                    const summary = (await page.getByTestId('automation-rule-0').textContent()) ?? '';
                    expect(summary).toContain('Deploy complete');
                    expect(summary.replace(/\s+/g, ' ')).toContain(
                        'When an agent job finishes: then set status to Done, comment',
                    );
                    const saved = await workspace.read('.tasks/automation.yml');
                    expect(saved).toContain('job_completed:');
                    expect(saved).toContain('status: Done');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('runs a manual rule now, and a switched-off rule cannot run (DEV-241)', async ({ expect }) => {
        const rules = `automation:
  rules:
    - name: Flag finished work
      when:
        status: Done
      on:
        manual:
          add:
            tags: [flagged]
`;
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASE_CONFIG, '.tasks/automation.yml': rules },
        });
        try {
            const done = await workspace.addTask('Finished one', { args: ['--status=Done'] });
            await workspace.addTask('Still open');
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/automations`, async (page) => {
                    await page.setViewportSize({ width: 1280, height: 900 });
                    await page.getByTestId('automation-run-0').click();
                    const dialog = page.getByTestId('automation-run-dialog');
                    const preview = dialog.getByTestId('automation-run-preview');
                    await preview.waitFor({ timeout: 10_000 });
                    expect(await preview.textContent()).toContain(done.id);
                    expect(await preview.textContent()).not.toContain('Still open');
                    expect(await workspace.readTaskYaml(done.id)).not.toContain('flagged');
                    await dialog.getByTestId('automation-run-confirm').click();
                    await expect
                        .poll(async () => workspace.readTaskYaml(done.id), {
                            timeout: 10_000,
                        })
                        .toContain('flagged');
                    await dialog.getByRole('button', { name: 'Close', exact: true }).click();

                    // Off: the switch marks it, and Run now waits until it is on again.
                    await page.getByTestId('automation-rule-toggle-0').uncheck();
                    expect(await page.getByTestId('automation-run-0').isDisabled()).toBe(true);
                    await page.getByRole('button', { name: 'Save rules', exact: true }).click();
                    await expect
                        .poll(async () => workspace.read('.tasks/automation.yml'), { timeout: 10_000 })
                        .toContain('enabled: false');

                    // The editor fits a phone.
                    await page.setViewportSize({ width: 390, height: 844 });
                    await page.getByTestId('automation-new-rule').click();
                    await page.getByTestId('automation-rule-dialog').waitFor();
                    await page.getByTestId('automation-recipe-agent-handback').click();
                    const overflow = await page.evaluate(
                        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
                    );
                    expect(overflow).toBeLessThanOrEqual(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
