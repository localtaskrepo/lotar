import { existsSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { SmokeWorkspace } from '../helpers/workspace.js';

const AUTOMATION_CONFIG = `default:
  project: AUTO
  reporter: smoker@example.com
issue:
  states: [Todo, InProgress, Review, Done]
  priorities: [Low, Medium, High, Critical]
  types: [Feature, Bug, Chore]

agent:
  logs_dir: .logs
  worktree:
    enabled: false
`;

function writeAutomation(workspace: SmokeWorkspace, yaml: string): Promise<void> {
    return workspace.write('.tasks/automation.yml', yaml);
}

describe.concurrent('CLI automation smoke scenarios', () => {
    it('fires on.created event and sets fields', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Auto-set on create
      on:
        created:
          set:
            priority: High
          add:
            tags: [auto-created]
`);

            const task = await workspace.addTask('Test auto create');
            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;

            expect(yaml.priority).toBe('High');
            expect(yaml.tags).toContain('auto-created');
        } finally {
            await workspace.dispose();
        }
    });

    it('fires on.updated event when status changes', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: InProgress triggers tag
      when:
        status: InProgress
      on:
        updated:
          add:
            tags: [working]
`);

            const task = await workspace.addTask('Test auto update');
            await workspace.runLotar(['status', task.id, 'InProgress']);

            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            expect(yaml.status).toBe('InProgress');
            expect(yaml.tags).toContain('working');
        } finally {
            await workspace.dispose();
        }
    });

    it('fires on.assigned event', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Auto-set on assign
      on:
        assigned:
          set:
            status: InProgress
          add:
            tags: [assigned]
`);

            const task = await workspace.addTask('Test auto assign');
            await workspace.runLotar(['assignee', task.id, 'dev@example.com']);

            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            expect(yaml.assignee).toBe('dev@example.com');
            expect(yaml.status).toBe('InProgress');
            expect(yaml.tags).toContain('assigned');
        } finally {
            await workspace.dispose();
        }
    });

    it('supports condition matching with changes block', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Status transition tag
      when:
        changes:
          status:
            from: InProgress
            to: Review
      on:
        updated:
          add:
            tags: [review-ready]
`);

            const task = await workspace.addTask('Test changes block');
            await workspace.runLotar(['status', task.id, 'InProgress']);
            await workspace.runLotar(['status', task.id, 'Review']);

            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            expect(yaml.status).toBe('Review');
            expect(yaml.tags).toContain('review-ready');
        } finally {
            await workspace.dispose();
        }
    });

    it('does not fire changes rule when from does not match', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Only from InProgress
      when:
        changes:
          status:
            from: InProgress
            to: Review
      on:
        updated:
          add:
            tags: [review-ready]
`);

            const task = await workspace.addTask('Test changes mismatch');
            // Transition directly from Todo -> Review (not from InProgress)
            await workspace.runLotar(['status', task.id, 'Review']);

            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            expect(yaml.status).toBe('Review');
            const tags = Array.isArray(yaml.tags) ? yaml.tags : [];
            expect(tags).not.toContain('review-ready');
        } finally {
            await workspace.dispose();
        }
    });

    it('adds and removes tags in a single rule', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Add seed tag on create
      on:
        created:
          add:
            tags: [seed, needs-review]
    - name: Swap tags on InProgress
      when:
        status: InProgress
      on:
        updated:
          add:
            tags: [active]
          remove:
            tags: [needs-review]
`);

            const task = await workspace.addTask('Test tag swap');
            let yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            expect(yaml.tags).toContain('seed');
            expect(yaml.tags).toContain('needs-review');

            await workspace.runLotar(['status', task.id, 'InProgress']);
            yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            expect(yaml.tags).toContain('seed');
            expect(yaml.tags).toContain('active');
            expect(yaml.tags).not.toContain('needs-review');
        } finally {
            await workspace.dispose();
        }
    });

    it('appends a comment via automation action', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Comment on create
      on:
        created:
          comment: "Auto-created by automation"
`);

            const task = await workspace.addTask('Test comment action');
            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            const comments = Array.isArray(yaml.comments) ? yaml.comments : [];
            expect(comments.length).toBeGreaterThanOrEqual(1);
            const texts = comments.map((c: any) => c.text ?? c);
            expect(texts.some((t: string) => t.includes('Auto-created by automation'))).toBe(true);
        } finally {
            await workspace.dispose();
        }
    });

    it('automation simulate returns expected actions', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: High priority rule
      when:
        priority: High
      on:
        updated:
          set:
            status: Review
          add:
            tags: [priority-escalated]
`);

            const task = await workspace.addTask('Simulate test', {
                args: ['--priority', 'High'],
            });

            const result = await workspace.runLotar([
                'automation', 'simulate',
                '--ticket', task.id,
                '--event', 'updated',
            ]);

            expect(result.stdout).toContain('Rule matched');
            expect(result.stdout).toContain('High priority rule');
        } finally {
            await workspace.dispose();
        }
    });

    it('automation simulate with no matching rule', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Only High priority
      when:
        priority: High
      on:
        updated:
          set:
            status: Review
`);

            const task = await workspace.addTask('Simulate no match');
            // Created with default Medium priority, so rule won't match

            const result = await workspace.runLotar([
                'automation', 'simulate',
                '--ticket', task.id,
                '--event', 'updated',
            ]);

            expect(result.stdout).toContain('No automation rules matched');
        } finally {
            await workspace.dispose();
        }
    });

    it('supports all/any condition combinators', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Combined conditions
      when:
        all:
          - priority: High
          - any:
              - type: Bug
              - type: Feature
      on:
        created:
          add:
            tags: [hot]
`);

            // Positive: priority=High + type=Feature(default) → both all conditions match
            const task1 = await workspace.addTask('Combinator pos', {
                args: ['--priority', 'High'],
            });
            let yaml = parse(await workspace.readTaskYaml(task1.id)) as Record<string, any>;
            let tags = Array.isArray(yaml.tags) ? yaml.tags : [];
            expect(tags).toContain('hot');

            // Negative: default priority=Medium → all fails at priority check
            const task2 = await workspace.addTask('Combinator neg');
            yaml = parse(await workspace.readTaskYaml(task2.id)) as Record<string, any>;
            tags = Array.isArray(yaml.tags) ? yaml.tags : [];
            expect(tags).not.toContain('hot');
        } finally {
            await workspace.dispose();
        }
    });

    it('max_iterations limits cascading automation', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            // Create a rule that would cascade: created sets InProgress, InProgress sets Review
            // With max_iterations: 1, only the first rule fires
            await writeAutomation(workspace, `automation:
  max_iterations: 1
  rules:
    - name: First cascade
      on:
        created:
          set:
            status: InProgress
    - name: Second cascade
      when:
        status: InProgress
      on:
        updated:
          add:
            tags: [cascaded]
`);

            const task = await workspace.addTask('Max iterations test');
            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;

            // First rule should fire (status -> InProgress)
            expect(yaml.status).toBe('InProgress');
            // Second rule should NOT fire because max_iterations: 1
            const tags = Array.isArray(yaml.tags) ? yaml.tags : [];
            expect(tags).not.toContain('cascaded');
        } finally {
            await workspace.dispose();
        }
    });

    it('field condition with regex matches', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Regex title match
      when:
        title:
          matches: "^\\\\[BUG\\\\]"
      on:
        created:
          set:
            type: Bug
            priority: High
`);

            const task = await workspace.addTask('[BUG] Something broken');
            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            expect(yaml.type).toBe('Bug');
            expect(yaml.priority).toBe('High');
        } finally {
            await workspace.dispose();
        }
    });

    it('run action executes a shell command', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            // Shell form (plain string) runs synchronously with wait:true by default
            await writeAutomation(workspace, `automation:
  rules:
    - name: Run on create
      on:
        created:
          run: "touch run-fired.txt"
`);

            await workspace.addTask('Run action test');
            // Shell variant waits by default; sentinel should exist in workspace root
            const sentinel = `${workspace.root}/run-fired.txt`;
            expect(existsSync(sentinel)).toBe(true);
        } finally {
            await workspace.dispose();
        }
    });

    it('template expansion in comment actions', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Expand ticket fields
      on:
        created:
          comment: "Type is \${{ticket.type}}, priority is \${{ticket.priority}}"
`);

            const task = await workspace.addTask('Template test', {
                args: ['--type', 'Bug', '--priority', 'High'],
            });
            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            const comments = Array.isArray(yaml.comments) ? yaml.comments : [];
            const texts = comments.map((c: any) => c.text ?? c);
            expect(texts.some((t: string) => t.includes('Type is Bug'))).toBe(true);
            expect(texts.some((t: string) => t.includes('priority is High'))).toBe(true);
        } finally {
            await workspace.dispose();
        }
    });

    it('${{previous.*}} template variables in update events', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Previous status comment
      on:
        updated:
          comment: "Was \${{previous.status}}, now \${{ticket.status}}"
`);

            const task = await workspace.addTask('Previous vars test');
            await workspace.runLotar(['status', task.id, 'InProgress']);

            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            const comments = Array.isArray(yaml.comments) ? yaml.comments : [];
            const texts = comments.map((c: any) => c.text ?? c);
            expect(texts.some((t: string) => t.includes('Was Todo') && t.includes('now InProgress'))).toBe(true);
        } finally {
            await workspace.dispose();
        }
    });

    it('async run action does not block task update', async () => {
        const workspace = await SmokeWorkspace.create();

        try {
            await workspace.write('.tasks/config.yml', AUTOMATION_CONFIG);
            await writeAutomation(workspace, `automation:
  rules:
    - name: Slow async run
      on:
        created:
          run: "sleep 30"
          add:
            tags: [async-verified]
`);

            // If the run were blocking, this would take 30s and likely time out.
            // The tag should be added regardless since run is async by default.
            const task = await workspace.addTask('Async run test');
            const yaml = parse(await workspace.readTaskYaml(task.id)) as Record<string, any>;
            expect(yaml.tags).toContain('async-verified');
        } finally {
            await workspace.dispose();
        }
    });
});
