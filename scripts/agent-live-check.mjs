#!/usr/bin/env node
// Opt-in live check of LoTaR agent jobs against the real agent CLIs installed
// on this machine (DEV-199, DEV-202). It spends a little of each CLI's quota,
// so it never runs in CI. Usage:
//
//   npm run agents:live                         # every installed CLI, all scenarios
//   npm run agents:live -- --runner claude      # one CLI
//   npm run agents:live -- --scenario edit      # one scenario
//   npm run agents:live -- --keep               # keep the temp repositories
//
// Each scenario builds a throwaway git repository with a small bug and a
// ticket, drives LoTaR the way a user would, and checks the outcome.
// LOTAR_BIN overrides the binary (default: target/debug/lotar).

import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const lotarBin = process.env.LOTAR_BIN || path.join(root, 'target', 'debug', 'lotar');

// Cheap settings per CLI so a full run costs cents, not dollars.
const RUNNERS = {
    claude: { args: ['--model', 'haiku'] },
    codex: { args: ['-c', 'model_reasoning_effort=low'] },
    copilot: { args: [] },
    gemini: { args: ['-m', 'flash'] },
};

const BUGGY = 'def add(a, b):\n    return a - b\n';
const CHECK = "python3 -c 'import calc; print(calc.add(2,3))'";

function parseArgs(argv) {
    const options = { runners: [], scenarios: [], keep: false, timeout: 240 };
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i];
        if (arg === '--runner') options.runners.push(argv[++i]);
        else if (arg === '--scenario') options.scenarios.push(argv[++i]);
        else if (arg === '--keep') options.keep = true;
        else if (arg === '--timeout') options.timeout = Number(argv[++i]);
        else throw new Error(`Unknown argument: ${arg}`);
    }
    return options;
}

function installed(program) {
    return spawnSync(program, ['--version'], { encoding: 'utf8' }).status === 0;
}

function run(program, args, options = {}) {
    return spawnSync(program, args, { encoding: 'utf8', ...options });
}

function lotar(dir, args, env = {}, timeoutSeconds = 240) {
    return run(lotarBin, ['--tasks-dir', path.join(dir, '.tasks'), ...args], {
        cwd: dir,
        env: { ...process.env, LOTAR_AGENT_LOGS_DIR: path.join(dir, '.agent-logs'), ...env },
        timeout: timeoutSeconds * 1000,
    });
}

function makeRepo(runner, scenario) {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), `lotar-live-${runner}-${scenario}-`));
    const git = (...args) => execFileSync('git', args, { cwd: dir, stdio: 'ignore' });
    git('init', '-q', '-b', 'main');
    git('config', 'user.email', 'live@example.com');
    git('config', 'user.name', 'Live Check');
    fs.writeFileSync(path.join(dir, 'calc.py'), BUGGY);
    git('add', '.');
    git('commit', '-qm', 'init');
    return dir;
}

function writeConfig(dir, runner, extra = '') {
    const args = JSON.stringify(RUNNERS[runner].args);
    fs.mkdirSync(path.join(dir, '.tasks'), { recursive: true });
    fs.writeFileSync(
        path.join(dir, '.tasks', 'config.yml'),
        [
            'default:',
            '  project: LIVE',
            'issue:',
            '  states: [Todo, InProgress, NeedsReview, Done, HelpNeeded]',
            'agents:',
            '  fixer:',
            `    runner: ${runner}`,
            `    args: ${args}`,
            extra,
        ].join('\n'),
    );
}

function addTicket(dir) {
    const added = lotar(dir, ['add', 'Fix add() in calc.py: it subtracts instead of adding', '--type', 'bug']);
    if (added.status !== 0) throw new Error(`lotar add failed: ${added.stderr}`);
    return 'LIVE-1';
}

function fixed(file) {
    return fs.existsSync(file) && fs.readFileSync(file, 'utf8').includes('a + b');
}

function jobLog(dir, output) {
    const id = /Job (job-\S+) started/.exec(output)?.[1];
    if (!id) return '';
    return lotar(dir, ['agent', 'logs', id]).stdout;
}

// -- Scenarios -------------------------------------------------------------------

