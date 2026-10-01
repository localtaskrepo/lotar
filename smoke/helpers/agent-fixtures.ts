import fs from 'fs-extra';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const fixturesDir = path.dirname(fileURLToPath(import.meta.url));

/** Node-based Copilot-wire mock agent (replaces the former mock-agent.sh). */
export const MOCK_AGENT_SCRIPT = path.resolve(fixturesDir, 'mock-agent.mjs');

/** Node-based gated mock agent used for live mid-run assertions. */
export const GATE_AGENT_SCRIPT = path.resolve(fixturesDir, 'gate-agent.mjs');

/** Absolute path of the Node runtime executing the smoke suite. */
export function nodeExecutable(): string {
    return process.execPath;
}

let launchCounter = 0;

/**
 * Write a portable launcher for a fixture script into `dir`.
 *
 * The agent runner contract spawns `<command> -p --output-format stream-json
 * --input-format stream-json <args...> <prompt>` for copilot profiles: the
 * base flags precede profile args, so `command` must be an executable that
 * tolerates leading unknown flags. A shebang copy of the fixture (Unix) and
 * a cmd forwarder (Windows) both satisfy that without bash/date/sleep, and
 * neither parses the runner flags — the fixtures read their own argv.
 */
async function writeAgentLauncher(dir: string, script: string): Promise<string> {
    launchCounter += 1;
    if (process.platform === 'win32') {
        const launcher = path.join(dir, `agent-launcher-${launchCounter}.cmd`);
        await fs.writeFile(launcher, `@"${nodeExecutable()}" "${script}" %*\r\n`, 'utf8');
        return launcher;
    }
    const launcher = path.join(dir, `agent-launcher-${launchCounter}.mjs`);
    // An import forwarder rather than a content copy: the fixture keeps its
    // own optional shebang, and process.argv seen by the fixture is exactly
    // the launcher's argv (runner flags + fixture flags + prompt).
    const fixtureUrl = pathToFileURL(script).href;
    await fs.writeFile(
        launcher,
        `#!${nodeExecutable()}\nawait import(${JSON.stringify(fixtureUrl)});\n`,
        'utf8',
    );
    await fs.chmod(launcher, 0o755);
    return launcher;
}

export interface AgentFixtureSpec {
    /** Fixture script the launcher forwards to; defaults to the mock agent. */
    readonly script?: string;
    /** Fixture arguments appended after the script path. */
    readonly args?: readonly string[];
    readonly description?: string;
}

export interface AgentFixtureAgents {
    /** Rendered `agents:` config section using the launcher commands. */
    readonly yaml: string;
    /** Launcher command path used for the given profile name. */
    command(profileName: string): string;
    dispose(): Promise<void>;
}

/**
 * Build one portable launcher per distinct fixture script and render the
 * matching `agents:` config section. Every value is quoted as a JSON string,
 * which is also a valid YAML double-quoted scalar, so Windows paths with
 * backslashes stay valid config.
 */
export async function createFixtureAgents(
    specs: Record<string, AgentFixtureSpec>,
    prefix = 'lotar-smoke-agents-',
): Promise<AgentFixtureAgents> {
    const dir = await mkdtemp(path.join(tmpdir(), prefix));
    const launchers = new Map<string, string>();

    const launcherFor = async (script: string): Promise<string> => {
        const existing = launchers.get(script);
        if (existing) {
            return existing;
        }
        const launcher = await writeAgentLauncher(dir, script);
        launchers.set(script, launcher);
        return launcher;
    };

    const commands = new Map<string, string>();
    const blocks: string[] = [];
    for (const [name, spec] of Object.entries(specs)) {
        const launcher = await launcherFor(spec.script ?? MOCK_AGENT_SCRIPT);
        commands.set(name, launcher);
        const args = spec.args ?? [];
        const lines = [
            `  ${name}:`,
            `    runner: "copilot"`,
            `    command: ${JSON.stringify(launcher)}`,
        ];
        if (args.length > 0) {
            lines.push(`    args:`);
            for (const arg of args) {
                lines.push(`      - ${JSON.stringify(arg)}`);
            }
        }
        if (spec.description) {
            lines.push(`    description: ${JSON.stringify(spec.description)}`);
        }
        blocks.push(lines.join('\n'));
    }
    if (blocks.length === 0) {
        await fs.remove(dir);
        throw new Error('createFixtureAgents requires at least one agent profile');
    }

    return {
        yaml: `agents:\n${blocks.join('\n')}`,
        command: (profileName: string): string => {
            const command = commands.get(profileName);
            if (!command) {
                throw new Error(`no agent profile named ${profileName}`);
            }
            return command;
        },
        dispose: () => fs.remove(dir),
    };
}

/** Indent every line of a block by `spaces` (blank lines stay blank). */
export function indentBlock(block: string, spaces: number): string {
    const pad = ' '.repeat(spaces);
    return block
        .split('\n')
        .map((line) => (line.length > 0 ? pad + line : line))
        .join('\n');
}

