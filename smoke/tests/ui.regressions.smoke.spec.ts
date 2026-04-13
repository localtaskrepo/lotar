import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('UI regression: assignee display and sprints project filter', () => {
    it('displays normalised assignee on the tasks page', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.addTask('Assignee display test', {
                args: ['--assignee=@alice', '--project', 'REG'],
            });

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.waitForSelector('text=Assignee display test', { timeout: 15_000 });

                    const bodyText = await page.evaluate(() => document.body.innerText);
                    // @alice normalises to 'alice' (@ prefix reserved for directives)
                    expect(bodyText).toContain('alice');
                    expect(bodyText).not.toContain('@@alice');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('displays normalised assignee on the sprints page', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'Assignee Sprint']);

            const task = await workspace.addTask('Sprint assignee test', {
                args: ['--assignee=@bob', '--project', 'REG'],
            });
            await workspace.runLotar(['sprint', 'add', task.id, '--sprint', '1']);

            const server = await startLotarServer(workspace);

            try {
                await withPage(server.url, async (page) => {
                    await page.click('a[href="/sprints"]');
                    await page.waitForSelector('text=Sprint assignee test', { timeout: 15_000 });

                    const bodyText = await page.evaluate(() => document.body.innerText);
                    // @bob normalises to 'bob' (@ prefix reserved for directives)
                    expect(bodyText).toContain('bob');
                    expect(bodyText).not.toContain('@@bob');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('filters sprints page by project query parameter', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'Project Filter Sprint']);

            const alphaTask = await workspace.addTask('Alpha project task', {
                args: ['--project', 'ALPHA'],
            });
            const betaTask = await workspace.addTask('Beta project task', {
                args: ['--project', 'BETA'],
            });
            await workspace.runLotar(['sprint', 'add', alphaTask.id, '--sprint', '1']);
            await workspace.runLotar(['sprint', 'add', betaTask.id, '--sprint', '1']);

            const server = await startLotarServer(workspace);

            try {
                // Navigate to sprints with ?project=ALPHA
                await withPage(`${server.url}/sprints?project=ALPHA`, async (page) => {
                    await page.waitForSelector('text=Project Filter Sprint', { timeout: 15_000 });
                    await page.waitForSelector('text=Alpha project task', { timeout: 15_000 });

                    // Beta task should not appear when filtering by ALPHA
                    await page.waitForFunction(
                        (excludeTitle: string) => !document.body.innerText.includes(excludeTitle),
                        'Beta project task',
                        { timeout: 15_000 },
                    );
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('sprint add accepts --project flag alongside positional task IDs', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'Project Flag Sprint']);

            const task = await workspace.addTask('Project flag task', {
                args: ['--project', 'CLI'],
            });

            // This previously failed with "unexpected argument found"
            const result = await workspace.runLotar([
                'sprint', 'add', '--sprint', '1', '--project', 'CLI', task.id,
            ]);

            expect(result.exitCode).toBe(0);
            expect(result.stdout).toContain('Attached sprint');
        } finally {
            await workspace.dispose();
        }
    });
});
