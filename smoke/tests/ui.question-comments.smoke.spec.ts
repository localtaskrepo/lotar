import path from 'node:path';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const ARTIFACTS = path.join(process.cwd(), 'target/smoke-artifacts/question-comments');

async function ask(workspace: SmokeWorkspace, id: string, extra: string[] = []) {
    await workspace.runLotar(
        [
            'comment',
            id,
            '--question',
            'Rework or close?',
            '--option',
            'rework: Rework it',
            '--option',
            'close: Close it',
            '--set',
            'close:status=Done',
            '--recommend',
            'rework',
            ...extra,
        ],
        { env: { LOTAR_DEFAULT_REPORTER: 'claude' } },
    );
}

describe('DEV-488 question comments (actual backend/browser)', () => {
    it('answers by click in the Comments tab, by Other text on the Tasks list, and from a board card', async ({
        expect,
    }) => {
        const workspace = await SmokeWorkspace.create({ name: 'question-ui-' });
        try {
            const panelTask = await workspace.addTask('Question in the panel', { args: ['--project=QST'] });
            const listTask = await workspace.addTask('Question on the list', { args: ['--project=QST'] });
            const boardTask = await workspace.addTask('Question on the board', { args: ['--project=QST'] });
            for (const task of [panelTask, listTask, boardTask]) {
                await ask(workspace, task.id);
            }
            const server = await startLotarServer(workspace, { env: { LOTAR_SSE_READY: '1' } });
            try {
                await withBrowser({}, async (context) => {
                    const page = await context.newPage();
                    try {
                        // Comments tab: the options are buttons, the recommended one marked.
                        await page.goto(`${server.url}/?project=QST&taskPanel=${panelTask.id}`, {
                            waitUntil: 'domcontentloaded',
                        });
                        const panel = page.locator('.task-panel');
                        const question = panel.locator('[data-testid="comment-question"]');
                        await expect.poll(() => question.count()).toBe(1);
                        const close = question.locator('[data-option="close"]');
                        await expect.poll(() => close.isVisible()).toBe(true);
                        expect(await question.locator('[data-option="rework"]').textContent()).toContain('recommended');
                        expect(await question.textContent()).not.toContain('Options:');
                        await question.screenshot({
                            path: path.join(ARTIFACTS, 'panel-question.png'),
                            animations: 'disabled',
                        });
                        await close.click();
                        const answer = question.locator('[data-testid="comment-answer"]');
                        await expect.poll(() => answer.count()).toBe(1);
                        expect(await answer.textContent()).toContain('Close it');
                        await question.screenshot({
                            path: path.join(ARTIFACTS, 'panel-answer.png'),
                            animations: 'disabled',
                        });
                        await expect
                            .poll(async () => (await workspace.readTaskYaml(panelTask.id)).includes('status: Done'))
                            .toBe(true);
                        expect(await question.locator('[data-testid="question-prompt"]').count()).toBe(0);

                        // Tasks list: the row shows the open question; Other answers in words.
                        await page.goto(`${server.url}/?project=QST`, { waitUntil: 'domcontentloaded' });
                        const row = page.locator('tbody tr', { hasText: 'Question on the list' });
                        const toggle = row.locator('[data-testid="open-question-toggle"]');
                        await expect.poll(() => toggle.count()).toBe(1);
                        expect(await page.locator('[data-testid="open-question"]').count()).toBe(2);
                        await toggle.click();
                        await row.locator('[data-testid="question-other-input"]').fill('Ask me next week');
                        await page.screenshot({
                            path: path.join(ARTIFACTS, 'list-question.png'),
                            animations: 'disabled',
                        });
                        await row.locator('[data-testid="question-other-submit"]').click();
                        await expect.poll(() => row.locator('[data-testid="open-question"]').count()).toBe(0);
                        expect(page.url()).not.toContain('taskPanel=');
                        const listYaml = await workspace.readTaskYaml(listTask.id);
                        expect(listYaml).toContain('note: Ask me next week');
                        expect(listYaml).toContain('status: Todo');

                        // Board card: answerable in place.
                        await page.goto(`${server.url}/boards?project=QST`, { waitUntil: 'domcontentloaded' });
                        const card = page.locator('[data-board-card]', { hasText: 'Question on the board' });
                        const cardToggle = card.locator('[data-testid="open-question-toggle"]');
                        await expect.poll(() => cardToggle.count()).toBe(1);
                        await cardToggle.click();
                        await page.screenshot({
                            path: path.join(ARTIFACTS, 'board-question.png'),
                            animations: 'disabled',
                        });
                        await card.locator('[data-option="rework"]').click();
                        await expect
                            .poll(async () => (await workspace.readTaskYaml(boardTask.id)).includes('- rework'))
                            .toBe(true);
                        await expect.poll(() => page.locator('[data-testid="open-question"]').count()).toBe(0);
                    } finally {
                        await page.close();
                    }
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
