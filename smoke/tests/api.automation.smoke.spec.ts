import { spawn, type ChildProcess } from 'node:child_process';
import fs from 'fs-extra';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { indentBlock, nodeExecutable, nodeRunActionYaml } from '../helpers/agent-fixtures.js';
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

// Bounded chatty-stderr fixture for the async automation monitor contract.
// Writes --total-bytes to stderr in --chunk-bytes writes, each awaited via
// its write callback (honest backpressure: with no draining reader the
// callbacks stall once the pipe buffer fills), then records the drained
// byte count and a completed marker and exits 0. A --bound-ms leak guard
// aborts with an explicit marker so a wedged pipe can never leak the
// process — that is what makes both monitor shapes below deterministic
// without fixed sleeps.
const CHATTY_STDERR_SCRIPT = `import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

function parseArgs(argv) {
    const parsed = {};
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i];
        if (['--marker-dir', '--label', '--chunk-bytes', '--total-bytes', '--bound-ms'].includes(arg)) {
            const value = argv[i + 1];
            if (value !== undefined) {
                parsed[arg] = value;
                i += 1;
            }
        }
    }
    return parsed;
}

function intOr(raw, fallback) {
    const value = Number.parseInt(raw ?? '', 10);
    return Number.isInteger(value) ? value : fallback;
}

const options = parseArgs(process.argv.slice(2));
const markerDir = options['--marker-dir'] ?? '.';
const label = options['--label'] ?? 'chatty';
const chunkBytes = intOr(options['--chunk-bytes'], 65536);
const totalBytes = intOr(options['--total-bytes'], 327680);
const boundMs = intOr(options['--bound-ms'], 15000);
const chunk = 'x'.repeat(chunkBytes);
let written = 0;
let settled = false;

function writeMarker(name, text) {
    try {
        mkdirSync(markerDir, { recursive: true });
        writeFileSync(path.join(markerDir, name), text);
    } catch {
        // Best effort: markers must never crash the fixture.
    }
}

const bound = setTimeout(() => {
    if (settled) {
        return;
    }
    settled = true;
    writeMarker(\`aborted-\${label}.marker\`, \`written=\${written} bound=\${boundMs}\\n\`);
    process.exit(3);
}, boundMs);

function writeNext() {
    if (settled) {
        return;
    }
    if (written >= totalBytes) {
        settled = true;
        clearTimeout(bound);
        writeMarker(\`bytes-\${label}.marker\`, \`\${written}\\n\`);
        writeMarker(\`completed-\${label}.marker\`, \`\${process.pid}\\n\`);
        process.exit(0);
    }
    const size = Math.min(chunkBytes, totalBytes - written);
    process.stderr.write(size === chunkBytes ? chunk : chunk.slice(0, size), (err) => {
        if (settled) {
            return;
        }
        if (err) {
            settled = true;
            clearTimeout(bound);
            writeMarker(\`aborted-\${label}.marker\`, \`written=\${written} error=\${err}\\n\`);
            process.exit(4);
        }
        written += size;
        writeNext();
    });
}

writeNext();
`;

function chattyArgs(markerDir: string, label: string, totalBytes: number, boundMs: number): string[] {
    return [
        '--marker-dir',
        markerDir,
        '--label',
        label,
        '--chunk-bytes',
        '65536',
        '--total-bytes',
        String(totalBytes),
        '--bound-ms',
        String(boundMs),
    ];
}

async function waitForMarker(filePath: string, timeoutMs: number): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (fs.pathExistsSync(filePath)) {
            return;
        }
        await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error(`Timed out after ${timeoutMs}ms waiting for ${filePath}`);
}

/** Pre-fix monitor replica: wait for exit first, read piped stderr after. */
function monitorOldShape(child: ChildProcess): Promise<{ code: number | null }> {
    return new Promise((resolve, reject) => {
        child.on('error', reject);
        child.on('close', (code) => resolve({ code }));
    });
}

