import { stat } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { createFixtureAgents, createGateFixture } from '../helpers/agent-fixtures.js';
import { callTool, extractToolPayload, withMcpClient } from '../helpers/mcp-harness.js';
import { startLotarServer } from '../helpers/server.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

type Surface = 'REST' | 'MCP';
type Patch = { status?: string; assignee?: string };
type Mutation = { ok: boolean; code?: string | number; task?: ReviewTask };
interface ReviewTask {
    status: string;
    assignee?: string;
    reporter?: string;
    history?: Array<{ actor?: string }>;
}
type Mutate = (id: string, patch: Patch) => Promise<Mutation>;
type Configure = (value: string, global?: boolean) => Promise<boolean>;

const CONFIG = `default.project: REVIEW
default.reporter: alice
auto.identity: false
issue.states: [Todo, InProgress, NeedsReview, Done]
issue.priorities: [Low, Medium, High]
issue.types: [Feature, Bug, Chore]
members: [alice, bob, carol]
`;
const ENV = { LOTAR_DEFAULT_REPORTER: '' };

function task(reporter?: string): string {
    return (
        '# Preserve this user-owned comment on rejected and no-op mutations.\n' +
        stringify({
            title: 'Private review-policy sentinel',
            status: 'NeedsReview',
            priority: 'Medium',
            type: 'Feature',
            ...(reporter === undefined ? {} : { reporter }),
            assignee: 'bob',
            created: '2026-01-01T00:00:00Z',
            modified: '2026-01-02T00:00:00Z',
            tags: [],
        })
    );
}

async function snapshot(workspace: SmokeWorkspace, id: string) {
    return {
        bytes: await workspace.readTaskYaml(id),
        mtimeMs: (await stat(workspace.getTaskFilePath(id))).mtimeMs,
    };
}

async function withSurface(
    surface: Surface,
    workspace: SmokeWorkspace,
    body: (mutate: Mutate, configure: Configure) => Promise<void>,
) {
    if (surface === 'MCP') {
        await withMcpClient(
            workspace,
            async (client) => {
                let requestId = 0;
                await body(
                    async (id, patch) => {
                        const response = await callTool(client, ++requestId, 'task_update', { id, patch });
                        if (response.error) {
                            return { ok: false, code: response.error.code };
                        }
                        // task_update uses protocol errors for validation, not result.isError.
                        expect(response.result?.isError).not.toBe(true);
                        return { ok: true, task: extractToolPayload(response) as ReviewTask };
                    },
                    async (value, global = false) => {
                        const response = await callTool(client, ++requestId, 'config_set', {
                            project: 'REVIEW',
                            global,
                            values: { 'review.reviewers': value },
                        });
                        if (response.error || response.result?.isError) return false;
                        expect(response.result).toBeDefined();
                        expect(response.result?.isError).not.toBe(true);
                        return true;
                    },
                );
            },
            { env: ENV },
        );
        return;
    }
    const server = await startLotarServer(workspace, { env: ENV });
    try {
        await body(
            async (id, patch) => {
                // Exercise the dedicated status route as well as the general update route.
                const route = patch.assignee === undefined ? 'status' : 'update';
                const response = await fetch(`${server.url}/api/tasks/${route}`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ id, ...patch }),
                });
                const payload = (await response.json()) as {
                    data?: ReviewTask;
                    error?: { code: string };
                };
                expect(response.status).toBe(response.ok ? 200 : 400);
                return { ok: response.ok, code: payload.error?.code, task: payload.data };
            },
            async (value, global = false) => {
                const response = await fetch(`${server.url}/api/config/set`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ project: 'REVIEW', global, values: { 'review.reviewers': value } }),
                });
                return response.ok;
            },
        );
    } finally {
        await server.stop();
    }
}

