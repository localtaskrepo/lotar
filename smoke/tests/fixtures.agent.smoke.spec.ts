import { spawn } from 'node:child_process';
import process from 'node:process';
import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import {
    GATE_AGENT_SCRIPT,
    MOCK_AGENT_SCRIPT,
    createFixtureAgents,
    createGateFixture,
    nodeExecutable,
    nodeRunActionYaml,
} from '../helpers/agent-fixtures.js';

interface SpawnOutcome {
    readonly code: number | null;
    readonly signal: NodeJS.Signals | null;
    readonly stdout: string;
    readonly stderr: string;
}

interface TrackedScript {
    readonly outcome: Promise<SpawnOutcome>;
}

/**
 * Spawn a program with the given argv and return a promise for its
 * fully-reaped exit, so a hung child can never leak past the bound: the
 * timeout kills the child and the close listener still settles the outcome.
 */
function startExecutable(program: string, args: readonly string[], timeoutMs = 15_000): TrackedScript {
    const outcome = new Promise<SpawnOutcome>((resolve, reject) => {
        const child = spawn(program, args, {
            stdio: ['ignore', 'pipe', 'pipe'],
        });
        let stdout = '';
        let stderr = '';
        let settled = false;

        const timer = setTimeout(() => {
            if (settled) {
                return;
            }
            settled = true;
            child.kill('SIGKILL');
            reject(new Error(`program ${program} did not exit within ${timeoutMs}ms`));
        }, timeoutMs);

        child.stdout.setEncoding('utf8');
        child.stderr.setEncoding('utf8');
        child.stdout.on('data', (chunk: string) => {
            stdout += chunk;
        });
        child.stderr.on('data', (chunk: string) => {
            stderr += chunk;
        });
        child.on('error', (error: Error) => {
            if (settled) {
                return;
            }
            settled = true;
            clearTimeout(timer);
            reject(error);
        });
        child.on('close', (code, signal) => {
            clearTimeout(timer);
            if (settled) {
                return;
            }
            settled = true;
            resolve({ code, signal, stdout, stderr });
        });
    });
    return { outcome };
}

/** Spawn a fixture script with the real Node runtime. */
function startNodeScript(script: string, args: readonly string[], timeoutMs = 15_000): TrackedScript {
    return startExecutable(nodeExecutable(), [script, ...args], timeoutMs);
}

function parseWireLines(stdout: string): Array<Record<string, unknown>> {
    return stdout
        .trim()
        .split('\n')
        .map((line) => JSON.parse(line) as Record<string, unknown>);
}