const SCENARIOS = {
    /** A direct `lotar agent run` edits the file in the repository checkout. */
    edit(runner, options) {
        const dir = makeRepo(runner, 'edit');
        writeConfig(dir, runner);
        const ticket = addTicket(dir);
        const result = lotar(
            dir,
            [
                'agent',
                'run',
                ticket,
                `Fix the bug described in the ticket, then run ${CHECK} to check it.`,
                '--agent',
                'fixer',
                '--wait',
                '--timeout-seconds',
                String(options.timeout),
            ],
            {},
            options.timeout + 30,
        );
        const output = `${result.stdout}${result.stderr}`;
        return {
            dir,
            ok: result.status === 0 && /completed/.test(output) && fixed(path.join(dir, 'calc.py')),
            detail: output.trim().split('\n').slice(-3).join(' | '),
            log: jobLog(dir, output),
        };
    },

    /**
     * Assigning the ticket to the agent profile queues the job; automation
     * moves it to InProgress on start and NeedsReview on completion, and the
     * work happens on an agent/ branch in its own worktree.
     */
    pipeline(runner, options) {
        const dir = makeRepo(runner, 'pipeline');
        writeConfig(dir, runner, ['agent:', '  worktree:', '    enabled: true'].join('\n'));
        fs.writeFileSync(
            path.join(dir, '.tasks', 'automation.yml'),
            [
                'automation:',
                '  rules:',
                '    - name: Agent lifecycle',
                '      when: { assignee: "@agent" }',
                '      on:',
                '        job_started: { set: { status: InProgress } }',
                '        job_completed: { set: { status: NeedsReview, assignee: "@reporter" } }',
                '        job_failed: { set: { status: HelpNeeded } }',
            ].join('\n'),
        );
        const ticket = addTicket(dir);
        const assigned = lotar(dir, ['assignee', ticket, '@fixer']);
        if (assigned.status !== 0) return { dir, ok: false, detail: `assign failed: ${assigned.stderr}` };
        // The queue worker runs in the background; wait for automation to settle.
        const deadline = Date.now() + options.timeout * 1000;
        let status = '';
        const ticketFile = path.join(dir, '.tasks', 'LIVE', '1.yml');
        while (Date.now() < deadline) {
            status = /^status:\s*(\S+)/m.exec(fs.readFileSync(ticketFile, 'utf8'))?.[1] ?? '';
            if (status === 'NeedsReview' || status === 'HelpNeeded') break;
            spawnSync('sleep', ['3']);
        }
        const worktrees = run('git', ['worktree', 'list', '--porcelain'], { cwd: dir }).stdout;
        const worktree = /^worktree (.*LIVE-1)$/m.exec(worktrees)?.[1];
        const commits = worktree ? run('git', ['log', '--oneline', 'main..HEAD'], { cwd: worktree }).stdout.trim() : '';
        const commented = /^comments:/m.test(fs.readFileSync(ticketFile, 'utf8'));
        const ok =
            status === 'NeedsReview' &&
            !!worktree &&
            fixed(path.join(worktree, 'calc.py')) &&
            !fixed(path.join(dir, 'calc.py'));
        const logs = lotar(dir, ['agent', 'list-jobs', '-n', '1']).stdout;
        return {
            dir,
            ok,
            detail: `final status ${status || '(none)'}; worktree ${worktree ? 'found' : 'missing'}; ticket comment ${commented ? 'added' : 'missing'}; agent commits ${commits ? commits.split('\n').length : 0}`,
            log: logs,
        };
    },
};

// -- Main ----------------------------------------------------------------------------

const options = parseArgs(process.argv.slice(2));
if (!fs.existsSync(lotarBin)) {
    console.error(`LoTaR binary not found at ${lotarBin}; build it first (cargo build) or set LOTAR_BIN.`);
    process.exit(2);
}
const runners = (options.runners.length ? options.runners : Object.keys(RUNNERS)).filter((runner) => {
    if (!RUNNERS[runner]) throw new Error(`Unknown runner: ${runner}`);
    if (installed(runner)) return true;
    console.log(`- ${runner}: not installed, skipped`);
    return false;
});
const scenarios = options.scenarios.length ? options.scenarios : Object.keys(SCENARIOS);

let failures = 0;
for (const runner of runners) {
    for (const scenario of scenarios) {
        const started = Date.now();
        let outcome;
        try {
            outcome = SCENARIOS[scenario](runner, options);
        } catch (error) {
            outcome = { ok: false, detail: String(error) };
        }
        const seconds = Math.round((Date.now() - started) / 1000);
        console.log(`${outcome.ok ? 'PASS' : 'FAIL'} ${runner} ${scenario} (${seconds}s): ${outcome.detail}`);
        if (!outcome.ok) {
            failures += 1;
            if (outcome.log)
                console.log(
                    outcome.log
                        .split('\n')
                        .slice(-25)
                        .map((line) => `    ${line.slice(0, 200)}`)
                        .join('\n'),
                );
        }
        if (outcome.dir) {
            if (options.keep || !outcome.ok) console.log(`    kept: ${outcome.dir}`);
            else fs.rmSync(outcome.dir, { recursive: true, force: true });
        }
    }
}
process.exit(failures ? 1 : 0);