describe.each<Surface>(['REST', 'MCP'])('%s project review policy (DEV-128)', (surface) => {
    it('configures reviewers through the public configuration boundary and rejects invalid or global authority', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': CONFIG,
                '.tasks/REVIEW/config.yml': '{}\n',
                '.tasks/REVIEW/1.yml': task('bob'),
            },
        });
        try {
            await withSurface(surface, workspace, async (mutate, configure) => {
                const original = await workspace.read('.tasks/REVIEW/config.yml');
                const global = await workspace.read('.tasks/config.yml');
                expect(await configure('["@me"]')).toBe(false);
                expect(await workspace.read('.tasks/REVIEW/config.yml')).toBe(original);
                expect(await configure('[" Alice ", "alice"]')).toBe(true);
                const configured = await workspace.read('.tasks/REVIEW/config.yml');
                expect(parse(configured).review.reviewers).toEqual(['Alice']);
                expect((await mutate('REVIEW-1', { assignee: 'carol' })).ok).toBe(true);
                expect(await configure('Alice')).toBe(true);
                expect(await workspace.read('.tasks/REVIEW/config.yml')).toBe(configured);
                expect(await configure('[]')).toBe(true);
                const before = await snapshot(workspace, 'REVIEW-1');
                expect((await mutate('REVIEW-1', { status: 'Done' })).ok).toBe(false);
                expect(await snapshot(workspace, 'REVIEW-1')).toEqual(before);
                expect(await configure('alice', true)).toBe(false);
                expect(await workspace.read('.tasks/config.yml')).toBe(global);
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('allows a trimmed case-insensitive named reviewer for agent and unknown reporters without changing provenance', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': CONFIG,
                '.tasks/REVIEW/config.yml': 'review:\n  reviewers: ["  ALIce  "]\n',
                '.tasks/REVIEW/1.yml': task('@copilot'),
                '.tasks/REVIEW/2.yml': task(),
                '.tasks/REVIEW/3.yml': task('@copilot'),
                '.tasks/REVIEW/4.yml': task(),
            },
        });
        try {
            await withSurface(surface, workspace, async (mutate) => {
                for (const [id, patch] of [
                    ['REVIEW-1', { status: 'Done' }],
                    ['REVIEW-2', { status: 'Done' }],
                    ['REVIEW-3', { assignee: 'carol' }],
                    ['REVIEW-4', { assignee: 'carol' }],
                ] as const) {
                    const result = await mutate(id, patch);
                    expect(result.ok).toBe(true);
                    expect(result.task).toMatchObject(patch);
                    const stored = parse(await workspace.readTaskYaml(id)) as ReviewTask;
                    expect(stored).toMatchObject(patch);
                    expect(stored.reporter ?? null).toBe(['REVIEW-1', 'REVIEW-3'].includes(id) ? '@copilot' : null);
                    expect(stored.history).toHaveLength(1);
                    expect(stored.history?.[0]?.actor).toBe('alice');
                }
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('rejects unlisted actors without writes, permits no-ops, and revokes reviewers live', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': CONFIG,
                '.tasks/REVIEW/config.yml': 'review.reviewers: [carol]\n',
                '.tasks/OTHER/config.yml': 'review.reviewers: [alice]\n',
                '.tasks/REVIEW/1.yml': task('bob'),
                '.tasks/REVIEW/2.yml': task(),
            },
        });
        try {
            await withSurface(surface, workspace, async (mutate) => {
                for (const id of ['REVIEW-1', 'REVIEW-2']) {
                    const before = await snapshot(workspace, id);
                    for (const patch of [{ status: 'Done' }, { assignee: 'carol' }]) {
                        const denied = await mutate(id, patch);
                        expect(denied).toMatchObject({
                            ok: false,
                            code: surface === 'REST' ? 'INVALID_ARGUMENT' : -32602,
                        });
                        expect(await snapshot(workspace, id)).toEqual(before);
                    }
                    for (const patch of [{ status: 'NeedsReview' }, { assignee: 'bob' }]) {
                        expect((await mutate(id, patch)).ok).toBe(true);
                        expect(await snapshot(workspace, id)).toEqual(before);
                    }
                }

                // Both calls use the same server/client: no restart or watcher delay.
                await workspace.write('.tasks/REVIEW/config.yml', 'review.reviewers: [alice]\n');
                expect((await mutate('REVIEW-1', { assignee: 'carol' })).ok).toBe(true);
                const authorized = await snapshot(workspace, 'REVIEW-1');
                await workspace.write('.tasks/REVIEW/config.yml', 'review.reviewers: []\n');
                expect((await mutate('REVIEW-1', { status: 'Done' })).ok).toBe(false);
                expect((await mutate('REVIEW-1', { assignee: 'bob' })).ok).toBe(false);
                expect(await snapshot(workspace, 'REVIEW-1')).toEqual(authorized);

                // The original reporter remains authorized even with an empty list.
                await workspace.write('.tasks/REVIEW/config.yml', 'default.reporter: bob\nreview.reviewers: []\n');
                expect((await mutate('REVIEW-1', { status: 'Done' })).ok).toBe(true);
                const stored = parse(await workspace.readTaskYaml('REVIEW-1')) as ReviewTask;
                expect(stored.reporter).toBe('bob');
                expect(stored.history?.map((entry) => entry.actor)).toEqual(['alice', 'bob']);
            });
        } finally {
            await workspace.dispose();
        }
    });

    it('keeps absent and empty project reviewer lists reporter-only', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': CONFIG,
                '.tasks/REVIEW/1.yml': task('bob'),
            },
        });
        try {
            await withSurface(surface, workspace, async (mutate) => {
                const before = await snapshot(workspace, 'REVIEW-1');
                for (const config of ['{}\n', 'review.reviewers: []\n']) {
                    await workspace.write('.tasks/REVIEW/config.yml', config);
                    expect((await mutate('REVIEW-1', { status: 'Done' })).ok).toBe(false);
                    expect((await mutate('REVIEW-1', { assignee: 'carol' })).ok).toBe(false);
                    expect(await snapshot(workspace, 'REVIEW-1')).toEqual(before);
                }
                await workspace.write('.tasks/REVIEW/config.yml', 'default.reporter: bob\n');
                expect((await mutate('REVIEW-1', { assignee: 'carol' })).ok).toBe(true);
                expect((await mutate('REVIEW-1', { status: 'Done' })).ok).toBe(true);
            });
        } finally {
            await workspace.dispose();
        }
    });
});

