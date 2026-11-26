import path from 'node:path';
import { describe, it } from 'vitest';
import type { Page } from 'playwright';
import { startLotarServer } from '../helpers/server.js';
import { withBrowser } from '../helpers/ui.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const ARTIFACTS = path.join(process.cwd(), 'target/smoke-artifacts/task-timeline-conversation');
const T3_LINK = 'https://t3.test/threads/env-1/thread-1';

type Line = Record<string, unknown>;

/** A harnessmux event log line (format 1). */
function event(session: string, seq: number, ts: string, turn: number | null, body: Line): Line {
    return { v: 1, ts, session, seq, ...(turn === null ? {} : { turn }), ...body };
}

/** One turn of a harnessmux session: its prompt, then the body's events, then its end. */
function turn(session: string, seq: number, n: number, day: string, prompt: string, body: Line[]): Line[] {
    const at = (second: number) => `${day}:${String(second).padStart(2, '0')}Z`;
    return [
        event(session, seq, at(1), n, { type: 'turn.started', prompt }),
        ...body.map((line, index) => event(session, seq + 1 + index, at(2 + index), n, line)),
        event(session, seq + 1 + body.length, at(2 + body.length), n, {
            type: 'turn.finished',
            stop_reason: 'end_turn',
        }),
    ];
}

const tools = (id: string, title: string, output: string): Line[] => [
    { type: 'tool.started', id, title, kind: 'execute', raw_input: { command: title } },
    { type: 'tool.finished', id, status: 'completed', output },
];

const started = (session: string, seq: number, ts: string, agent: string, how: string): Line =>
    event(session, seq, ts, null, { type: 'session.started', agent, cwd: '/work', how, agent_session: `${agent}-1` });

/** A job's saved log: the job ran turn `turnNo` of `session` from `from` to `to`. */
function jobLog(
    id: string,
    ticket: string,
    runner: string,
    session: string,
    turnNo: number,
    from: string,
    to: string,
    summary: string,
    link?: string,
): string {
    return [
        { type: 'header', job_id: id, ticket_id: ticket, runner, created_at: from, agent_profile: 'fix' },
        { type: 'event', kind: 'agent_job_started', at: from },
        {
            type: 'event',
            kind: 'agent_job_transcript',
            at: from,
            message: `Turn ${turnNo} of the harnessmux session ${session}.`,
            transcript: {
                source: 'harnessmux',
                session,
                agent: runner,
                turn: turnNo,
                format: 1,
                ...(link ? { link } : {}),
            },
        },
        { type: 'event', kind: 'agent_job_message', at: to, message: summary },
        { type: 'status', status: 'completed', at: to, exit_code: 0, summary },
    ]
        .map((line) => JSON.stringify(line))
        .join('\n');
}

const jsonl = (lines: Line[]) => `${lines.map((line) => JSON.stringify(line)).join('\n')}\n`;

