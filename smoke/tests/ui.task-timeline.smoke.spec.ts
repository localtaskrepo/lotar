import { relative } from 'node:path';
import { describe, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { startLotarServer } from '../helpers/server.js';
import { withPage } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const JOB_ID = 'job-20261113T110000.000Z-1';

/** A job of an earlier server run: nothing but its saved log is left. */
function savedJobLog(ticketId: string): string {
    return [
        {
            type: 'header',
            job_id: JOB_ID,
            ticket_id: ticketId,
            runner: 'claude',
            created_at: '2026-11-13T11:00:00Z',
            model: 'sonnet',
            agent_profile: 'fix',
        },
        { type: 'event', kind: 'agent_job_started', at: '2026-11-13T11:00:00Z' },
        { type: 'event', kind: 'agent_job_input', at: '2026-11-13T11:00:01Z', message: 'Work the ticket.' },
        {
            type: 'event',
            kind: 'agent_job_message',
            at: '2026-11-13T11:02:00Z',
            message: 'I read the code and fixed it.',
        },
        {
            type: 'status',
            status: 'completed',
            at: '2026-11-13T11:03:05Z',
            exit_code: 0,
            summary: 'Fixed the bug and added a test.',
            usage: { cost_usd: 0.42 },
        },
    ]
        .map((line) => JSON.stringify(line))
        .join('\n');
}

describe.concurrent('Task timeline (DEV-518)', () => {
    it('lists changes, comments and a saved agent run, whose thread opens on demand', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'ui-task-timeline-' });
        try {
            const created = await workspace.addTask('Timeline fixture');
            const task = parse(await workspace.readTaskYaml(created.id));
            task.comments = [{ date: '2026-11-13T12:00:00Z', author: 'mallox', text: 'Looks good.' }];
            task.history = [
                {
                    at: '2026-11-13T10:00:00Z',
                    actor: 'mallox',
                    changes: [{ field: 'assignee', old: '', new: '@fix' }],
                },
            ];
            await workspace.write(relative(workspace.root, workspace.getTaskFilePath(created.id)), stringify(task));
            await workspace.write(`.tasks/@agent-logs/${JOB_ID}.jsonl`, savedJobLog(created.id));

            const server = await startLotarServer(workspace);
            try {
                const jobs = await (await fetch(`${server.url}/api/jobs?ticket_id=${created.id}`)).json();
                expect(jobs.data.jobs.map((job: { id: string; status: string }) => [job.id, job.status])).toEqual([
                    [JOB_ID, 'completed'],
                ]);
                const logs = await (await fetch(`${server.url}/api/jobs/logs?id=${JOB_ID}`)).json();
                expect(logs.data.events.map((event: { sequence: number }) => event.sequence)).toEqual([1, 2, 3]);

                await withPage('about:blank', async (page) => {
                    await page.goto(`${server.url}/task/${created.id}`);
                    await page.getByRole('tab', { name: 'Timeline', exact: true }).click();
                    const items = page.locator('.timeline__item');
                    await expect
                        .poll(() => items.evaluateAll((all) => all.map((item) => item.dataset.kind)))
                        .toEqual(['comment', 'job', 'change']);
                    const run = page.getByTestId('timeline-job');
                    const runText = await run.textContent();
                    for (const part of ['completed', 'claude · sonnet', '3m 05s', '$0.42', 'Fixed the bug']) {
                        expect(runText).toContain(part);
                    }

                    // The profile is the run's author.
                    expect(await page.locator('.timeline__item--job .task-panel__history-actor').textContent()).toBe(
                        '@fix',
                    );

                    await run.getByRole('button', { name: 'Show thread' }).click();
                    await expect
                        .poll(() => run.locator('.log-list').textContent())
                        .toContain('I read the code and fixed it.');

                    const filter = page.getByRole('group', { name: 'Filter timeline' });
                    await filter.getByRole('button', { name: 'Agent runs', exact: true }).click();
                    await expect.poll(() => items.count()).toBe(2);
                    expect(await filter.getByRole('status').textContent()).toContain('1 of 3 entries hidden.');
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
