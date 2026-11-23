import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

// DEV-491: automation's notes (job failed, retry started) render smaller and
// muted, so the agents' and the owner's comments stand out; links still work.
describe.concurrent('UI comments', () => {
    for (const colorScheme of ['light', 'dark'] as const) {
        it(`mutes automation's comments in the ${colorScheme} theme`, async ({ expect }) => {
            const workspace = await SmokeWorkspace.create();
            try {
                const created = await workspace.addTask('Comments task');
                const comment = (author: string, text: string) =>
                    workspace.runLotar(['comment', created.id, text], { env: { LOTAR_DEFAULT_REPORTER: author } });
                await comment('claude', 'Started the fix.');
                await comment('automation', 'Agent job failed, see [the log](https://example.com/log).');
                await comment('mallox', 'Looks good.');
                const server = await startLotarServer(workspace);
                try {
                    await withPage(
                        `${server.url}/task/${created.id}`,
                        async (page) => {
                            await page.setViewportSize({ width: 390, height: 900 });
                            await page.getByRole('tab', { name: 'Comments', exact: true }).click();
                            const comments = page.locator('.task-panel__comment');
                            await expect.poll(() => comments.count()).toBe(3);

                            const style = (index: number) =>
                                comments.nth(index).evaluate((el) => {
                                    const body = el.querySelector('.markdown-content')!;
                                    const computed = getComputedStyle(body);
                                    return { color: computed.color, fontSize: parseFloat(computed.fontSize) };
                                });
                            const [agent, automation, owner] = [await style(0), await style(1), await style(2)];
                            expect(owner).toEqual(agent);
                            expect(automation.fontSize).toBeLessThan(agent.fontSize);
                            expect(automation.color).not.toBe(agent.color);

                            const link = comments.nth(1).getByRole('link', { name: 'the log' });
                            expect(await link.getAttribute('href')).toBe('https://example.com/log');
                        },
                        { context: { colorScheme } },
                    );
                } finally {
                    await server.stop();
                }
            } finally {
                await workspace.dispose();
            }
        });
    }

    // DEV-809: Delete (Withdraw on an open question) hides a comment behind the
    // "N deleted" toggle; Restore brings it back. The task file keeps it.
    it('withdraws a question and restores it', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create();
        try {
            const created = await workspace.addTask('Delete comments task');
            await workspace.runLotar(['comment', created.id, 'A note to keep.']);
            await workspace.runLotar([
                'comment',
                created.id,
                '--question',
                'Ship it?',
                '--option',
                'y: Yes',
                '--option',
                'n: No',
            ]);
            const server = await startLotarServer(workspace);
            try {
                await withPage(`${server.url}/task/${created.id}`, async (page) => {
                    await page.getByRole('tab', { name: 'Comments', exact: true }).click();
                    const live = page.locator('.task-panel__comments > .task-panel__comment');
                    await expect.poll(() => live.count()).toBe(2);
                    await page.getByTestId('comment-delete').filter({ hasText: 'Withdraw' }).click();
                    await expect.poll(() => live.count()).toBe(1);

                    const toggle = page.getByTestId('comments-deleted-toggle');
                    await expect.poll(() => toggle.textContent()).toContain('Show 1 deleted');
                    await toggle.click();
                    await expect.poll(() => page.getByTestId('comment-deleted').textContent()).toContain('Ship it?');
                    const shown = JSON.parse(
                        (await workspace.runLotar(['--format', 'json', 'show', created.id])).stdout,
                    );
                    expect(shown.comments.map((c: { text: string }) => c.text)).toEqual(['A note to keep.']);

                    await page.getByTestId('comment-restore').click();
                    await expect.poll(() => live.count()).toBe(2);
                    await expect.poll(() => toggle.count()).toBe(0);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
