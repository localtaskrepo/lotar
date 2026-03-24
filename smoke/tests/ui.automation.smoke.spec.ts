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
                    const ruleNames = await page.$$eval('.rule-name', (els) =>
                        els.map((el) => el.textContent?.trim()),
                    );
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

    it('creates a rule through the guided dialog', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
            },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/automations`, async (page) => {
                    await page.waitForSelector('h1', { timeout: 15_000 });

                    // Open the create dialog
                    await page.click('button:has-text("New rule")');
                    await page.waitForSelector('.automation-builder__dialog', { timeout: 5_000 });

                    // Step 0 (Goal): pick "Move a task" recipe
                    await page.click('.automation-builder__recipe-card:has-text("Move a task")');

                    // Fill in rule name
                    await page.fill('input[placeholder="Human review after testing"]', 'Deploy complete');

                    // Click Next to go to step 1 (Trigger)
                    await page.click('button:has-text("Next")');

                    // Step 1: pick the "complete" trigger
                    await page.click('.automation-builder__event-toggle:has-text("complete")');

                    // Click Next to go to step 2 (Result)
                    await page.click('button:has-text("Next")');

                    // Step 2: fill in status value
                    await page.fill('input[placeholder="Done"]', 'Done');

                    // Click Next to go to step 3 (Review)
                    await page.click('button:has-text("Next")');

                    // Step 3: verify the review shows our rule
                    const reviewText = await page.textContent('.automation-builder__dialog');
                    expect(reviewText).toContain('Deploy complete');
                    expect(reviewText).toContain('complete');

                    // Save the rule
                    await page.click('button:has-text("Create rule")');

                    // Dialog should close and the new rule card should appear
                    await page.waitForSelector('.rule-card', { timeout: 5_000 });
                    const ruleNames = await page.$$eval('.rule-name', (els) =>
                        els.map((el) => el.textContent?.trim()),
                    );
                    expect(ruleNames).toContain('Deploy complete');

                    // Save button should be enabled (dirty state)
                    const saveButton = page.locator('button:has-text("Save rules")');
                    await saveButton.waitFor({ state: 'visible', timeout: 3_000 });
                    const disabled = await saveButton.isDisabled();
                    expect(disabled).toBe(false);

                    // Save the rules to persist
                    await saveButton.click();

                    // Wait for save to complete — save button should become disabled again
                    await page.waitForFunction(
                        () => {
                            const btn = [...document.querySelectorAll('button')].find((b) => b.textContent?.includes('Save rules'));
                            return btn?.hasAttribute('disabled');
                        },
                        { timeout: 10_000 },
                    );

                    // Verify the rule persisted by refreshing
                    await page.click('button[title="Refresh automations"]');
                    await page.waitForSelector('.rule-card', { timeout: 10_000 });
                    const refreshedNames = await page.$$eval('.rule-name', (els) =>
                        els.map((el) => el.textContent?.trim()),
                    );
                    expect(refreshedNames).toContain('Deploy complete');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('guided dialog shows project-aware suggestions', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
            },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/automations`, async (page) => {
                    await page.waitForSelector('h1', { timeout: 15_000 });

                    // Open the create dialog
                    await page.click('button:has-text("New rule")');
                    await page.waitForSelector('.automation-builder__dialog', { timeout: 5_000 });

                    // Select "Move a task" recipe
                    await page.click('.automation-builder__recipe-card:has-text("Move a task")');

                    // Go to trigger step
                    await page.click('button:has-text("Next")');
                    await page.click('.automation-builder__event-toggle:has-text("complete")');

                    // Go to result step
                    await page.click('button:has-text("Next")');

                    // Status field should have a datalist with configured statuses
                    const datalist = page.locator('#automation-status-suggestions');
                    const options = datalist.locator('option');
                    const count = await options.count();
                    expect(count).toBeGreaterThanOrEqual(4);

                    // Check that our configured statuses are present
                    const values: string[] = [];
                    for (let i = 0; i < count; i++) {
                        const value = await options.nth(i).getAttribute('value');
                        if (value) values.push(value);
                    }
                    expect(values).toContain('Todo');
                    expect(values).toContain('Done');
                    expect(values).toContain('InProgress');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