/**
 * Render a structured automation `run:` action that executes the Node
 * runtime directly (`Command::new(command).args(args)`, no sh/cmd), so the
 * command works identically on every supported platform. Values are quoted
 * as JSON strings to stay valid YAML double-quoted scalars.
 */
export function nodeRunActionYaml(
    scriptArgs: readonly string[],
    options: { wait?: boolean } = {},
): string {
    const lines = [
        `run:`,
        `  command: ${JSON.stringify(nodeExecutable())}`,
        `  args:`,
        ...scriptArgs.map((arg) => `    - ${JSON.stringify(arg)}`),
    ];
    if (options.wait === false) {
        lines.push(`  wait: false`);
    }
    return lines.join('\n');
}

export type GateAgentMode = 'copilot' | 'hold' | 'plain';

export interface GateAgentOptions {
    readonly mode: GateAgentMode;
    readonly label: string;
    readonly holdMs?: number;
    readonly exitCode?: number;
}

export interface GateFixture {
    readonly dir: string;
    readonly script: string;
    /** Fixture argv (without the script path) for the given options. */
    agentArgs(options: GateAgentOptions): string[];
    /** Write the shared release sentinel (idempotent). */
    release(): Promise<void>;
    startedExists(label: string): boolean;
    completedExists(label: string): boolean;
    /** Labels of children that wrote a started sentinel. */
    startedLabels(): string[];
    waitForStarted(label: string, timeoutMs?: number): Promise<void>;
    waitForCompleted(label: string, timeoutMs?: number): Promise<void>;
    /**
     * Release the gate and bounded-await completion of every started child.
     * Returns the labels that did not finish within the bound; it never
     * throws so cleanup paths stay safe.
     */
    drain(timeoutMs?: number): Promise<readonly string[]>;
    /** Release, short drain, and remove the gate directory. Never throws. */
    dispose(): Promise<void>;
}

async function pollUntil(
    predicate: () => boolean,
    timeoutMs: number,
    description: string,
): Promise<void> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (predicate()) {
            return;
        }
        await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error(`Timed out after ${timeoutMs}ms waiting for ${description}`);
}

const STARTED_PATTERN = /^started-(.*)\.sentinel$/;

export async function createGateFixture(prefix = 'lotar-smoke-gate-'): Promise<GateFixture> {
    const dir = await mkdtemp(path.join(tmpdir(), prefix));
    const releasePath = path.join(dir, 'release.sentinel');

    const sentinelPath = (name: string) => path.join(dir, name);

    const startedLabels = (): string[] => {
        const labels: string[] = [];
        for (const entry of fs.readdirSync(dir)) {
            const match = STARTED_PATTERN.exec(entry);
            if (match?.[1] !== undefined) {
                labels.push(match[1]);
            }
        }
        return labels;
    };

    const waitForCompleted = async (label: string, timeoutMs = 10_000): Promise<void> => {
        await pollUntil(
            () => fs.pathExistsSync(sentinelPath(`completed-${label}.sentinel`)),
            timeoutMs,
            `completed-${label}.sentinel in ${dir}`,
        );
    };

    const release = async (): Promise<void> => {
        await fs.ensureDir(dir);
        await fs.writeFile(releasePath, `${process.pid}\n`);
    };

    const drain = async (timeoutMs = 10_000): Promise<readonly string[]> => {
        await release();
        const pending = startedLabels().filter((label) => !fs.pathExistsSync(sentinelPath(`completed-${label}.sentinel`)));
        if (pending.length === 0) {
            return [];
        }
        const results = await Promise.allSettled(
            pending.map((label) => waitForCompleted(label, timeoutMs)),
        );
        return pending.filter((_, index) => results[index]?.status === 'rejected');
    };

    return {
        dir,
        script: GATE_AGENT_SCRIPT,
        agentArgs: (options: GateAgentOptions): string[] => {
            const args = ['--mode', options.mode, '--gate-dir', dir, '--label', options.label];
            if (options.holdMs !== undefined) {
                args.push('--hold-ms', String(options.holdMs));
            }
            if (options.exitCode !== undefined) {
                args.push('--exit', String(options.exitCode));
            }
            return args;
        },
        release,
        startedExists: (label: string) => fs.pathExistsSync(sentinelPath(`started-${label}.sentinel`)),
        completedExists: (label: string) => fs.pathExistsSync(sentinelPath(`completed-${label}.sentinel`)),
        startedLabels,
        waitForStarted: async (label: string, timeoutMs = 10_000): Promise<void> => {
            await pollUntil(
                () => fs.pathExistsSync(sentinelPath(`started-${label}.sentinel`)),
                timeoutMs,
                `started-${label}.sentinel in ${dir}`,
            );
        },
        waitForCompleted,
        drain,
        dispose: async (): Promise<void> => {
            try {
                await drain(1_000);
            } catch {
                // Cleanup must never mask a test failure.
            }
            await fs.remove(dir).catch(() => undefined);
        },
    };
}
