import fs from 'fs-extra';
import { Buffer } from 'node:buffer';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { SmokeWorkspace } from '../helpers/workspace.js';

const stdoutText = (value: unknown): string => {
    if (typeof value === 'string') {
        return value;
    }

    if (value instanceof Uint8Array) {
        return Buffer.from(value).toString('utf8');
    }

    if (Array.isArray(value)) {
        return value.map((entry) => stdoutText(entry)).join('');
    }

    return value === undefined || value === null ? '' : String(value);
};

interface TimeInStatusResponse {
    status: string;
    items: Array<{
        id: string;
        items: Array<{
            status: string;
            seconds: number;
        }>;
    }>;
}

describe.concurrent('CLI advanced smoke scenarios', () => {
    it('supports operations in a detached HEAD worktree', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.initGit();
            const initial = await workspace.addTask('Detached baseline task');
            await workspace.commitAll('record baseline task');

            await workspace.runGit(['checkout', '--detach', 'HEAD']);
            const detached = await workspace.addTask('Detached head task');

            const list = await workspace.runLotar(['list']);
            const listOutput = stdoutText(list.stdout);
            expect(listOutput).toContain(initial.id);
            expect(listOutput).toContain(detached.id);

            const status = await workspace.runGit(['status', '--short']);
            expect(stdoutText(status.stdout)).toContain(path.basename(detached.filePath));
        } finally {
            await workspace.dispose();
        }
    });

    it('reads updated metadata with uncommitted changes', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.initGit();
            const created = await workspace.addTask('Original title smoke task');

            const source = await workspace.readTaskYaml(created.id);
            const payload = parse(source) as Record<string, unknown>;
            payload.title = 'Edited without commit';
            await fs.writeFile(created.filePath, stringify(payload));

            const list = await workspace.runLotar(['list']);
            const listOutput = stdoutText(list.stdout);
            expect(listOutput).toContain('Edited without commit');
            expect(listOutput).not.toContain('Original title smoke task');
        } finally {
            await workspace.dispose();
        }
    });

    it('operates correctly when the default branch is renamed away from main', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.initGit();
            await workspace.runGit(['branch', '-m', 'trunk']);

            const head = await workspace.runGit(['rev-parse', '--abbrev-ref', 'HEAD']);
            expect(stdoutText(head.stdout).trim()).toBe('trunk');

            const trunkTask = await workspace.addTask('Trunk branch default task');
            await workspace.commitAll('record trunk baseline');

            await workspace.runGit(['checkout', '-b', 'feature/non-default-base']);
            const featureTask = await workspace.addTask('Feature branch task with trunk default');
            await workspace.commitAll('record feature task');

            const list = await workspace.runLotar(['list']);
            const listOutput = stdoutText(list.stdout);
            expect(listOutput).toContain(trunkTask.id);
            expect(listOutput).toContain(featureTask.id);
        } finally {
            await workspace.dispose();
        }
    });

    it('aggregates multi-author history in time-in-status stats', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.initGit();
            const projectDir = path.join(workspace.tasksDir, 'TEST');
            await fs.ensureDir(projectDir);
            const taskPath = path.join(projectDir, '1.yml');

            const writeTask = async (status: string, modified: string) => {
                await fs.writeFile(
                    taskPath,
                    [
                        'title: Timeline',
                        `status: ${status}`,
                        'priority: Medium',
                        'type: Feature',
                        'assignee: ',
                        'reporter: ',
                        'created: 2025-08-01T10:00:00Z',
                        `modified: ${modified}`,
                        '',
                    ].join('\n'),
                );
            };

            const commit = async (message: string, name: string, email: string, date: string) => {
                await workspace.runGit(['add', '.']);
                await workspace.runGit(
                    ['commit', '-m', message],
                    {
                        env: {
                            GIT_AUTHOR_NAME: name,
                            GIT_AUTHOR_EMAIL: email,
                            GIT_COMMITTER_NAME: name,
                            GIT_COMMITTER_EMAIL: email,
                            GIT_AUTHOR_DATE: date,
                            GIT_COMMITTER_DATE: date,
                        },
                    },
                );
            };

            await writeTask('Todo', '2025-08-01T10:00:00Z');
            await commit('add timeline', 'Alice', 'alice@example.com', '2025-08-01T10:00:00Z');

            await writeTask('InProgress', '2025-08-10T09:00:00Z');
            await commit('progress', 'Bob', 'bob@example.com', '2025-08-10T09:00:00Z');

            await writeTask('Done', '2025-08-17T12:00:00Z');
            await commit('done', 'Alice', 'alice@example.com', '2025-08-17T12:00:00Z');

            const stats = await workspace.runLotar([
                '--format',
                'json',
                'stats',
                'time-in-status',
                '--since',
                '2025-08-01T00:00:00Z',
                '--until',
                '2025-08-18T00:00:00Z',
                '--global',
            ]);

            const payload = JSON.parse(stdoutText(stats.stdout)) as TimeInStatusResponse;
            expect(payload.status).toBe('ok');
            const entry = payload.items.find((item) => item.id === 'TEST-1');
            expect(entry).toBeDefined();
            const segments = entry?.items ?? [];
            const secondsByStatus = new Map(
                segments.map((segment) => [segment.status.toLowerCase(), segment.seconds]),
            );
            expect(secondsByStatus.get('todo')).toBeGreaterThan(0);
            expect(secondsByStatus.get('inprogress')).toBeGreaterThan(0);
            expect(secondsByStatus.get('done')).toBeGreaterThan(0);
        } finally {
            await workspace.dispose();
        }
    });

    it('finds tasks when executed from nested workspace directories', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.initGit();
            const rootTask = await workspace.addTask('Root workspace task');
            await workspace.commitAll('record root task');

            const nestedDir = path.join('apps', 'frontend');
            await fs.ensureDir(path.join(workspace.root, nestedDir));

            const nestedTask = await workspace.addTask('Nested workspace task', {
                cwd: path.join(workspace.root, nestedDir),
            });
            await workspace.commitAll('record nested task');

            const nestedList = await workspace.runLotarIn(nestedDir, ['list']);
            const nestedOutput = stdoutText(nestedList.stdout);
            expect(nestedOutput).toContain(rootTask.id);
            expect(nestedOutput).toContain(nestedTask.id);
        } finally {
            await workspace.dispose();
        }
    });

    it('continues listing tasks when merge conflicts exist', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.initGit();
            const created = await workspace.addTask('Merge conflict task');
            await workspace.commitAll('seed task state');

            await workspace.runGit(['checkout', '-b', 'feature/conflict']);
            const featurePayload = parse(await workspace.readTaskYaml(created.id)) as Record<string, unknown>;
            featurePayload.description = 'Edited on feature branch';
            await fs.writeFile(created.filePath, stringify(featurePayload));
            await workspace.commitAll('feature edit');

            await workspace.runGit(['checkout', 'main']);
            const mainPayload = parse(await workspace.readTaskYaml(created.id)) as Record<string, unknown>;
            mainPayload.description = 'Edited on main branch';
            await fs.writeFile(created.filePath, stringify(mainPayload));
            await workspace.commitAll('main edit');

            const merge = await workspace.runGit(['merge', 'feature/conflict'], { reject: false });
            expect(merge.exitCode).not.toBe(0);

            const list = await workspace.runLotar(['list']);
            expect(stdoutText(list.stderr)).toContain('No tasks found');
            expect(stdoutText(list.stdout).trim()).toBe('');
        } finally {
            await workspace.dispose();
        }
    });

    it('normalizes sprint files and reports required changes', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.runLotar(['sprint', 'create', '--label', 'Normalize Smoke Sprint']);

            const sprintPath = path.join(workspace.tasksDir, '@sprints', '1.yml');
            const source = await fs.readFile(sprintPath, 'utf8');
            const payload = parse(source) as Record<string, any>;
            payload.plan = payload.plan ?? {};
            payload.plan.label = ' Normalize Smoke Sprint ';
            payload.plan.ends_at = '2030-01-15T10:00:00Z ';
            payload.plan.length = ' 2w ';
            await fs.writeFile(sprintPath, stringify(payload));

            const check = await workspace.runLotar(['sprint', 'normalize', '--check'], {
                acceptExitCodes: [1],
            });
            expect(check.exitCode).toBe(1);
            const checkOutput = stdoutText(check.stderr);
            expect(checkOutput).toContain('Sprint #1 requires normalization (run with --write to update).');
            expect(checkOutput).toContain(
                'Sprint #1 canonicalization notice: plan.length was ignored because plan.ends_at was provided.',
            );

            const write = await workspace.runLotar(['sprint', 'normalize', '--write']);
            const writeOutput = stdoutText(write.stdout);
            expect(writeOutput).toContain('Normalized sprint #1.');
            expect(writeOutput).toContain(
                'Sprint #1 canonicalization notice: plan.length was ignored because plan.ends_at was provided.',
            );

            const normalized = parse(await fs.readFile(sprintPath, 'utf8')) as Record<string, any>;
            expect(normalized.plan?.label).toBe('Normalize Smoke Sprint');
            expect(normalized.plan?.ends_at).toBe('2030-01-15T10:00:00Z');
            expect(normalized.plan?.length).toBeUndefined();

            const verify = await workspace.runLotar(['sprint', 'normalize', '--check']);
            expect(verify.exitCode).toBe(0);
            expect(stdoutText(verify.stdout)).toContain('Sprint #1 already canonical.');
        } finally {
            await workspace.dispose();
        }
    });

    it('summarizes sprint stats with effort and capacity details', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write(
                '.tasks/config.yml',
                [
                    'default.project: TEST',
                    'issue.states: [Todo, InProgress, Done]',
                    'issue.types: [Feature]',
                    'issue.priorities: [Medium]',
                    '',
                ].join('\n'),
            );

            await workspace.runLotar([
                'sprint',
                'create',
                '--label',
                'Smoke Stats',
                '--length',
                '1w',
                '--capacity-points',
                '20',
                '--capacity-hours',
                '60',
            ]);
            await workspace.runLotar([
                'sprint',
                'start',
                '1',
                '--at',
                '2025-02-01T09:00:00Z',
            ]);
            await workspace.runLotar([
                'sprint',
                'close',
                '1',
                '--at',
                '2025-02-08T09:00:00Z',
            ]);

            const first = await workspace.addTask('Stats hours done');
            const firstYaml = parse(await workspace.readTaskYaml(first.id)) as Record<string, any>;
            firstYaml.status = 'Done';
            firstYaml.effort = '8h';
            await fs.writeFile(first.filePath, stringify(firstYaml));

            const second = await workspace.addTask('Stats hours remaining');
            const secondYaml = parse(await workspace.readTaskYaml(second.id)) as Record<string, any>;
            secondYaml.status = 'InProgress';
            secondYaml.effort = '4h';
            await fs.writeFile(second.filePath, stringify(secondYaml));

            const third = await workspace.addTask('Stats points done');
            const thirdYaml = parse(await workspace.readTaskYaml(third.id)) as Record<string, any>;
            thirdYaml.status = 'Done';
            thirdYaml.effort = '3pt';
            await fs.writeFile(third.filePath, stringify(thirdYaml));

            const fourth = await workspace.addTask('Stats points todo');
            const fourthYaml = parse(await workspace.readTaskYaml(fourth.id)) as Record<string, any>;
            fourthYaml.status = 'Todo';
            fourthYaml.effort = '5pt';
            await fs.writeFile(fourth.filePath, stringify(fourthYaml));

            await workspace.runLotar([
                'sprint',
                'add',
                first.id,
                second.id,
                third.id,
                fourth.id,
                '--sprint',
                '1',
                '--allow-closed',
            ]);

            const stats = await workspace.runLotar(['sprint', 'stats', '1']);
            const output = stdoutText(stats.stdout);
            expect(output).toContain('Sprint stats for #1 (Smoke Stats).');
            expect(output).toContain('Tasks: 4 committed');
            expect(output).toContain('Points: 8 committed');
            expect(output).toContain('Hours: 12 committed');
            expect(output).toContain('Capacity: 20 planned');
            expect(output).toContain('Capacity: 60 planned');
            expect(output).toContain('Timeline:');
        } finally {
            await workspace.dispose();
        }
    });

    it('assigns CODEOWNERS on status changes when available', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.initGit();
            await workspace.write('CODEOWNERS', '* @docs-team\n');

            const created = await workspace.addTask('CODEOWNERS auto assignment');
            const initialPayload = parse(await workspace.readTaskYaml(created.id)) as Record<string, unknown>;
            expect(initialPayload['assignee']).toBeUndefined();

            await workspace.runLotar(['task', 'status', created.id, 'InProgress']);

            const updatedPayload = parse(await workspace.readTaskYaml(created.id)) as Record<string, unknown>;
            expect(updatedPayload['status']).toBe('InProgress');
            expect(updatedPayload['assignee']).toBe('docs-team');
        } finally {
            await workspace.dispose();
        }
    });

    it('respects default identity overrides from environment variables', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            const env = {
                LOTAR_DEFAULT_ASSIGNEE: 'env-assignee@example.com',
                LOTAR_DEFAULT_REPORTER: 'env-reporter@example.com',
            } satisfies Record<string, string>;

            const whoami = await workspace.runLotar(['whoami'], { env });
            expect(stdoutText(whoami.stdout)).toContain('env-reporter@example.com');

            const created = await workspace.addTask('Env override smoke task', {
                env,
            });

            const payload = parse(await workspace.readTaskYaml(created.id)) as Record<string, unknown>;
            expect(payload.assignee).toBe('env-assignee@example.com');
        } finally {
            await workspace.dispose();
        }
    });
});
