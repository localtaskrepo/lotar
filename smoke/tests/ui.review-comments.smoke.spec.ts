import path from 'node:path';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const ARTIFACTS = path.join(process.cwd(), 'target/smoke-artifacts/review-comments');

describe('DEV-489 review comments (actual backend/browser)', () => {
    it('shows the verdict badge with the commit, the findings, and the newest verdict on the panel', async ({
        expect,
    }) => {
        const workspace = await SmokeWorkspace.create({ name: 'review-ui-' });
        try {
            const task = await workspace.addTask('Reviewed work', { args: ['--project=REV'] });
            const env = { env: { LOTAR_DEFAULT_REPORTER: 'codex' } };
            await workspace.runLotar(['comment', task.id, '--review', 'pass', '--commit', '1111111aaaa'], env);
            await workspace.runLotar(
                [
                    'comment',
                    task.id,
                    '--review',
                    'block',
                    '--commit',
                    'abc1234def',
                    '--finding',
                    'docs/help/auth.md: the example is out of date',
                    '--blocker',
                    'src/auth.rs:42: the token is logged',
                    'The change leaks the token.',
                ],
                env,
            );
            const server = await startLotarServer(workspace, { env: { LOTAR_SSE_READY: '1' } });
            try {
                await withBrowser({}, async (context) => {
                    const page = await context.newPage();
                    await page.goto(`${server.url}/?project=REV&taskPanel=${task.id}`, {
                        waitUntil: 'domcontentloaded',
                    });
                    const panel = page.locator('.task-panel');
                    const reviews = panel.locator('[data-testid="comment-review"]');
                    await expect.poll(() => reviews.count()).toBe(2);

                    const headerBadge = panel.locator('[data-testid="task-panel-review-badge"]');
                    await expect.poll(() => headerBadge.getAttribute('data-verdict')).toBe('block');
                    expect(await headerBadge.textContent()).toContain('abc1234');

                    const passed = reviews.nth(0);
                    expect(await passed.locator('.review-badge').getAttribute('data-verdict')).toBe('pass');
                    expect(await passed.textContent()).not.toContain('Review passed at');

                    const blocked = reviews.nth(1);
                    const badge = blocked.locator('.review-badge');
                    expect(await badge.textContent()).toContain('Review blocked');
                    expect(await badge.textContent()).toContain('abc1234');
                    expect(await blocked.textContent()).toContain('The change leaks the token.');
                    expect(await blocked.textContent()).not.toContain('Findings:');
                    const findings = blocked.locator('[data-testid="review-finding"]');
                    expect(await findings.count()).toBe(2);
                    expect(await findings.nth(0).textContent()).toContain('Blocking');
                    expect(await findings.nth(0).textContent()).toContain('src/auth.rs:42');
                    expect(await findings.nth(1).textContent()).toContain('docs/help/auth.md');
                    expect(await findings.nth(1).textContent()).not.toContain('Blocking');

                    // The Type chips offer reviews.
                    expect(await panel.getByRole('button', { name: 'Review', exact: true }).count()).toBe(1);

                    await blocked.screenshot({
                        path: path.join(ARTIFACTS, 'review-blocked.png'),
                        animations: 'disabled',
                    });
                    await panel
                        .locator('.task-panel__header')
                        .screenshot({ path: path.join(ARTIFACTS, 'panel-header.png'), animations: 'disabled' });
                    await page.emulateMedia({ colorScheme: 'dark' });
                    await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'dark'));
                    await blocked.screenshot({
                        path: path.join(ARTIFACTS, 'review-blocked-dark.png'),
                        animations: 'disabled',
                    });
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