describe.concurrent('Task timeline conversation (DEV-909)', () => {
    it('reads a job conversation from its harnessmux event log only when it is opened', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'ui-timeline-conversation-' });
        try {
            const claude = await workspace.addTask('A Claude turn');
            const reused = await workspace.addTask('A reused session');
            const t3 = await workspace.addTask('T3 Code jobs');
            const gone = await workspace.addTask('An expired session');
            const sessions = '.harnessmux/state/sessions';

            // A Claude session: a thought, a tool call and a reply, streamed.
            await workspace.write(
                `${sessions}/claude-one.events.jsonl`,
                jsonl([
                    started('claude-one', 1, '2026-11-20T10:00:00Z', 'claude', 'new'),
                    ...turn('claude-one', 2, 1, '2026-11-20T10:00', 'Fix the parser.', [
                        { type: 'thought.delta', text: 'The parser drops ' },
                        { type: 'thought.delta', text: 'the last line.' },
                        ...tools('t1', 'cargo test -p parser', 'test result: ok. 12 passed'),
                        { type: 'message.delta', text: 'Fixed the parser ' },
                        { type: 'message.delta', text: 'and added a test.' },
                        { type: 'message', text: 'Fixed the parser and added a test.' },
                    ]),
                ]),
            );
            await workspace.write(
                `.tasks/@agent-logs/job-20261120T100000.000Z-1.jsonl`,
                jobLog(
                    'job-20261120T100000.000Z-1',
                    claude.id,
                    'claude',
                    'claude-one',
                    1,
                    '2026-11-20T10:00:00Z',
                    '2026-11-20T10:01:00Z',
                    'Parser fixed.',
                ),
            );

            // One session, two jobs: the review (turn 1) and the rework that continued it (turn 2).
            await workspace.write(
                `${sessions}/reuse.events.jsonl`,
                jsonl([
                    started('reuse', 1, '2026-11-20T11:00:00Z', 'claude', 'new'),
                    ...turn('reuse', 2, 1, '2026-11-20T11:00', 'Review the change.', [
                        { type: 'message', text: 'The test is missing.' },
                    ]),
                    started('reuse', 5, '2026-11-20T12:00:00Z', 'claude', 'resumed'),
                    ...turn('reuse', 6, 2, '2026-11-20T12:00', 'Rework it: add the test.', [
                        { type: 'message', text: 'Added the test.' },
                    ]),
                ]),
            );
            await workspace.write(
                `.tasks/@agent-logs/job-20261120T110000.000Z-2.jsonl`,
                jobLog(
                    'job-20261120T110000.000Z-2',
                    reused.id,
                    'claude',
                    'reuse',
                    1,
                    '2026-11-20T11:00:00Z',
                    '2026-11-20T11:01:00Z',
                    'Reviewed.',
                ),
            );
            await workspace.write(
                `.tasks/@agent-logs/job-20261120T120000.000Z-3.jsonl`,
                jobLog(
                    'job-20261120T120000.000Z-3',
                    reused.id,
                    'claude',
                    'reuse',
                    2,
                    '2026-11-20T12:00:00Z',
                    '2026-11-20T12:01:00Z',
                    'Reworked.',
                ),
            );

            // T3 Code: a session with tool events, and one harnessmux 0.2.0 wrote without them.
            await workspace.write(
                `${sessions}/t3-tools.events.jsonl`,
                jsonl([
                    started('t3-tools', 1, '2026-11-20T13:00:00Z', 't3', 'new'),
                    ...turn('t3-tools', 2, 1, '2026-11-20T13:00', 'Fix the docs.', [
                        ...tools('t2', 'npm run lint', 'All files pass.'),
                        { type: 'message', text: 'The docs are fixed.' },
                    ]),
                ]),
            );
            await workspace.write(
                `${sessions}/t3-plain.events.jsonl`,
                jsonl([
                    started('t3-plain', 1, '2026-11-20T14:00:00Z', 't3', 'new'),
                    ...turn('t3-plain', 2, 1, '2026-11-20T14:00', 'Fix the page.', [
                        { type: 'message', text: 'The page is fixed.' },
                    ]),
                ]),
            );
            await workspace.write(
                `.tasks/@agent-logs/job-20261120T130000.000Z-4.jsonl`,
                jobLog(
                    'job-20261120T130000.000Z-4',
                    t3.id,
                    't3',
                    't3-tools',
                    1,
                    '2026-11-20T13:00:00Z',
                    '2026-11-20T13:01:00Z',
                    'Docs fixed.',
                    T3_LINK,
                ),
            );
            await workspace.write(
                `.tasks/@agent-logs/job-20261120T140000.000Z-5.jsonl`,
                jobLog(
                    'job-20261120T140000.000Z-5',
                    t3.id,
                    't3',
                    't3-plain',
                    1,
                    '2026-11-20T14:00:00Z',
                    '2026-11-20T14:01:00Z',
                    'Page fixed.',
                    T3_LINK,
                ),
            );

            // A job whose session's event log is gone.
            await workspace.write(
                `.tasks/@agent-logs/job-20261120T150000.000Z-6.jsonl`,
                jobLog(
                    'job-20261120T150000.000Z-6',
                    gone.id,
                    'claude',
                    'removed',
                    1,
                    '2026-11-20T15:00:00Z',
                    '2026-11-20T15:01:00Z',
                    'Done long ago.',
                ),
            );

            const server = await startLotarServer(workspace, {
                env: { HARNESSMUX_HOME: path.join(workspace.root, '.harnessmux') },
            });
            try {
                await withBrowser({ context: { viewport: { width: 390, height: 844 } } }, async (context) => {
                    const page = await context.newPage();
                    const transcriptRequests: string[] = [];
                    page.on('request', (request) => {
                        if (request.url().includes('/api/jobs/transcript')) transcriptRequests.push(request.url());
                    });

                    /** Opens the ticket's Timeline, then the thread of its `index`th job (newest first). */
                    const openThread = async (ticket: string, index: number) => {
                        await page.goto(`${server.url}/task/${ticket}`, { waitUntil: 'domcontentloaded' });
                        await page.getByRole('tab', { name: 'Timeline', exact: true }).click();
                        const run = page.getByTestId('timeline-job').nth(index);
                        await run.getByRole('button', { name: 'Show thread' }).click();
                        await expect.poll(() => run.locator('.log-list').count()).toBeGreaterThan(0);
                        return run;
                    };
                    const openConversation = async (run: ReturnType<Page['getByTestId']>) => {
                        await run.getByRole('button', { name: 'Show conversation' }).click();
                        const panel = run.getByRole('region', { name: 'Conversation' });
                        await expect.poll(() => panel.isVisible()).toBe(true);
                        return panel;
                    };
                    /**
                     * The job's whole entry in light and dark at 1920, 1440 and 390 px; the Timeline scrolls
                     * within 520px, which would cut it off. Ends back at phone width in light mode.
                     */
                    const shot = async (name: string, run: ReturnType<Page['getByTestId']>) => {
                        const unclipped = await page.addStyleTag({
                            content: '.task-panel__history-scroll { max-height: none !important; }',
                        });
                        for (const colorScheme of ['light', 'dark'] as const) {
                            await page.emulateMedia({ colorScheme });
                            for (const width of [1920, 1440, 390]) {
                                await page.setViewportSize({ width, height: 844 });
                                await run.screenshot({
                                    path: path.join(ARTIFACTS, `${name}-${colorScheme}-${width}.png`),
                                    animations: 'disabled',
                                });
                            }
                        }
                        await page.emulateMedia({ colorScheme: 'light' });
                        await unclipped.evaluate((style) => (style as Element).remove());
                    };

                    // A Claude turn: the thread opens without asking for the conversation.
                    const first = await openThread(claude.id, 0);
                    await page.waitForLoadState('networkidle');
                    expect(transcriptRequests).toEqual([]);
                    await shot('before-thread', first);
                    const panel = await openConversation(first);
                    await expect.poll(() => panel.textContent()).toContain('Fixed the parser and added a test.');
                    const text = (await panel.textContent()) ?? '';
                    for (const part of [
                        'Turn 1 of 1 in the claude session claude-one',
                        'Fix the parser.',
                        'cargo test -p parser',
                    ]) {
                        expect(text).toContain(part);
                    }
                    expect(text).not.toContain('Tool calls are in T3');
                    expect(transcriptRequests).toHaveLength(1);
                    await shot('after-claude', first);

                    // A reused session: the rework job reads turn 2 only, the review job turn 1.
                    const rework = await openThread(reused.id, 0);
                    const second = await openConversation(rework);
                    await expect.poll(() => second.textContent()).toContain('Added the test.');
                    expect(await second.textContent()).toContain('Turn 2 of 2');
                    expect(await second.textContent()).not.toContain('The test is missing.');
                    await shot('after-reused', rework);

                    // T3 Code: both link to the thread; the one without tool events says where they are.
                    const plain = await openThread(t3.id, 0);
                    const plainPanel = await openConversation(plain);
                    await expect.poll(() => plainPanel.textContent()).toContain('The page is fixed.');
                    expect(await plainPanel.textContent()).toContain('Tool calls are in T3');
                    expect(await plainPanel.getByRole('link', { name: 'Open in T3' }).getAttribute('href')).toBe(
                        T3_LINK,
                    );
                    await shot('after-t3-without-tools', plain);
                    const withTools = page.getByTestId('timeline-job').nth(1);
                    await withTools.getByRole('button', { name: 'Show thread' }).click();
                    const toolsPanel = await openConversation(withTools);
                    await expect.poll(() => toolsPanel.textContent()).toContain('npm run lint');
                    expect(await toolsPanel.textContent()).not.toContain('Tool calls are in T3');
                    expect(await toolsPanel.getByRole('link', { name: 'Open in T3' }).count()).toBe(1);

                    // An expired session: the job's summary stays.
                    const old = await openThread(gone.id, 0);
                    const expired = await openConversation(old);
                    await expect
                        .poll(() => expired.textContent())
                        .toContain('The conversation is no longer on this machine.');
                    await shot('after-expired', old);
                });
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
