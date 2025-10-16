import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

interface TaskResponse {
    readonly id: string;
    readonly title: string;
    readonly status: string;
    readonly priority: string;
    readonly assignee?: string | null;
    readonly tags?: string[];
}

interface ListResponse {
    readonly data: TaskResponse[];
    readonly meta?: { readonly count?: number };
}

interface CreateResponse {
    readonly data: TaskResponse;
}

describe.concurrent('API smoke harness', () => {
    it('lists tasks via the REST API', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const first = await workspace.addTask('API List Task A');
            const second = await workspace.addTask('API List Task B');

            await workspace.runLotar(['status', second.id, 'done']);

            const server = await startLotarServer(workspace);

            try {
                const baseUrl = `${server.url}/api/tasks/list`;
                const listResponse = await fetch(baseUrl);
                expect(listResponse.ok).toBe(true);

                const listPayload = (await listResponse.json()) as ListResponse;
                expect(Array.isArray(listPayload.data)).toBe(true);
                expect(listPayload.data.length).toBeGreaterThanOrEqual(2);

                const titles = listPayload.data.map((task) => task.title);
                expect(titles).toContain('API List Task A');
                expect(titles).toContain('API List Task B');

                const doneResponse = await fetch(`${baseUrl}?status=done`);
                expect(doneResponse.ok).toBe(true);

                const donePayload = (await doneResponse.json()) as ListResponse;
                const doneTitles = donePayload.data.map((task) => task.title);
                expect(doneTitles).toContain('API List Task B');
                expect(doneTitles).not.toContain('API List Task A');

                const metaCount = donePayload.meta?.count ?? donePayload.data.length;
                expect(metaCount).toBeGreaterThanOrEqual(1);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('creates tasks via the REST API', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const server = await startLotarServer(workspace);

            try {
                const createResponse = await fetch(`${server.url}/api/tasks/add`, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify({
                        title: 'API Created Task',
                        assignee: 'api-smoke@example.com',
                        tags: ['api', 'smoke'],
                    }),
                });

                expect(createResponse.status).toBe(201);
                const createdPayload = (await createResponse.json()) as CreateResponse;
                expect(createdPayload.data.title).toBe('API Created Task');
                expect(createdPayload.data.id).toMatch(/^[A-Z0-9_-]+-\d+$/);

                const listResponse = await fetch(`${server.url}/api/tasks/list`);
                expect(listResponse.ok).toBe(true);

                const listPayload = (await listResponse.json()) as ListResponse;
                const createdTask = listPayload.data.find((task) => task.id === createdPayload.data.id);

                expect(createdTask).toBeDefined();
                expect(createdTask?.assignee).toBe('api-smoke@example.com');
                expect(createdTask?.tags ?? []).toContain('api');
                expect(createdTask?.tags ?? []).toContain('smoke');
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
