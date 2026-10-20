import { stat, utimes, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const WORKSPACE_CONFIG = `default.project: TEST
default.reporter: alpha
auto.identity: false
issue.states: [Todo, InProgress, NeedsReview, Done]
issue.priorities: [Low, Medium, High, Critical]
issue.types: [Feature, Bug, Chore]
members: [alpha, bravo, carol, david]
`;

const REVIEW_TASK = `title: Live identity review
status: NeedsReview
priority: Medium
type: Feature
reporter: bravo
assignee: bravo
created: '2026-01-01T00:00:00Z'
modified: '2026-01-02T00:00:00Z'
tags: []
`;

describe('API live identity', () => {
    it('refreshes externally edited identity for whoami and review authorization without restarting', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': WORKSPACE_CONFIG,
                '.tasks/TEST/1.yml': REVIEW_TASK,
            },
        });

        try {
            const server = await startLotarServer(workspace, {
                env: { LOTAR_DEFAULT_REPORTER: '' },
            });

            try {
                const whoami = async () => {
                    const response = await fetch(`${server.url}/api/whoami`);
                    expect(response.status).toBe(200);
                    const payload = await response.json() as { data: string };
                    return payload.data;
                };
                const transition = () => fetch(`${server.url}/api/tasks/status`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ id: 'TEST-1', status: 'Done' }),
                });

                expect(await whoami()).toBe('alpha');
                const rejected = await transition();
                expect(rejected.status).toBe(400);
                const rejection = await rejected.json() as {
                    error: { code: string; message: string };
                };
                expect(rejection.error.code).toBe('INVALID_ARGUMENT');
                expect(rejection.error.message).toContain("Only reporter 'bravo'");
                expect(await workspace.read('.tasks/TEST/1.yml')).toBe(REVIEW_TASK);

                await workspace.write('.tasks/config.yml',
                    WORKSPACE_CONFIG.replace('default.reporter: alpha', 'default.reporter: bravo'));
                expect(await whoami()).toBe('bravo');

                const accepted = await transition();
                expect(accepted.status).toBe(200);
                const updated = await accepted.json() as {
                    data: { status: string; history: Array<{ actor?: string }> };
                };
                expect(updated.data.status).toBe('Done');
                expect(updated.data.history).toHaveLength(1);
                expect(updated.data.history[0]?.actor).toBe('bravo');

                const projectConfig = '.tasks/TEST/config.yml';
                const projectPath = path.join(workspace.root, projectConfig);
                const carolConfig = 'default.reporter: carol\nauto.identity: false\n';
                const davidConfig = carolConfig.replace('carol', 'david');
                await workspace.write(projectConfig, carolConfig);
                const fixedTime = new Date('2026-01-01T00:00:00Z');
                await utimes(projectPath, fixedTime, fixedTime);
                expect(await whoami()).toBe('carol');

                // Keep size, inode, and mtime stable to reject caches keyed only by those values.
                const before = await stat(projectPath);
                expect(Buffer.byteLength(davidConfig)).toBe(Buffer.byteLength(carolConfig));
                await writeFile(projectPath, davidConfig);
                await utimes(projectPath, before.atime, before.mtime);
                expect(await whoami()).toBe('david');
                const after = await stat(projectPath);
                expect(after.size).toBe(before.size);
                expect(after.ino).toBe(before.ino);
                expect(after.mtimeMs).toBe(before.mtimeMs);

                await workspace.remove(projectConfig);
                expect(await whoami()).toBe('bravo');
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
