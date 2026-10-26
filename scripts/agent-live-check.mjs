#!/usr/bin/env node
// Opt-in live check of LoTaR agent jobs against the real agent CLIs installed
// on this machine (DEV-199, DEV-202). It spends a little of each CLI's quota,
// so it never runs in CI. Usage:
//
//   npm run agents:live                         # every installed CLI, all scenarios
//   npm run agents:live -- --runner claude      # one CLI
//   npm run agents:live -- --scenario edit      # one scenario (edit, settings, pipeline, template, handback)
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
import { parse, stringify } from 'yaml';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const lotarBin = process.env.LOTAR_BIN || path.join(root, 'target', 'debug', 'lotar');

// Cheap settings per CLI so a full run costs cents, not dollars.
const RUNNERS = {
    claude: { args: ['--model', 'haiku'] },
    codex: { args: ['-c', 'model_reasoning_effort=low'] },
    copilot: { args: [] },
    gemini: { args: ['-m', 'flash'] },
};

// Model and effort each CLI is asked to use in the settings scenario.
const SETTINGS = {
    claude: { model: 'haiku', effort: 'low' },
    codex: { effort: 'low' },
    // Copilot plans without model choice only allow `auto`, which takes no effort.
    copilot: { model: 'auto' },
    gemini: { model: 'flash' },
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
        env: {
            ...process.env,
            // Agents call `lotar` themselves; make that the binary under test.
            PATH: `${path.dirname(lotarBin)}${path.delimiter}${process.env.PATH}`,
            LOTAR_AGENT_LOGS_DIR: path.join(dir, '.agent-logs'),
            ...env,
        },
        timeout: timeoutSeconds * 1000,
    });
}

function makeRepo(runner, scenario) {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), `lotar-live-${runner}-${scenario}-`));
    const git = (...args) => execFileSync('git', args, { cwd: dir, stdio: 'ignore' });
    git('init', '-q', '-b', 'main');
    git('config', 'user.email', 'live@example.com');
    git('config', 'user.name', 'Live Check');
    // Signing through a key agent (1Password, gpg-agent) can't reach its
    // socket from inside an agent sandbox; this repository is throwaway.
    git('config', 'commit.gpgsign', 'false');
    fs.writeFileSync(path.join(dir, 'calc.py'), BUGGY);
    git('add', '.');
    git('commit', '-qm', 'init');
    return dir;
}