describe('agent fixture behavior', () => {
    it('mock agent emits the copilot wire sequence and exits zero', async () => {
        const child = startNodeScript(MOCK_AGENT_SCRIPT, []);
        const outcome = await child.outcome;

        expect(outcome.code).toBe(0);
        expect(outcome.signal).toBeNull();

        const events = parseWireLines(outcome.stdout);
        expect(events).toHaveLength(5);
        expect(events[0]).toMatchObject({ type: 'system', subtype: 'init' });
        expect(events[0]?.session_id).toEqual(expect.any(String));
        expect(events[1]).toMatchObject({ type: 'message', delta: true, content: 'Hello' });
        expect(events[2]).toMatchObject({ type: 'message', delta: true, content: ' from mock agent!' });
        expect(events[3]).toMatchObject({ type: 'message', content: 'Hello from mock agent!' });
        expect(events[3]?.delta).toBeUndefined();
        expect(events[4]).toMatchObject({ type: 'result', result: 'Completed successfully' });
    });

    it('mock agent honors a requested non-zero exit code', async () => {
        const child = startNodeScript(MOCK_AGENT_SCRIPT, ['--exit', '7']);
        const outcome = await child.outcome;

        expect(outcome.code).toBe(7);
        expect(parseWireLines(outcome.stdout)).toHaveLength(5);
    });

    it('gate agent holds until released, then completes with both phases', async () => {
        const gate = await createGateFixture('fixture-gate-');
        try {
            const child = startNodeScript(GATE_AGENT_SCRIPT, gate.agentArgs({ mode: 'copilot', label: 'probe' }));

            await gate.waitForStarted('probe', 10_000);
            // The child cannot complete before the release file exists; the
            // hold bound is far larger than this check, so absence here is a
            // protocol property, not a timing race.
            expect(gate.completedExists('probe')).toBe(false);

            await gate.release();
            const outcome = await child.outcome;

            expect(outcome.code).toBe(0);
            const events = parseWireLines(outcome.stdout);
            expect(events[0]).toMatchObject({ type: 'system', subtype: 'init' });
            expect(events.some((e) => e.content === 'phase one' && e.delta === true)).toBe(true);
            expect(events.some((e) => e.content === 'phase one / phase two done')).toBe(true);
            expect(events[events.length - 1]).toMatchObject({ type: 'result', result: 'Completed successfully' });

            await gate.waitForCompleted('probe', 5_000);
            expect(await gate.drain(1_000)).toEqual([]);
        } finally {
            await gate.dispose();
        }
    });

    it('gate agent self-exits at its owned hold bound when never released', async () => {
        const gate = await createGateFixture('fixture-gate-bound-');
        try {
            const child = startNodeScript(
                GATE_AGENT_SCRIPT,
                gate.agentArgs({ mode: 'plain', label: 'orphan', holdMs: 300 }),
            );

            const outcome = await child.outcome;
            expect(outcome.code).toBe(0);
            await gate.waitForCompleted('orphan', 5_000);
        } finally {
            await gate.dispose();
        }
    });

    it('portable launcher tolerates leading runner flags like the real spawn shape', async () => {
        const fixtureAgents = await createFixtureAgents(
            { mock: {} },
            'fixture-agents-launcher-',
        );

        try {
            const launcher = fixtureAgents.command('mock');
            // Mirror the product's copilot spawn shape: base flags precede any
            // profile args and the prompt is the final argument. Node's CLI
            // would reject --output-format, so this proves the launcher (not
            // the raw node executable) is the configured command.
            const runnerArgs = [
                '-p',
                '--output-format',
                'stream-json',
                '--input-format',
                'stream-json',
                'ignored prompt text',
            ];
            const child = process.platform === 'win32'
                ? startExecutable('cmd', ['/c', launcher, ...runnerArgs])
                : startExecutable(launcher, runnerArgs);
            const outcome = await child.outcome;

            expect(outcome.code).toBe(0);
            expect(outcome.signal).toBeNull();
            const events = parseWireLines(outcome.stdout);
            expect(events).toHaveLength(5);
            expect(events[0]).toMatchObject({ type: 'system', subtype: 'init' });
            expect(events[4]).toMatchObject({ type: 'result', result: 'Completed successfully' });
        } finally {
            await fixtureAgents.dispose();
        }
    });

    it('renders agent and automation run YAML that parses back to the intended contracts', async () => {
        const fixtureAgents = await createFixtureAgents(
            {
                ok: { description: 'completes' },
                broken: { args: ['--exit', '1'] },
            },
            'fixture-agents-yaml-',
        );

        try {
        const agents = parse(fixtureAgents.yaml) as {
            agents: Record<string, { runner: string; command: string; args?: string[]; description?: string }>;
        };

        expect(agents.agents.ok?.runner).toBe('copilot');
        expect(agents.agents.ok?.command).toBe(fixtureAgents.command('ok'));
        expect(agents.agents.ok?.args).toBeUndefined();
        expect(agents.agents.ok?.description).toBe('completes');
        expect(agents.agents.broken?.args).toEqual(['--exit', '1']);
        } finally {
            await fixtureAgents.dispose();
        }

        const gate = await createGateFixture('fixture-gate-yaml-');
        try {
            const runBlock = nodeRunActionYaml(
                [gate.script, ...gate.agentArgs({ mode: 'plain', label: 'yaml' })],
                { wait: false },
            );
            const automation = parse(`automation:
  rules:
    - name: Rendered run
      on:
        created:
${runBlock
    .split('\n')
    .map((line) => (line.length > 0 ? `          ${line}` : line))
    .join('\n')}
`) as {
                automation: { rules: Array<{ on: { created: Record<string, unknown> } }> };
            };

            const run = automation.automation.rules[0]?.on?.created?.run as {
                command: string;
                args: string[];
                wait: boolean;
            };
            expect(run.command).toBe(nodeExecutable());
            expect(run.args[0]).toBe(gate.script);
            expect(run.args).toEqual([gate.script, ...gate.agentArgs({ mode: 'plain', label: 'yaml' })]);
            expect(run.wait).toBe(false);
        } finally {
            await gate.dispose();
        }
    });
});
