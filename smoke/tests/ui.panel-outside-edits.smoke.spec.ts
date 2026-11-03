import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-263: an open task panel follows outside edits (here the CLI), so its
// next save builds on them; Escape saves the field being edited like the
// close button does, and a create draft is not dropped without asking.
describe('UI task panel outside edits and Escape', () => {
    it('shows an outside description edit and saves the typed note on Escape without losing it', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            const task = await workspace.addTask('Agent edited task', {
                args: ['--project=OUT', '--description=Original text'],
            });
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=OUT&taskPanel=${task.id}`, async (page) => {
                    const panel = page.locator('.task-panel');
                    const display = panel.locator('.task-panel__description-display');
                    await expect.poll(() => display.textContent(), { timeout: 15_000 }).toContain('Original text');

                    await workspace.runLotar([
                        'task',
                        'edit',
                        task.id,
                        '--description=Original text. Acceptance criteria added by agent.',
                    ]);
                    await expect
                        .poll(() => display.textContent(), { timeout: 15_000 })
                        .toContain('Acceptance criteria added by agent.');

                    await display.click();
                    const textarea = panel.locator('.task-panel__description textarea');
                    await textarea.waitFor({ state: 'visible' });
                    await textarea.press('End');
                    await textarea.pressSequentially(' User note.');
                    await page.keyboard.press('Escape');
                    await panel.waitFor({ state: 'detached', timeout: 10_000 });

                    await expect
                        .poll(() => workspace.readTaskYaml(task.id), { timeout: 10_000 })
                        .toContain('Acceptance criteria added by agent. User note.');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('asks before Escape discards a create draft', async () => {
        const workspace = await SmokeWorkspace.create();
        try {
            await workspace.addTask('Seed', { args: ['--project=OUT'] });
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/?project=OUT`, async (page) => {
                    await page.getByTestId('global-new-task').click();
                    const editor = page.getByRole('dialog', { name: 'Create task', exact: true });
                    await editor.waitFor({ state: 'visible' });
                    const title = editor.locator('input[placeholder="Title"]');
                    await title.fill('Draft that must survive Escape');

                    await page.keyboard.press('Escape');
                    const keep = page.getByTestId('task-panel-discard-cancel');
                    await keep.waitFor({ state: 'visible' });
                    await keep.click();
                    await keep.waitFor({ state: 'detached' });
                    expect(await title.inputValue()).toBe('Draft that must survive Escape');

                    await page.keyboard.press('Escape');
                    await page.getByTestId('task-panel-discard-confirm').click();
                    await editor.waitFor({ state: 'detached', timeout: 10_000 });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