/** Fixed monitor replica (wait_with_output): drain stderr while waiting. */
async function monitorNewShape(child: ChildProcess): Promise<{ code: number | null; stderrBytes: number }> {
    let stderrBytes = 0;
    child.stderr?.on('data', (chunk: Buffer) => {
        stderrBytes += chunk.length;
    });
    const code = await new Promise<number | null>((resolve, reject) => {
        child.on('error', reject);
        child.on('close', (exitCode) => resolve(exitCode));
    });
    return { code, stderrBytes };
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

    it('POST /api/automation/set rejects invalid YAML and cooldowns without rewriting rules', async () => {
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': SEED_RULES,
            },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                for (const yaml of [
                    'not valid yaml: [[[',
                    'automation:\n  rules:\n    - cooldown: "1\u00e9"\n',
                    'automation:\n  rules:\n    - cooldown: "18446744073709551615d"\n',
                ]) {
                    const res = await fetch(`${server.url}/api/automation/set`, {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ yaml }),
                    });
                    expect(res.status).toBe(400);
                    expect(await fs.readFile(path.join(workspace.tasksDir, 'automation.yml'), 'utf8'))
                        .toBe(SEED_RULES);
                }
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

    it('monitor counterfactual: wait-then-read wedges a chatty stderr child, concurrent drain completes', async () => {
        // Private counterfactual against the held pre-fix monitor shape
        // (child.wait() BEFORE reading piped stderr): with 256KiB of
        // callback-awaited stderr and no reader, the pipe buffer fills and
        // the child can only exit through its leak-guard bound. The fixed
        // shape (wait_with_output) drains while waiting and the child
        // completes. Production is already fixed, so the old leg replicates
        // the old monitor here rather than reverting anything; this models
        // pipe-buffer semantics portably and is not a real Windows claim.
        const oldDir = await mkdtemp(path.join(tmpdir(), 'lotar-smoke-monitor-old-'));
        const newDir = await mkdtemp(path.join(tmpdir(), 'lotar-smoke-monitor-new-'));
        const script = path.join(oldDir, 'chatty-stderr.mjs');
        await fs.writeFile(script, CHATTY_STDERR_SCRIPT, 'utf8');
        try {
            const oldChild = spawn(
                nodeExecutable(),
                [script, ...chattyArgs('.', 'old', 262_144, 3_000)],
                { cwd: oldDir, stdio: ['ignore', 'ignore', 'pipe'] },
            );
            const oldOutcome = await monitorOldShape(oldChild);
            expect(oldOutcome.code).toBe(3);
            expect(fs.pathExistsSync(path.join(oldDir, 'aborted-old.marker'))).toBe(true);
            expect(fs.pathExistsSync(path.join(oldDir, 'completed-old.marker'))).toBe(false);

            const newChild = spawn(
                nodeExecutable(),
                [script, ...chattyArgs('.', 'new', 262_144, 15_000)],
                { cwd: newDir, stdio: ['ignore', 'ignore', 'pipe'] },
            );
            const newOutcome = await monitorNewShape(newChild);
            expect(newOutcome.code).toBe(0);
            expect(newOutcome.stderrBytes).toBeGreaterThanOrEqual(262_144);
            await waitForMarker(path.join(newDir, 'completed-new.marker'), 5_000);
            const drained = Number.parseInt(
                await fs.readFile(path.join(newDir, 'bytes-new.marker'), 'utf8'),
                10,
            );
            expect(drained).toBeGreaterThanOrEqual(262_144);
            expect(fs.pathExistsSync(path.join(newDir, 'aborted-new.marker'))).toBe(false);
        } finally {
            await fs.remove(oldDir).catch(() => undefined);
            await fs.remove(newDir).catch(() => undefined);
        }
    });

    it('server async run drains a chatty stderr child without wedging the monitor', async () => {
        // The automation must fire inside the SERVER process (task created
        // via the REST API while the server hosts the spawned child), so
        // the production monitor thread is the one under test. The child
        // writes 320KiB of callback-awaited stderr before its completed
        // marker: with the pre-fix monitor it could only exit through its
        // leak-guard bound (aborted marker); the fixed monitor drains while
        // waiting and the run completes.
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': `automation:
  rules:
    - name: Chatty async run
      on:
        created:
${indentBlock(
    nodeRunActionYaml(
        ['fixtures/chatty-stderr.mjs', ...chattyArgs('chatty-markers', 'server', 327_680, 15_000)],
        { wait: false },
    ),
    10,
)}
`,
                'fixtures/chatty-stderr.mjs': CHATTY_STDERR_SCRIPT,
            },
        });

        try {
            const server = await startLotarServer(workspace);
            try {
                const createRes = await fetch(`${server.url}/api/tasks/add`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ title: 'Chatty async run task' }),
                });
                expect(createRes.status).toBe(201);

                const markers = path.join(workspace.root, 'chatty-markers');
                await waitForMarker(path.join(markers, 'completed-server.marker'), 10_000);

                const drained = Number.parseInt(
                    await fs.readFile(path.join(markers, 'bytes-server.marker'), 'utf8'),
                    10,
                );
                expect(drained).toBeGreaterThanOrEqual(262_144);
                const aborted = await fs.readFile(path.join(markers, 'aborted-server.marker'), 'utf8')
                    .catch(() => null);
                expect(aborted).toBeNull();
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