describe('REST reviewer active-job boundary (DEV-128)', () => {
    it('does not let an authorized reviewer bypass a running agent job', async ({ expect }) => {
        const gate = await createGateFixture('lotar-smoke-review-gate-');
        try {
            const agents = await createFixtureAgents({
                gated: { script: gate.script, args: gate.agentArgs({ mode: 'copilot', label: 'review' }) },
            });
            try {
                const workspace = await SmokeWorkspace.create({
                    seedFiles: {
                        '.tasks/config.yml':
                            CONFIG + '\nagent:\n  logs_dir: .logs\n  worktree:\n    enabled: false\n' + agents.yaml,
                        '.tasks/REVIEW/config.yml': 'review.reviewers: [alice]\n',
                        '.tasks/REVIEW/1.yml': task('@copilot'),
                    },
                });
                try {
                    const server = await startLotarServer(workspace, { env: ENV });
                    try {
                        const started = await fetch(`${server.url}/api/jobs`, {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({
                                ticket_id: 'REVIEW-1',
                                agent: 'gated',
                                prompt: 'Wait for release.',
                            }),
                        });
                        expect(started.ok).toBe(true);
                        const payload = (await started.json()) as { data: { job: { id: string } } };
                        await gate.waitForStarted('review');
                        await expect
                            .poll(
                                async () => {
                                    const response = await fetch(`${server.url}/api/jobs`);
                                    expect(response.ok).toBe(true);
                                    const jobs = (await response.json()) as {
                                        data: { jobs: Array<{ id: string; status: string }> };
                                    };
                                    return jobs.data.jobs.find((job) => job.id === payload.data.job.id)?.status;
                                },
                                { timeout: 10_000 },
                            )
                            .toBe('running');
                        const before = await snapshot(workspace, 'REVIEW-1');
                        for (const patch of [{ status: 'Done' }, { assignee: 'carol' }]) {
                            const response = await fetch(`${server.url}/api/tasks/update`, {
                                method: 'POST',
                                headers: { 'Content-Type': 'application/json' },
                                body: JSON.stringify({ id: 'REVIEW-1', ...patch }),
                            });
                            expect(response.status).toBe(400);
                            const rejected = (await response.json()) as { error: { code: string; message: string } };
                            expect(rejected.error.code).toBe('INVALID_ARGUMENT');
                            expect(rejected.error.message).toContain('active agent job');
                            expect(await snapshot(workspace, 'REVIEW-1')).toEqual(before);
                        }
                    } finally {
                        try {
                            await gate.drain();
                        } finally {
                            await server.stop();
                        }
                    }
                } finally {
                    await workspace.dispose();
                }
            } finally {
                await agents.dispose();
            }
        } finally {
            await gate.dispose();
        }
    });
});
