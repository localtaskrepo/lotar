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
// process — but on Windows that guard cannot fire while the wedge holds
// (pipe writes are synchronous there, so a full pipe blocks the child's
// whole event loop, watchdog timer included). The old-shape leg therefore
// owns its bound in the PARENT: the monitor replica kills the child after
// a guard window instead of waiting on the child-local watchdog, which is
// what makes both legs below deterministic on every platform without
// fixed sleeps, inflated timeouts, or skips.
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

// Descendant-held stderr fixture for the async monitor's bounded-grace
// contract. The primary writes --total-bytes of newline-free stderr
// (callback-awaited, unique head/tail sentinels embedded first/last),
// spawns a descendant with stderr INHERITED (so it holds the automation
// monitor's pipe write end), waits for the descendant's holding marker,
// then exits nonzero. The descendant writes nothing to stderr until it
// OBSERVES the primary-exit marker; it then writes a probe record to the
// inherited stderr and records delivered/EPIPE markers — the probe is the
// binding proof that the write endpoint genuinely survives the primary
// (an alive-marker alone cannot prove that). Finally it waits for the
// owned release marker under a bounded leak guard and records
// released/bailed on exit.
//
// Windows note: the descendant MUST be spawned `detached: true`. libuv
// assigns every non-detached child of a Node process to that process's
// private global job object (src/win/process.c, uv_spawn ->
// AssignProcessToJobObject) created with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
// (uv__init_global_job_handle); when the primary exits, the job handle
// closes and the kernel kills the descendant, closing the inherited write
// end and delivering a premature EOF to the monitor. `detached: true`
// skips the assignment so the descendant survives the primary on both
// platforms (POSIX detached = new session, harmless here).
const DESCENDANT_STDERR_SCRIPT = `import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';

function parseArgs(argv) {
    const parsed = {};
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i];
        if (['--role', '--marker-dir', '--label', '--total-bytes', '--head-sentinel', '--tail-sentinel', '--exit', '--release-file', '--hold-ms'].includes(arg)) {
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

function writeMarker(dir, name, text) {
    try {
        mkdirSync(dir, { recursive: true });
        writeFileSync(path.join(dir, name), \`\${text}\\n\`);
    } catch {
        // Best effort: markers must never crash the fixture.
    }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const options = parseArgs(process.argv.slice(2));
const role = options['--role'] ?? 'primary';
const markerDir = path.resolve(options['--marker-dir'] ?? '.');
const label = options['--label'] ?? 'held';
const holdMs = intOr(options['--hold-ms'], 20000);
const releaseFile = path.resolve(options['--release-file'] ?? path.join(markerDir, \`release-\${label}.marker\`));

if (role === 'descendant') {
    writeMarker(markerDir, \`descendant-holding-\${label}.marker\`, \`pid=\${process.pid}\`);
    const deadline = Date.now() + holdMs;
    // Wait for the primary to exit first: the probe below must prove the
    // inherited write endpoint survives the primary's death.
    const primaryExited = path.join(markerDir, \`primary-exited-\${label}.marker\`);
    while (!existsSync(primaryExited) && Date.now() < deadline) {
        await sleep(10);
    }
    const probe = \`\\nPROBE-\${label}-\${process.pid}-stderr-endpoint-alive\\n\`;
    try {
        process.stderr.write(probe, (err) => {
            writeMarker(
                markerDir,
                err ? \`probe-epipe-\${label}.marker\` : \`probe-delivered-\${label}.marker\`,
                String(err ?? 'ok'),
            );
        });
    } catch (err) {
        writeMarker(markerDir, \`probe-epipe-\${label}.marker\`, String(err));
    }
    while (!existsSync(releaseFile) && Date.now() < deadline) {
        await sleep(25);
    }
    writeMarker(
        markerDir,
        existsSync(releaseFile) ? \`descendant-released-\${label}.marker\` : \`descendant-bailed-\${label}.marker\`,
        \`pid=\${process.pid}\`,
    );
    process.exit(0);
}

const totalBytes = intOr(options['--total-bytes'], 32768);
const chunkBytes = 4096;
const headSentinel = options['--head-sentinel'] ?? \`HEAD-\${label}\`;
const tailSentinel = options['--tail-sentinel'] ?? \`TAIL-\${label}\`;
const exitCode = intOr(options['--exit'], 7);
writeMarker(markerDir, \`primary-started-\${label}.marker\`, \`pid=\${process.pid}\`);

const chunks = [];
let offset = 0;
while (offset < totalBytes) {
    const size = Math.min(chunkBytes, totalBytes - offset);
    let chunk = 'x'.repeat(size);
    if (offset === 0) {
        chunk = headSentinel + chunk.slice(headSentinel.length);
    }
    if (offset + size >= totalBytes) {
        chunk = chunk.slice(0, size - tailSentinel.length) + tailSentinel;
    }
    chunks.push(chunk);
    offset += size;
}

let finished = false;
function finish() {
    if (finished) {
        return;
    }
    finished = true;
    (async () => {
        const holdingMarker = path.join(markerDir, \`descendant-holding-\${label}.marker\`);
        const deadline = Date.now() + 5000;
        while (!existsSync(holdingMarker) && Date.now() < deadline) {
            await sleep(10);
        }
        writeMarker(markerDir, \`primary-exited-\${label}.marker\`, \`pid=\${process.pid}\`);
        process.exit(exitCode);
    })();
}

function afterPayload() {
    const child = spawn(
        process.execPath,
        [
            process.argv[1],
            '--role', 'descendant',
            '--marker-dir', markerDir,
            '--label', label,
            '--release-file', releaseFile,
            '--hold-ms', String(holdMs),
        ],
        // detached is load-bearing on Windows: it skips libuv's global
        // kill-on-parent-exit job assignment (see fixture comment).
        { stdio: ['ignore', 'ignore', 'inherit'], detached: true },
    );
    child.on('error', () => finish());
    child.on('spawn', () => finish());
    setTimeout(() => finish(), Math.min(holdMs, 5000)).unref();
}

function writeNext(index) {
    if (index >= chunks.length) {
        afterPayload();
        return;
    }
    process.stderr.write(chunks[index], () => writeNext(index + 1));
}

writeNext(0);
`;

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

