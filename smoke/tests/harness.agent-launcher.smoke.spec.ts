import { spawn, type ChildProcess } from 'node:child_process';
import fs from 'fs-extra';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { describe, expect, it } from 'vitest';
import {
    GATE_AGENT_SCRIPT,
    createFixtureAgents,
    nodeExecutable,
    windowsLauncherContent,
} from '../helpers/agent-fixtures.js';

// Windows counterfactuals for the agent fixture launcher contract behind
// the two Windows-only portable smoke failures (DEV-101):
//  1. a pure string-level model proving the launcher's baked-in fixture
//     arguments survive arbitrary mangling of the forwarded `%*` tail
//     (SIMULATED cmd behavior — not a real Windows execution claim), and
//  2./3. real portable processes proving the gate protocol works with
//     spaces in the gate directory (and label), both directly and through
//     the platform launcher, on every platform the suite runs on.

const KNOWN_FLAGS = ['--gate-dir', '--label', '--mode', '--hold-ms', '--exit', '--session'];

/** Mirrors gate-agent.mjs/mock-agent.mjs parseArgs: pairwise, last wins. */
function parseFixtureArgs(argv: readonly string[]): Record<string, string> {
    const parsed: Record<string, string> = {};
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i];
        if (KNOWN_FLAGS.includes(arg)) {
            const value = argv[i + 1];
            if (value !== undefined) {
                parsed[arg] = value;
                i += 1;
            }
        }
    }
    return parsed;
}

/** Strip one pair of surrounding double quotes, as node's argv parsing does. */
function dequote(token: string): string {
    if (token.length >= 2 && token.startsWith('"') && token.endsWith('"')) {
        return token.slice(1, -1);
    }
    return token;
}

/** Quote-aware whitespace tokenization: how cmd/node treat quoted segments. */
function tokenizeQuoted(line: string): string[] {
    const tokens: string[] = [];
    let current = '';
    let quoted = false;
    for (const char of line) {
        if (char === '"') {
            quoted = !quoted;
            current += char;
        } else if (char === ' ' && !quoted) {
            if (current.length > 0) {
                tokens.push(current);
                current = '';
            }
        } else {
            current += char;
        }
    }
    if (current.length > 0) {
        tokens.push(current);
    }
    return tokens;
}

interface ExitOutcome {
    readonly code: number | null;
    readonly stderr: string;
}

function awaitExit(child: ChildProcess, boundMs = 20_000): Promise<ExitOutcome> {
    let stderr = '';
    child.stderr?.setEncoding('utf8');
    child.stderr?.on('data', (chunk: string) => {
        stderr += chunk;
    });
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
            reject(new Error(`fixture child did not exit within ${boundMs}ms; stderr: ${stderr}`));
        }, boundMs);
        child.on('error', (error: Error) => {
            clearTimeout(timer);
            reject(error);
        });
        child.on('close', (code) => {
            clearTimeout(timer);
            resolve({ code, stderr });
        });
    });
}

async function waitForFile(filePath: string, timeoutMs: number): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (fs.pathExistsSync(filePath)) {
            return;
        }
        await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error(`Timed out after ${timeoutMs}ms waiting for ${filePath}`);
}

