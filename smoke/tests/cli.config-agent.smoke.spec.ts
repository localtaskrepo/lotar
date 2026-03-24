import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { SmokeWorkspace } from '../helpers/workspace.js';

describe.concurrent('CLI config and agent features smoke', () => {
    it('config show includes worktree cleanup settings', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', `default:
  project: CFG
issue:
  states: [Todo, Done]
  priorities: [Low]
  types: [Feature]

agent:
  worktree:
    enabled: true
    cleanup_on_failure: true
    cleanup_on_cancel: false
`);

            const result = await workspace.runLotar(['config', 'show', '--format', 'json']);
            const json = JSON.parse(result.stdout) as Record<string, any>;

            // config show JSON nests resolved settings under "config" key
            const worktree = json.config?.agent_worktree;
            expect(worktree).toBeDefined();
            expect(worktree.enabled).toBe(true);
            expect(worktree.cleanup_on_failure).toBe(true);
            expect(worktree.cleanup_on_cancel).toBe(false);
        } finally {
            await workspace.dispose();
        }
    });

    it('config show includes agent profiles', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', `default:
  project: CFG
issue:
  states: [Todo, Done]
  priorities: [Low]
  types: [Feature]

agent:
  worktree:
    enabled: false

agents:
  review-bot:
    runner: claude
    description: Reviews PRs
  code-bot:
    runner: copilot
    description: Writes code
`);

            const result = await workspace.runLotar(['config', 'show', '--format', 'json']);
            const json = JSON.parse(result.stdout) as Record<string, any>;

            // agent_profiles is the resolved key name in config show JSON
            const agents = json.config?.agent_profiles;
            expect(agents).toBeDefined();
            expect(agents['review-bot']).toBeDefined();
            expect(agents['review-bot'].runner).toBe('claude');
            expect(agents['code-bot']).toBeDefined();
            expect(agents['code-bot'].runner).toBe('copilot');
        } finally {
            await workspace.dispose();
        }
    });

    it('automation file is loaded from project-specific path', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', `default:
  project: PROJ
issue:
  states: [Todo, InProgress, Done]
  priorities: [Low, Medium, High]
  types: [Feature]
`);

            // Only project-specific automation (no global competing file)
            await workspace.write('.tasks/PROJ/automation.yml', `automation:
  rules:
    - name: Project rule
      on:
        created:
          add:
            tags: [project-specific]
`);

            const task = await workspace.addTask('Project automation test', {
                args: ['-p', 'PROJ'],
            });
            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            const tags = Array.isArray(yaml.tags) ? yaml.tags : [];

            expect(tags).toContain('project-specific');
        } finally {
            await workspace.dispose();
        }
    });

    it('config explain shows field sources', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', `default:
  project: EXP
  reporter: explain@example.com
issue:
  states: [Todo, Done]
  priorities: [Low]
  types: [Feature]
`);

            const result = await workspace.runLotar(['config', 'show', '--explain']);
            expect(result.stdout).toContain('explain@example.com');
        } finally {
            await workspace.dispose();
        }
    });
});