/** Pre-fix monitor replica: wait for exit first, read piped stderr after.
 *
 * The bound is parent-owned: on POSIX a wedged child still runs its own
 * watchdog and exits by itself, but on Windows pipe writes are
 * synchronous, so a child stalled on a full pipe has a frozen event loop
 * and can never self-terminate — the guard here kills it so the replica
 * always resolves inside `guardMs` instead of hanging the suite. */
function monitorOldShapeGuarded(
    child: ChildProcess,
    guardMs: number,
): Promise<{ code: number | null; signal: NodeJS.Signals | null; killedByGuard: boolean }> {
    return new Promise((resolve, reject) => {
        child.on('error', reject);
        let killedByGuard = false;
        const guard = setTimeout(() => {
            killedByGuard = true;
            child.kill();
        }, guardMs);
        child.on('close', (code, signal) => {
            clearTimeout(guard);
            resolve({ code, signal, killedByGuard });
        });
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
                    expect(await fs.readFile(path.join(workspace.tasksDir, 'automation.yml'), 'utf8')).toBe(SEED_RULES);
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

                const createBody = (await createRes.json()) as {
                    data: { id: string; priority: string; tags: string[] };
                };

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
        // the child cannot finish its payload. On POSIX it exits through
        // its leak-guard bound; on Windows the same wedge freezes the
        // child's event loop (synchronous pipe writes), so the parent-side
        // guard kills it. Either way the completion markers stay absent —
        // that absence is the wedge proof, never the child's own timer.
        // The fixed shape (concurrent drain, the production monitor)
        // drains while waiting and the child completes. Production is
        // already fixed, so the old leg replicates the old monitor here
        // rather than reverting anything.
        const oldDir = await mkdtemp(path.join(tmpdir(), 'lotar-smoke-monitor-old-'));
        const newDir = await mkdtemp(path.join(tmpdir(), 'lotar-smoke-monitor-new-'));
        const script = path.join(oldDir, 'chatty-stderr.mjs');
        await fs.writeFile(script, CHATTY_STDERR_SCRIPT, 'utf8');
        try {
            const oldChild = spawn(nodeExecutable(), [script, ...chattyArgs('.', 'old', 262_144, 3_000)], {
                cwd: oldDir,
                stdio: ['ignore', 'ignore', 'pipe'],
            });
            // Guard is the child bound (3s) plus slack: on POSIX the child
            // self-aborts first and close fires normally; on Windows only
            // the guard can end the wedge.
            const oldOutcome = await monitorOldShapeGuarded(oldChild, 4_000);
            expect(fs.pathExistsSync(path.join(oldDir, 'completed-old.marker'))).toBe(false);
            expect(fs.pathExistsSync(path.join(oldDir, 'bytes-old.marker'))).toBe(false);
            if (oldOutcome.killedByGuard) {
                // Windows wedge shape: the frozen child never wrote any
                // terminal marker at all.
                expect(fs.pathExistsSync(path.join(oldDir, 'aborted-old.marker'))).toBe(false);
            } else {
                expect(oldOutcome.code).toBe(3);
                expect(fs.pathExistsSync(path.join(oldDir, 'aborted-old.marker'))).toBe(true);
            }

            const newChild = spawn(nodeExecutable(), [script, ...chattyArgs('.', 'new', 262_144, 15_000)], {
                cwd: newDir,
                stdio: ['ignore', 'ignore', 'pipe'],
            });
            const newOutcome = await monitorNewShape(newChild);
            expect(newOutcome.code).toBe(0);
            expect(newOutcome.stderrBytes).toBeGreaterThanOrEqual(262_144);
            await waitForMarker(path.join(newDir, 'completed-new.marker'), 5_000);
            const drained = Number.parseInt(await fs.readFile(path.join(newDir, 'bytes-new.marker'), 'utf8'), 10);
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
    nodeRunActionYaml(['fixtures/chatty-stderr.mjs', ...chattyArgs('chatty-markers', 'server', 327_680, 15_000)], {
        wait: false,
    }),
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
                const aborted = await fs
                    .readFile(path.join(markers, 'aborted-server.marker'), 'utf8')
                    .catch(() => null);
                expect(aborted).toBeNull();
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('server async run reports descendant-held stderr within the bounded grace', async () => {
        // Real public-boundary regression for the monitor's bounded-grace
        // contract: the wait:false child exits nonzero after writing 32KiB
        // of stderr, and its spawned descendant INHERITS that stderr and
        // stays alive until an owned release. The production monitor (in
        // the real server process) must log the primary's failure with the
        // retained tail while the descendant still holds the pipe — not
        // wait for descendant EOF. Asserted against the server's captured
        // stderr stream, never against a replica.
        const label = 'held';
        const markerDirRel = 'held-markers';
        const totalBytes = 32_768;
        const headSentinel = `HEAD-${label}-7f3adrop`;
        const tailSentinel = `TAIL-${label}-c9d1-END`;
        const releaseRel = `${markerDirRel}/release-${label}.marker`;
        const marker = (name: string) => path.join(workspace.root, markerDirRel, `${name}-${label}.marker`);
        const workspace = await SmokeWorkspace.create({
            seedFiles: {
                '.tasks/config.yml': BASE_CONFIG,
                '.tasks/automation.yml': `automation:
  rules:
    - name: Descendant-held stderr
      on:
        created:
${indentBlock(
    nodeRunActionYaml(
        [
            'fixtures/descendant-stderr.mjs',
            '--marker-dir',
            markerDirRel,
            '--label',
            label,
            '--total-bytes',
            String(totalBytes),
            '--head-sentinel',
            headSentinel,
            '--tail-sentinel',
            tailSentinel,
            '--exit',
            '7',
            '--release-file',
            releaseRel,
            '--hold-ms',
            '20000',
        ],
        { wait: false },
    ),
    10,
)}
`,
                'fixtures/descendant-stderr.mjs': DESCENDANT_STDERR_SCRIPT,
            },
        });

        let server: Awaited<ReturnType<typeof startLotarServer>> | undefined;
        const stderrChunks: string[] = [];
        // Binding endpoint evidence captured during cleanup (before the
        // workspace is disposed) and asserted after it: probe-delivered
        // proves the descendant really wrote to the inherited stderr AFTER
        // the primary exited, released proves the owned child finished.
        let probeOutcome: 'delivered' | 'epipe' | 'missing' = 'missing';
        let releasedSeen = false;
        try {
            server = await startLotarServer(workspace);
            server.raw.stderr?.on('data', (chunk: unknown) => {
                stderrChunks.push(String(chunk));
            });

            const createRes = await fetch(`${server.url}/api/tasks/add`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ title: 'Descendant held stderr task' }),
            });
            expect(createRes.status).toBe(201);
            const createBody = (await createRes.json()) as { data: { id: string } };

            await waitForMarker(marker('primary-started'), 10_000);
            await waitForMarker(marker('primary-exited'), 10_000);
            const primaryExitedAt = Date.now();

            // Bounded poll scaled to the 1s monitor grace (plus scheduler
            // slack): an EOF-waiting monitor would sit out the 20s hold.
            // The diagnostic spans two physical lines (the dropped-byte
            // notice embeds a newline before the retained payload), so it
            // is extracted from prefix line through sentinel, not by
            // single-line matching.
            const prefixMarker = `Async automation run for task ${createBody.data.id}`;
            const diagnosticDeadline = Date.now() + 5_000;
            let line: string | undefined;
            let diagnosticSeenAt = 0;
            while (Date.now() < diagnosticDeadline) {
                const full = stderrChunks.join('');
                const prefixIndex = full.indexOf(prefixMarker);
                const tailIndex = full.indexOf(tailSentinel, prefixIndex + 1);
                if (prefixIndex >= 0 && tailIndex > prefixIndex) {
                    const lineEnd = full.indexOf('\n', tailIndex + tailSentinel.length);
                    line = full.slice(prefixIndex, lineEnd === -1 ? undefined : lineEnd);
                    diagnosticSeenAt = Date.now();
                    break;
                }
                await new Promise((resolve) => setTimeout(resolve, 25));
            }
            expect(line).toBeDefined();

            // The descendant provably still holds the pipe: it never saw
            // the release and its holding marker is present.
            expect(fs.pathExistsSync(path.join(workspace.root, releaseRel))).toBe(false);
            expect(fs.pathExistsSync(marker('descendant-holding'))).toBe(true);
            expect(diagnosticSeenAt - primaryExitedAt).toBeLessThanOrEqual(5_000);

            expect(/exited with exit (?:status|code):? 7/.test(line ?? '')).toBe(true);
            expect(line).toContain('(stderr pipe still open, likely held by a descendant; tail truncated)');
            // Bounded tail: the head sentinel was dropped, the tail
            // sentinel retained, and the dropped-byte notice accounts for
            // everything above the 8KiB retention window.
            expect(line).not.toContain(headSentinel);
            const droppedMatch = /\[\+(\d+) earlier stderr bytes\]/.exec(line ?? '');
            expect(droppedMatch).not.toBeNull();
            const dropped = Number.parseInt(droppedMatch?.[1] ?? '0', 10);
            expect(dropped).toBeGreaterThanOrEqual(totalBytes - 8192 - 4096);
            expect((line ?? '').length).toBeLessThanOrEqual(8192 + 1024);
        } finally {
            // Release and bounded-await the owned descendant on every path;
            // its own hold bound is the final leak guard. Probe outcome is
            // read before the workspace disappears so the post-cleanup
            // assertions keep their evidence.
            await fs.writeFile(path.join(workspace.root, releaseRel), `${process.pid}\n`).catch(() => undefined);
            if (server) {
                await waitForMarker(marker('descendant-released'), 10_000)
                    .then(() => {
                        releasedSeen = true;
                    })
                    .catch(() => undefined);
            }
            probeOutcome = fs.pathExistsSync(marker('probe-delivered'))
                ? 'delivered'
                : fs.pathExistsSync(marker('probe-epipe'))
                  ? 'epipe'
                  : 'missing';
            if (server) {
                await server.stop();
            }
            await workspace.dispose();
        }
        // The descendant must have exited via the owned release (not its
        // leak-guard bound) and its post-primary-exit stderr probe must
        // have been delivered: the write endpoint was genuinely alive
        // after the primary died, so the held-note contract above was
        // asserted against a really-held pipe, not a marker file.
        expect(releasedSeen).toBe(true);
        expect(probeOutcome).toBe('delivered');
    });
});
