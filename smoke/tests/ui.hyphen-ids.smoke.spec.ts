import { expect } from '@playwright/test';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-56 durable browser coverage for hyphenated project prefixes:
// - tasks in a project like `ABC-OPS` render with the full final-dash prefix
//   (no first-dash `ABC` / `OPS-N` fallback),
// - the panel creates, opens, autosaves, and attaches for `ABC-OPS` ids,
// - config inspection and attachment URLs carry the full exact-case prefix.
describe('UI hyphenated task ids', () => {
    it('creates, opens, edits, and attaches tasks under a hyphenated project prefix', async () => {
        const workspace = await SmokeWorkspace.create();
        const seedTitle = 'Hyphen prefix seed';
        const editedTitle = 'Hyphen prefix edited';

        try {
            // `--project` keeps a literal prefix only when the project directory
            // already exists (otherwise the CLI derives initials), so seed the
            // hyphenated project directory first.
            await workspace.write('.tasks/ABC-OPS/.seed', '');
            const seeded = await workspace.addTask(seedTitle, { args: ['--project=ABC-OPS'] });
            expect(seeded.project).toBe('ABC-OPS');
            expect(seeded.id).toMatch(/^ABC-OPS-\d+$/);

            const server = await startLotarServer(workspace);

            try {
                await withPage(`${server.url}/?project=ABC-OPS`, async page => {
                    const inspectedProjects = new Set<string>();
                    const addBodies: Record<string, unknown>[] = [];
                    const updateBodies: Record<string, unknown>[] = [];
                    let createdId = '';
                    page.on('request', request => {
                        const url = new URL(request.url());
                        if (url.pathname === '/api/config/inspect') {
                            inspectedProjects.add(url.searchParams.get('project') ?? '');
                        }
                        if (url.pathname === '/api/tasks/add') addBodies.push(request.postDataJSON());
                    });
                    page.on('response', async response => {
                        if (new URL(response.url()).pathname === '/api/tasks/add' && response.ok()) {
                            try {
                                const payload = await response.json();
                                createdId = String(payload?.data?.id ?? '');
                            } catch {
                                // id polled below
                            }
                        }
                    });
                    page.on('request', request => {
                        if (new URL(request.url()).pathname === '/api/tasks/update') {
                            updateBodies.push(request.postDataJSON());
                        }
                    });

                    // Open: the seeded row renders the full prefix and the bare number.
                    const seedRow = page.locator('tr', { hasText: seedTitle });
                    await seedRow.waitFor({ timeout: 20_000 });
                    await expect(seedRow).toContainText('ABC-OPS');
                    await expect(seedRow).not.toContainText('OPS-1');

                    // Create: the panel submits project `ABC-OPS` atomically.
                    await page.getByRole('button', { name: 'Add task' }).click();
                    const panel = page.locator('.task-panel');
                    const create = panel.getByRole('button', { name: 'Create task', exact: true });
                    await expect.poll(() => create.isEnabled(), { timeout: 10_000 }).toBe(true);
                    await panel.locator('input[placeholder="Title"]').fill('Hyphen UI created');
                    await create.click();
                    await expect.poll(() => addBodies.length, { timeout: 10_000 }).toBe(1);
                    expect(addBodies[0]).toMatchObject({ project: 'ABC-OPS', title: 'Hyphen UI created' });
                    await expect.poll(() => createdId, { timeout: 10_000 }).toMatch(/^ABC-OPS-\d+$/);
                    await expect
                        .poll(async () => workspace.readTaskYaml(createdId), { timeout: 10_000 })
                        .toContain('title: Hyphen UI created');
                    await panel.waitFor({ state: 'hidden', timeout: 10_000 });

                    // Open the seeded task and edit it; autosave targets the full id.
                    await seedRow.click();
                    await panel.waitFor({ state: 'visible', timeout: 10_000 });
                    const title = panel.locator('input[placeholder="Title"]');
                    await expect
                        .poll(() => title.inputValue(), { timeout: 10_000 })
                        .toBe(seedTitle);
                    await title.fill(editedTitle);
                    await title.blur();
                    await expect.poll(() => updateBodies.length, { timeout: 10_000 }).toBeGreaterThanOrEqual(1);
                    expect(updateBodies[0]).toMatchObject({ id: seeded.id, title: editedTitle });
                    await expect
                        .poll(async () => workspace.readTaskYaml(seeded.id), { timeout: 10_000 })
                        .toContain(`title: ${editedTitle}`);

                    // Config inspection used the full exact-case prefix, never the first segment.
                    expect(inspectedProjects.has('ABC-OPS')).toBe(true);
                    expect(inspectedProjects.has('ABC')).toBe(false);

                    // Attachment lifecycle: drop a file, expect the full prefix in the URL.
                    await page.evaluate(() => {
                        const target = document.querySelector('.task-panel');
                        if (!target) throw new Error('task panel not found');
                        const dt = new DataTransfer();
                        dt.items.add(
                            new File(['hyphen attachment body'], 'hyphen-notes.txt', { type: 'text/plain' }),
                        );
                        target.dispatchEvent(
                            new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt }),
                        );
                    });
                    const attachmentLink = panel.locator('.task-panel__attachment-link');
                    await attachmentLink.waitFor({ timeout: 15_000 });
                    const href = await attachmentLink.first().getAttribute('href');
                    expect(href).toContain('project=ABC-OPS');

                    await panel.locator('button[aria-label="Remove attachment"]').click();
                    await attachmentLink.waitFor({ state: 'detached', timeout: 15_000 });

                    // Durable across reload: the edited title keeps the hyphenated id.
                    await page.reload({ waitUntil: 'domcontentloaded' });
                    await page.waitForSelector(`text=${editedTitle}`, { timeout: 15_000 });
                    const editedRow = page.locator('tr', { hasText: editedTitle });
                    await expect(editedRow).toContainText('ABC-OPS');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
