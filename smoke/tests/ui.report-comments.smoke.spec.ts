import path from 'node:path';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const ARTIFACTS = path.join(process.cwd(), 'target/smoke-artifacts/report-comments');

describe('DEV-490 hand-off report comments (actual backend/browser)', () => {
    it('shows commits as links and checks with pass, fail and not-run marks', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'report-comments-ui-' });
        try {
            await workspace.initGit();
            await workspace.runGit(['remote', 'add', 'origin', 'git@github.com:example/app.git']);
            const task = await workspace.addTask('Hand-off report', { args: ['--project=REP'] });
            await workspace.runLotar([
                'comment',
                task.id,
                'Report comments work',
                '--commit',
                '5b7385dc: REP-1 Hand-off report',
                '--check',
                'npm test=passed',
                '--check',
                'npm run lint=failed: clippy',
                '--check',
                'npm run smoke=not_run: no browser',
                '--next',
                'review agent',
            ]);
            await workspace.runLotar(['comment', task.id, 'Second pass', '--check', 'npm test=passed']);
            const server = await startLotarServer(workspace, { env: { LOTAR_SSE_READY: '1' } });
            try {
                await withBrowser({}, async (context) => {
                    const page = await context.newPage();
                    try {
                        await page.goto(`${server.url}/?project=REP&taskPanel=${task.id}`, {
                            waitUntil: 'domcontentloaded',
                        });
                        const reports = page.getByTestId('comment-report');
                        await expect.poll(() => reports.count()).toBe(2);
                        const first = reports.first();
                        const link = first.locator('a.comment-report__hash');
                        await expect
                            .poll(() => link.getAttribute('href'))
                            .toBe('https://github.com/example/app/commit/5b7385dc');
                        expect(await first.locator('.comment-report__commit').textContent()).toContain(
                            'REP-1 Hand-off report',
                        );
                        const checks = first.locator('.comment-report__check');
                        expect(
                            await checks.evaluateAll((items) => items.map((item) => item.getAttribute('data-result'))),
                        ).toEqual(['passed', 'failed', 'not_run']);
                        expect(await checks.nth(2).textContent()).toContain('not run');
                        expect(await checks.nth(2).textContent()).toContain('no browser');
                        expect(await first.locator('.comment-report__next').textContent()).toContain('review agent');
                        expect(await first.getByTestId('comment-report-missing').count()).toBe(0);
                        // A report without a not-run check says so.
                        expect(await reports.nth(1).getByTestId('comment-report-missing').textContent()).toBe(
                            'No check marked not run',
                        );
                        await page
                            .locator('.task-panel__comments')
                            .screenshot({ path: path.join(ARTIFACTS, 'comments.png') });
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
