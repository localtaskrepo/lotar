import fs from 'fs-extra';
import path from 'node:path';
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

    it('writes minimal global config when only defaults are required', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['add', 'Defaults Only Smoke Test']);

            const config = await workspace.read('.tasks/config.yml');

            expect(config).toContain('default:\n  project:');
            expect(config).not.toContain('server:');
            expect(config).not.toContain('issue:');
        } finally {
            await workspace.dispose();
        }
    });

    it('deletes sprints via the CLI and cleans task memberships', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const first = await workspace.addTask('CLI Sprint Delete Task A');
            const second = await workspace.addTask('CLI Sprint Delete Task B');

            await workspace.runLotar(['sprint', 'create', '--label', 'CLI Delete Smoke Sprint']);
            await workspace.runLotar(['sprint', 'add', first.id, second.id, '--sprint', '1']);

            const result = await workspace.runLotar([
                'sprint',
                'delete',
                '1',
                '--force',
                '--cleanup-missing',
            ]);

            expect(result.stdout).toContain('Deleted');
            const sprintPath = path.join(workspace.tasksDir, '@sprints', '1.yml');
            expect(await fs.pathExists(sprintPath)).toBe(false);

            const firstYaml = parse(await workspace.readTaskYaml(first.id)) as Record<string, any>;
            const secondYaml = parse(await workspace.readTaskYaml(second.id)) as Record<string, any>;

            const firstMembership = Array.isArray(firstYaml.sprints) ? firstYaml.sprints : [];
            const secondMembership = Array.isArray(secondYaml.sprints) ? secondYaml.sprints : [];

            expect(firstMembership).not.toContain(1);
            expect(secondMembership).not.toContain(1);
        } finally {
            await workspace.dispose();
        }
    });
});
