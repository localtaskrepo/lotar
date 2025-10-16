import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('CLI smoke harness', () => {
    it('runs lotar --version inside an isolated workspace', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const result = await workspace.runLotar(['--version']);
            expect(result.stdout).toMatch(/lotar/i);
        } finally {
            await workspace.dispose();
        }
    });

    it('creates a task and lists it via the CLI', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const created = await workspace.addTask('Smoke CLI Task');

            expect(created.id).toMatch(/^[A-Z0-9_-]+-\d+$/);
            const taskYaml = await workspace.readTaskYaml(created.id);
            const task = parse(taskYaml) as Record<string, unknown>;

            expect(task.title).toBe('Smoke CLI Task');
            expect(task.status).toBe('Todo');
            expect(task.priority).toBe('Medium');

            const list = await workspace.runLotar(['list']);
            expect(list.stdout).toContain(created.id);
            expect(list.stdout).toContain('Smoke CLI Task');

            const files = await workspace.listTaskFiles();
            expect(files).toHaveLength(1);
            expect(files[0]).toBe(created.filePath);
        } finally {
            await workspace.dispose();
        }
    });

    it('updates task status via CLI commands', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const created = await workspace.addTask('Status Update Task');

            await workspace.runLotar(['status', created.id, 'in_progress']);

            const updatedYaml = await workspace.readTaskYaml(created.id);
            const updated = parse(updatedYaml) as Record<string, unknown>;

            expect(updated.status).toBe('InProgress');
            expect(updated.title).toBe('Status Update Task');
        } finally {
            await workspace.dispose();
        }
    });
});
