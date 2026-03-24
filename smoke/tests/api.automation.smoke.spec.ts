import { describe, expect, it } from 'vitest';
import { startLotarServer } from '../helpers/server.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

const BASE_CONFIG = `default:
  project: API
issue:
  states: [Todo, InProgress, Review, Done]
  priorities: [Low, Medium, High, Critical]
  types: [Feature, Bug, Chore]
agent:
  worktree:
    enabled: false
`;

const SEED_RULES = `automation:
  rules:
    - name: Auto-tag bugs
      when:
        type: Bug
      on:
        created:
          set:
            priority: High
          add:
            tags: [bug-detected]
`;

interface AutomationShowResponse {
    data: {
        scope: string;
        source: string;
        scope_exists: boolean;
        scope_yaml: string;
        effective_yaml: string;
    };
}

interface AutomationSetResponse {
    data: {
        updated: boolean;
        warnings: string[];
        info: string[];
        errors: string[];
    };
}

interface AutomationSimulateResponse {
    data: {
        matched: boolean;
        rule_name: string | null;
        actions: Array<{ action: string; description: string }>;
        task_before: Record<string, unknown> | null;
        task_after: Record<string, unknown> | null;
    };
}

describe.concurrent('REST API automation endpoints', () => {
    it('GET /api/automation/show returns current rules', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': SEED_RULES,
            },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                const res = await fetch(`${server.url}/api/automation/show`);
                expect(res.ok).toBe(true);

                const body = (await res.json()) as AutomationShowResponse;
                expect(body.data.scope).toBe('global');
                expect(body.data.scope_exists).toBe(true);
                expect(body.data.effective_yaml).toContain('Auto-tag bugs');
                expect(body.data.scope_yaml).toContain('Auto-tag bugs');
                expect(body.data.source).toBeTruthy();
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('GET /api/automation/show?project=X returns project-scoped rules', async () => {
        const projectRules = `automation:
  rules:
    - name: Project-only rule
      on:
        created:
          add:
            tags: [project-scoped]
`;
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': SEED_RULES,
                '.tasks/API/automation.yml': projectRules,
            },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                const res = await fetch(`${server.url}/api/automation/show?project=API`);
                expect(res.ok).toBe(true);

                const body = (await res.json()) as AutomationShowResponse;
                expect(body.data.scope).toBe('project');
                expect(body.data.scope_exists).toBe(true);
                expect(body.data.scope_yaml).toContain('Project-only rule');
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('POST /api/automation/set saves valid YAML', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': SEED_RULES,
            },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                const newRules = [
                    'automation:',
                    '  rules:',
                    '    - name: API-set rule',
                    '      on:',
                    '        created:',
                    '          add:',
                    '            tags: [api-set]',
                ].join('\n');
                const setRes = await fetch(`${server.url}/api/automation/set`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ yaml: newRules }),
                });
                expect(setRes.ok).toBe(true);

                const setBody = (await setRes.json()) as AutomationSetResponse;
                expect(setBody.data.updated).toBe(true);
                expect(setBody.data.errors).toHaveLength(0);

                // Verify show returns the new rules
                const showRes = await fetch(`${server.url}/api/automation/show`);
                const showBody = (await showRes.json()) as AutomationShowResponse;
                expect(showBody.data.effective_yaml).toContain('API-set rule');
                expect(showBody.data.effective_yaml).toContain('api-set');
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('POST /api/automation/set rejects invalid YAML', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASE_CONFIG },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                const res = await fetch(`${server.url}/api/automation/set`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ yaml: 'not valid yaml: [[[' }),
                });
                // Should fail with a 4xx error
                expect(res.ok).toBe(false);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('POST /api/automation/simulate returns matching rule', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': SEED_RULES,
            },
        });

        try {
            const bugTask = await workspace.addTask('Simulate bug', { args: ['--type', 'Bug'] });
            const server = await startLotarServer(workspace);
            try {
                const res = await fetch(`${server.url}/api/automation/simulate`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ ticket_id: bugTask.id, event: 'created' }),
                });
                expect(res.ok).toBe(true);

                const body = (await res.json()) as AutomationSimulateResponse;
                expect(body.data.matched).toBe(true);
                expect(body.data.rule_name).toBe('Auto-tag bugs');
                expect(body.data.actions.length).toBeGreaterThan(0);
                expect(body.data.actions.some((a) => a.action === 'set_priority')).toBe(true);
                expect(body.data.actions.some((a) => a.action === 'add_tags')).toBe(true);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('POST /api/automation/simulate with no match returns empty', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': SEED_RULES,
            },
        });

        try {
            const task = await workspace.addTask('Non-bug task');
            const server = await startLotarServer(workspace);
            try {
                const res = await fetch(`${server.url}/api/automation/simulate`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ ticket_id: task.id, event: 'assigned' }),
                });
                expect(res.ok).toBe(true);

                const body = (await res.json()) as AutomationSimulateResponse;
                expect(body.data.matched).toBe(false);
                expect(body.data.rule_name).toBeNull();
                expect(body.data.actions).toHaveLength(0);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('round-trip: set rules via API then verify automation fires on task create', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: { '.tasks/config.yml': BASE_CONFIG },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                // Set automation rules via API
                const rules = [
                    'automation:',
                    '  rules:',
                    '    - name: API round-trip',
                    '      on:',
                    '        created:',
                    '          set:',
                    '            priority: Critical',
                    '          add:',
                    '            tags: [api-created]',
                ].join('\n');
                const setRes = await fetch(`${server.url}/api/automation/set`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ yaml: rules }),
                });
                expect(setRes.ok).toBe(true);

                // Create a task via REST API
                const createRes = await fetch(`${server.url}/api/tasks/add`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ title: 'API round-trip task' }),
                });
                expect(createRes.status).toBe(201);

                const createBody = (await createRes.json()) as { data: { id: string; priority: string; tags: string[] } };

                // Automation runs after the DTO is built, so the create
                // response may not reflect automated changes. Fetch the task
                // again to see the automated state.
                const getRes = await fetch(`${server.url}/api/tasks/get?id=${createBody.data.id}`);
                expect(getRes.ok).toBe(true);
                const getBody = (await getRes.json()) as { data: { id: string; priority: string; tags: string[] } };
                expect(getBody.data.priority).toBe('Critical');
                expect(getBody.data.tags).toContain('api-created');
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('cooldown prevents re-firing within the window (server mode)', async () => {
        const cooldownRules = `automation:
  rules:
    - name: Cooldown comment
      cooldown: 60s
      on:
        updated:
          comment: "automation fired"
`;
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': cooldownRules,
            },
        });

        try {
            const task = await workspace.addTask('Cooldown API test');
            const server = await startLotarServer(workspace);
            try {
                // First update: status → InProgress
                const res1 = await fetch(`${server.url}/api/tasks/status`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ id: task.id, status: 'InProgress' }),
                });
                expect(res1.ok).toBe(true);

                // Second update: status → Review (within cooldown)
                const res2 = await fetch(`${server.url}/api/tasks/status`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ id: task.id, status: 'Review' }),
                });
                expect(res2.ok).toBe(true);

                // Read comments from the task
                const getRes = await fetch(`${server.url}/api/tasks/get?id=${task.id}`);
                expect(getRes.ok).toBe(true);
                const body = (await getRes.json()) as { data: { comments?: Array<{ text: string }> } };
                const comments = body.data.comments ?? [];
                const autoComments = comments.filter((c) => c.text.includes('automation fired'));
                // Cooldown should have prevented the second fire
                expect(autoComments.length).toBe(1);
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
