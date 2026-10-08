import fs from 'fs-extra';
import path from 'node:path';
import { describe, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('shared REST domain error classification', () => {
    it('distinguishes bad input from missing tasks and sprints at the actual HTTP boundary', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({ name: 'rest-domain-errors-' });
        try {
            const task = await workspace.addTask('Error classification fixture', { args: ['--project=ERRS', '--reporter=Smoke'] });
            const server = await startLotarServer(workspace);
            try {
                const cases: Array<[string, Record<string, unknown> | undefined, number]> = [
                    ['/api/tasks/get?id=bad', undefined, 400],
                    ['/api/tasks/get?id=ERRS-999', undefined, 404],
                    ['/api/tasks/update', { id: task.id, status: 'NotAWorkflowState' }, 400],
                    ['/api/tasks/update', { id: 'ERRS-999', title: 'Missing target' }, 404],
                    ['/api/tasks/status', { id: task.id, status: 'NotAWorkflowState' }, 400],
                    ['/api/tasks/status', { id: 'ERRS-999', status: 'Done' }, 404],
                    ['/api/tasks/delete', { id: 'ERRS-999' }, 404],
                    ['/api/sprints/summary?sprint=0', undefined, 400],
                    ['/api/sprints/summary?sprint=999', undefined, 404],
                    ['/api/sprints/burndown?sprint=0', undefined, 400],
                    ['/api/sprints/burndown?sprint=999', undefined, 404],
                    ['/api/sprints/update', { sprint: 0, label: 'Invalid' }, 400],
                    ['/api/sprints/update', { sprint: 999, label: 'Missing' }, 404],
                    ['/api/sprints/delete', { sprint: 999 }, 404],
                    ['/api/tasks/add', { project: 'ERRS', title: 'Missing sprint reference', sprints: [999] }, 404],
                    ['/api/tasks/add', { project: 'ERRS', title: 'Invalid sprint reference', sprints: [0] }, 400],
                ];
                for (const [route, body, status] of cases) {
                    const response = await fetch(`${server.url}${route}`, {
                        method: body ? 'POST' : 'GET',
                        headers: body ? { 'Content-Type': 'application/json' } : {},
                        body: body ? JSON.stringify(body) : undefined,
                    });
                    expect(response.status, route).toBe(status);
                    const payload = await response.json() as { error: { code: string; message: string } };
                    expect(payload.error.code, route).toBe(status === 404 ? 'NOT_FOUND' : 'INVALID_ARGUMENT');
                    expect(payload.error.message, route).toEqual(expect.any(String));
                    expect(payload.error.message.length, route).toBeGreaterThan(0);
                }
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('does not misclassify corrupt persisted sprint data as client input', async ({ expect }) => {
        const workspace = await SmokeWorkspace.create({
            name: 'rest-corrupt-sprint-',
            seedFiles: { '.tasks/@sprints/1.yml': 'plan: [\n' },
        });
        try {
            const file = path.join(workspace.tasksDir, '@sprints/1.yml');
            const before = await fs.readFile(file, 'utf8');
            const server = await startLotarServer(workspace);
            try {
                for (const route of ['/api/sprints/summary?sprint=1', '/api/sprints/burndown?sprint=1']) {
                    const response = await fetch(`${server.url}${route}`);
                    expect(response.status).toBe(500);
                    const payload = await response.json() as { error: { code: string; message: string } };
                    expect(payload.error.code).toBe('INTERNAL');
                    expect(payload.error.message.length).toBeGreaterThan(0);
                }
                expect(await fs.readFile(file, 'utf8')).toBe(before);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