describe('agent fixture launcher Windows counterfactuals', () => {
    it('bakes fixture args so a mangled forwarded tail cannot break the gate contract', () => {
        const script = 'D:\\a\\lotar\\lotar\\smoke\\helpers\\gate-agent.mjs';
        const spacedGateDir = 'C:\\Users\\RUNNER~1\\AppData\\Local\\Temp\\lotar smoke gate api D7dk7J';
        const fixtureArgs = [
            '--mode',
            'copilot',
            '--gate-dir',
            spacedGateDir,
            '--label',
            'progress',
            '--hold-ms',
            '20000',
        ];

        const content = windowsLauncherContent(script, fixtureArgs);

        // The forwarder shape: node + script verbatim, %* first, baked tail last.
        expect(content.endsWith('\r\n')).toBe(true);
        expect(content).toContain(`@"${nodeExecutable()}" "${script}" %*`);
        expect(content).toContain(fixtureArgs.map((arg) => `"${arg}"`).join(' '));

        // Simulated win32 cmd mangling of the FORWARDED tail: every quote
        // lost and every space becoming a token boundary (the worst case
        // the runner/wrapper/cmd chain could produce). This is a model, not
        // a real Windows execution.
        const mangledTail = [
            '-p',
            '--output-format',
            'stream-json',
            '--input-format',
            'stream-json',
            '--mode',
            'copilot',
            '--gate-dir',
            ...spacedGateDir.split(' '),
            '--label',
            'pro gress',
            'Test prompt',
        ].join(' ');
        const bakedTail = fixtureArgs.map((arg) => `"${arg}"`).join(' ');

        const childArgv = [
            script,
            ...mangledTail.split(' '),
            ...tokenizeQuoted(bakedTail).map(dequote),
        ];

        // The fixture parsers let the LAST occurrence of a repeated flag
        // win, and the baked occurrence is last, so the exact baked values
        // must win over any mangled forwarded copies.
        const parsed = parseFixtureArgs(childArgv);
        expect(parsed['--gate-dir']).toBe(spacedGateDir);
        expect(parsed['--label']).toBe('progress');
        expect(parsed['--mode']).toBe('copilot');
        expect(parsed['--hold-ms']).toBe('20000');

        // Negative control: without the baked tail (the pre-fix launcher
        // shape), the same mangled forwarding truncates the spaced gate dir,
        // which is how a Windows-only started-sentinel timeout manifests.
        const oldStyleArgv = [script, ...mangledTail.split(' ')];
        expect(parseFixtureArgs(oldStyleArgv)['--gate-dir']).not.toBe(spacedGateDir);
    });

    it('rejects batch-unsafe launcher values and accepts native Windows path shapes', () => {
        const script = 'D:\\a\\lotar\\lotar\\smoke\\helpers\\gate-agent.mjs';
        for (const unsafe of [
            'val"ue', // double quote breaks the baked quoting
            'val\nue', // control character corrupts the cmd line
            'val%ue', // percent enables cmd variable expansion
            'C:\\path\\ends\\', // trailing backslash escapes the closing quote
        ]) {
            expect(() => windowsLauncherContent(script, [unsafe])).toThrow();
            expect(() => windowsLauncherContent(unsafe, [])).toThrow();
        }

        // Native Windows path shapes with internal backslashes and spaces
        // stay accepted; only genuinely batch-unsafe values are rejected.
        const native = windowsLauncherContent('C:\\tools\\agent.cmd', [
            '--mode',
            'copilot',
            '--gate-dir',
            'C:\\Users\\RUNNER~1\\AppData\\Local\\Temp\\lotar smoke gate api X1',
            '--label',
            'progress',
        ]);
        expect(native).toContain('%*');
        expect(native).toContain(
            '"--gate-dir" "C:\\Users\\RUNNER~1\\AppData\\Local\\Temp\\lotar smoke gate api X1"',
        );
    });

    it('gate agent boots, holds, and releases with a gate dir containing spaces', async ({ expect }) => {
        const dir = path.join(tmpdir(), `lotar smoke gate spaced-${process.pid}-${Date.now().toString(36)}`);
        await fs.ensureDir(dir);
        try {
            const child = spawn(
                nodeExecutable(),
                [
                    GATE_AGENT_SCRIPT,
                    '--mode',
                    'plain',
                    '--gate-dir',
                    dir,
                    '--label',
                    'spaced probe',
                    '--hold-ms',
                    '15000',
                ],
                { stdio: ['ignore', 'ignore', 'pipe'] },
            );
            const exit = awaitExit(child);

            // Creation is visible before the writer finishes the JSON payload.
            await expect.poll(async () => {
                try {
                    return JSON.parse(await fs.readFile(path.join(dir, 'boot-spaced probe.sentinel'), 'utf8'));
                } catch {
                    return undefined;
                }
            }, { timeout: 10_000 }).toMatchObject({
                pid: expect.any(Number), platform: expect.any(String), argv: expect.arrayContaining([dir]),
            });

            await waitForFile(path.join(dir, 'started-spaced probe.sentinel'), 10_000);
            expect(fs.pathExistsSync(path.join(dir, 'completed-spaced probe.sentinel'))).toBe(false);

            await fs.writeFile(path.join(dir, 'release.sentinel'), `${process.pid}\n`);
            const outcome = await exit;
            expect(outcome.code).toBe(0);
            await waitForFile(path.join(dir, 'completed-spaced probe.sentinel'), 10_000);
        } finally {
            await fs.remove(dir).catch(() => undefined);
        }
    });

    it('launcher keeps the gate reachable through runner-shaped flags and a spaced gate dir', async () => {
        const dir = path.join(
            tmpdir(),
            `lotar smoke launcher-${process.pid}-${Date.now().toString(36)}`,
        );
        await fs.ensureDir(dir);
        const fixtureArgs = [
            '--mode',
            'plain',
            '--gate-dir',
            dir,
            '--label',
            'launcher',
            '--hold-ms',
            '15000',
        ];
        const agents = await createFixtureAgents(
            { gated: { script: GATE_AGENT_SCRIPT, args: fixtureArgs } },
            'lotar-smoke-launcher-',
        );

        try {
            const launcher = agents.command('gated');
            // Mirror the product's copilot spawn shape: base flags precede
            // profile args and the prompt is the final argument.
            const runnerArgs = [
                '-p',
                '--output-format',
                'stream-json',
                '--input-format',
                'stream-json',
                ...fixtureArgs,
                'Test prompt',
            ];
            const child =
                process.platform === 'win32'
                    ? spawn('cmd', ['/c', launcher, ...runnerArgs], { stdio: ['ignore', 'ignore', 'pipe'] })
                    : spawn(launcher, runnerArgs, { stdio: ['ignore', 'ignore', 'pipe'] });
            const exit = awaitExit(child);

            await waitForFile(path.join(dir, 'boot-launcher.sentinel'), 10_000);
            await waitForFile(path.join(dir, 'started-launcher.sentinel'), 10_000);
            expect(fs.pathExistsSync(path.join(dir, 'completed-launcher.sentinel'))).toBe(false);

            await fs.writeFile(path.join(dir, 'release.sentinel'), `${process.pid}\n`);
            const outcome = await exit;
            expect(outcome.code).toBe(0);
            await waitForFile(path.join(dir, 'completed-launcher.sentinel'), 10_000);
        } finally {
            await fs.writeFile(path.join(dir, 'release.sentinel'), `${process.pid}\n`).catch(
                () => undefined,
            );
            await agents.dispose();
            await fs.remove(dir).catch(() => undefined);
        }
    });
});