function writeConfig(dir, runner, extra = '', args = JSON.stringify(RUNNERS[runner].args)) {
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

    /** `lotar agent run --model/--effort` reaches the CLI as its own flags. */
    settings(runner, options) {
        const dir = makeRepo(runner, 'settings');
        writeConfig(dir, runner, '', '[]');
        const ticket = addTicket(dir);
        const { model, effort } = SETTINGS[runner];
        const flags = [...(model ? ['--model', model] : []), ...(effort ? ['--effort', effort] : [])];
        const result = lotar(
            dir,
            [
                'agent',
                'run',
                ticket,
                'Reply with the single word OK. Do not change any files.',
                '--agent',
                'fixer',
                ...flags,
                '--wait',
                '--timeout-seconds',
                String(options.timeout),
            ],
            {},
            options.timeout + 30,
        );
        const output = `${result.stdout}${result.stderr}`;
        const expected = [model && `model ${model}`, effort && `effort ${effort}`].filter(Boolean).join(', ');
        return {
            dir,
            ok: result.status === 0 && /completed/.test(output) && output.includes(`Settings: ${expected}`),
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

    /**
     * The shipped agent-pipeline template, set up with `lotar init`: one
     * assignment runs implement → test → merge in the ticket's worktree, the
     * fix lands on main through a fast-forward, and the ticket ends Done
     * with its reporter.
     */
    template(runner, options) {
        const dir = makeRepo(runner, 'template');
        const setup = setUpTemplate(dir, runner);
        if (setup) return { dir, ok: false, detail: setup };
        const ticket = addTicket(dir);
        const ticketYaml = runPipeline(dir, ticket, options);
        if (ticketYaml.error) return { dir, ok: false, detail: ticketYaml.error };
        const mainLog = run('git', ['log', '--oneline', '-n', '5', 'main'], { cwd: dir }).stdout.trim();
        const agentCommits = mainLog.split('\n').length - 1;
        const jobs = lotar(dir, ['agent', 'list-jobs']).stdout;
        const ok =
            ticketYaml.status === 'Done' &&
            ticketYaml.assignee === 'Live Check' &&
            fixed(path.join(dir, 'calc.py')) &&
            agentCommits > 0;
        return {
            dir,
            ok,
            detail: `final status ${ticketYaml.status}, assignee ${ticketYaml.assignee ?? '(none)'}; main ${fixed(path.join(dir, 'calc.py')) ? 'fixed' : 'not fixed'} with ${agentCommits} agent commit(s); comments ${ticketYaml.comments?.length ?? 0}`,
            log: jobs,
        };
    },

    /**
     * A ticket the agent can't finish without a decision: with the
     * agent-pipeline template it hands the ticket back to the reporter in
     * HelpNeeded, and the pipeline stops there instead of moving on.
     */
    handback(runner, options) {
        const dir = makeRepo(runner, 'handback');
        const setup = setUpTemplate(dir, runner);
        if (setup) return { dir, ok: false, detail: setup };
        const added = lotar(dir, [
            'add',
            'Change the greeting in greet.py to the wording marketing approved',
            '--type',
            'feature',
            '--description',
            'Marketing approved new greeting wording last week. Update greet.py to use it.',
        ]);
        if (added.status !== 0) return { dir, ok: false, detail: `lotar add failed: ${added.stderr}` };
        fs.writeFileSync(path.join(dir, 'greet.py'), 'def greet(name):\n    return f"Hello, {name}"\n');
        execFileSync('git', ['add', 'greet.py'], { cwd: dir });
        execFileSync('git', ['commit', '-qm', 'greet'], { cwd: dir });
        const ticketYaml = runPipeline(dir, 'LIVE-1', options);
        if (ticketYaml.error) return { dir, ok: false, detail: ticketYaml.error };
        const jobs = lotar(dir, ['agent', 'list-jobs']).stdout;
        const jobCount = jobs.split('\n').filter((line) => line.startsWith('job-')).length;
        const ok =
            ticketYaml.status === 'HelpNeeded' &&
            ticketYaml.assignee === 'Live Check' &&
            !(ticketYaml.tags ?? []).includes('agent-failed') &&
            jobCount === 1;
        const question = ticketYaml.comments?.at(-1)?.text ?? '';
        return {
            dir,
            ok,
            detail: `final status ${ticketYaml.status}, assignee ${ticketYaml.assignee ?? '(none)'}; jobs ${jobCount}; question: ${question.slice(0, 120)}`,
            log: jobs,
        };
    },
};

/** `lotar init --template agent-pipeline` with every phase on `runner`. */
function setUpTemplate(dir, runner) {
    const init = lotar(dir, ['init', '--yes', '--project=LIVE', '--prefix=LIVE', '--template=agent-pipeline']);
    if (init.status !== 0) return `init failed: ${init.stderr}`;
    const configFile = path.join(dir, '.tasks', 'LIVE', 'config.yml');
    const config = parse(fs.readFileSync(configFile, 'utf8'));
    for (const profile of Object.values(config.agents)) {
        profile.runner = runner;
        profile.args = RUNNERS[runner].args;
    }
    fs.writeFileSync(configFile, stringify(config));
    return null;
}

/** Assign the ticket to @implement and wait until it is Done or HelpNeeded. */
function runPipeline(dir, ticket, options) {
    const assigned = lotar(dir, ['assignee', ticket, '@implement']);
    if (assigned.status !== 0) return { error: `assign failed: ${assigned.stderr}` };
    // Phases run one after another; allow each the full timeout.
    const deadline = Date.now() + 3 * options.timeout * 1000;
    const ticketFile = path.join(dir, '.tasks', 'LIVE', `${ticket.split('-')[1]}.yml`);
    let ticketYaml = {};
    while (Date.now() < deadline) {
        ticketYaml = parse(fs.readFileSync(ticketFile, 'utf8'));
        // A job whose log has no final status yet lists as running or unknown.
        const busy = /\t(running|unknown|queued)\t/.test(lotar(dir, ['agent', 'list-jobs']).stdout);
        if (!busy && (ticketYaml.status === 'Done' || ticketYaml.status === 'HelpNeeded')) break;
        spawnSync('sleep', ['5']);
    }
    return ticketYaml;
}

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
