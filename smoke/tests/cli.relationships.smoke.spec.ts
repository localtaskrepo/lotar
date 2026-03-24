import { describe, expect, it } from 'vitest';
import { parse, stringify } from 'yaml';
import { SmokeWorkspace } from '../helpers/workspace.js';

const BASE_CONFIG = `default:
  project: REL
  reporter: rel@example.com
statuses: [Todo, InProgress, Done]
priorities: [Low, Medium, High]
types: [Feature, Bug]
`;

describe.concurrent('CLI task relationships smoke', () => {
    it('task relationships command shows stored relationships', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', BASE_CONFIG);

            const taskA = await workspace.addTask('Parent Task');
            const taskB = await workspace.addTask('Child Task');

            // Write relationships nested under "relationships" key
            const yamlContent = parse(await workspace.readTaskYaml(taskA.id)) as Record<string, any>;
            yamlContent.relationships = { depends_on: [taskB.id], related: ['EXT-99'] };
            const relativePath = workspace.getTaskFilePath(taskA.id).replace(workspace.root + '/', '');
            await workspace.write(relativePath, stringify(yamlContent));

            const result = await workspace.runLotar(['task', 'relationships', taskA.id]);
            expect(result.stdout).toContain(taskB.id);
            expect(result.stdout).toContain('depends-on');
        } finally {
            await workspace.dispose();
        }
    });

    it('task relationships JSON output includes relationship data', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', BASE_CONFIG);

            const taskA = await workspace.addTask('Show with deps');
            const taskB = await workspace.addTask('Dependency target');

            const yamlContent = parse(await workspace.readTaskYaml(taskA.id)) as Record<string, any>;
            yamlContent.relationships = { depends_on: [taskB.id] };
            const relativePath = workspace.getTaskFilePath(taskA.id).replace(workspace.root + '/', '');
            await workspace.write(relativePath, stringify(yamlContent));

            const result = await workspace.runLotar(['task', 'relationships', taskA.id, '--format', 'json']);
            const json = JSON.parse(result.stdout) as Record<string, any>;
            expect(json.relationships?.depends_on).toContain(taskB.id);
        } finally {
            await workspace.dispose();
        }
    });

    it('automation adds depends-on relationship', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', BASE_CONFIG);

            const target = await workspace.addTask('Dependency target');

            await workspace.write('.tasks/automation.yml', `automation:
  rules:
    - name: Auto add dependency
      when:
        priority: High
      on:
        created:
          add:
            depends_on: ["${target.id}"]
`);

            const source = await workspace.addTask('Auto dep source', {
                args: ['--priority', 'High'],
            });

            const yaml = parse(await workspace.readTaskYaml(source.id)) as Record<string, any>;
            const deps = yaml.relationships?.depends_on ?? [];
            expect(deps).toContain(target.id);
        } finally {
            await workspace.dispose();
        }
    });

    it('automation removes relationship on status change', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', BASE_CONFIG);

            const target = await workspace.addTask('Removal target');
            const source = await workspace.addTask('Removal source');

            // Manually write the dependency nested under "relationships"
            const yamlContent = parse(await workspace.readTaskYaml(source.id)) as Record<string, any>;
            yamlContent.relationships = { depends_on: [target.id] };
            const relativePath = workspace.getTaskFilePath(source.id).replace(workspace.root + '/', '');
            await workspace.write(relativePath, stringify(yamlContent));

            let yaml = parse(await workspace.readTaskYaml(source.id)) as Record<string, any>;
            expect(yaml.relationships?.depends_on ?? []).toContain(target.id);

            await workspace.write('.tasks/automation.yml', `automation:
  rules:
    - name: Remove dep on Done
      when:
        status: Done
      on:
        updated:
          remove:
            depends_on: ["${target.id}"]
`);

            await workspace.runLotar(['status', source.id, 'Done']);

            yaml = parse(await workspace.readTaskYaml(source.id)) as Record<string, any>;
            const deps = yaml.relationships?.depends_on ?? [];
            expect(deps).not.toContain(target.id);
        } finally {
            await workspace.dispose();
        }
    });

    it('CLI task reference add creates a link reference', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', BASE_CONFIG);

            const task = await workspace.addTask('Reference target');
            await workspace.runLotar([
                'task', 'reference', 'add', 'link', task.id, 'https://example.com/issue/123',
            ]);

            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            const refs = Array.isArray(yaml.references) ? yaml.references : [];
            // References use { link: "url" } format (not kind/value)
            expect(refs.some((r: any) => r.link?.includes('example.com'))).toBe(true);
        } finally {
            await workspace.dispose();
        }
    });
});
