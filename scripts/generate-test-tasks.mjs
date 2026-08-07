#!/usr/bin/env node
// Generate a throwaway set of LoTaR tasks into a directory of your choice, so you can
// exercise the CLI/UI/manual flows without polluting the committed `.tasks/` backlog.
//
// Usage:
//   node scripts/generate-test-tasks.mjs                       # temp dir, 40 tasks, project DEMO
//   node scripts/generate-test-tasks.mjs --count 120           # more tasks
//   node scripts/generate-test-tasks.mjs --dir .tasks-demo     # use a specific dir
//   node scripts/generate-test-tasks.mjs --serve               # also launch `lotar serve` against it
//
// The script writes a project config with a broad vocabulary, then seeds tasks with a
// realistic spread of status/priority/type/assignee/tags/effort/due-date/custom-fields.
// It uses the built `lotar` binary (LOTAR_BINARY_PATH/LOTAR_BIN, else target/{smoke,release}).

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');
const BIN_NAME = process.platform === 'win32' ? 'lotar.exe' : 'lotar';

function resolveBinary() {
    const explicit = process.env.LOTAR_BINARY_PATH || process.env.LOTAR_BIN;
    if (explicit) return explicit;
    for (const candidate of [
        path.join(ROOT, 'target', 'smoke', BIN_NAME),
        path.join(ROOT, 'target', 'release', BIN_NAME),
    ]) {
        if (existsSync(candidate)) return candidate;
    }
    return path.join(ROOT, 'target', 'smoke', BIN_NAME); // yields a helpful error if missing
}

function parseArgs(argv) {
    const opts = { count: 40, project: 'DEMO', dir: null, serve: false, keep: true };
    for (let i = 0; i < argv.length; i += 1) {
        const a = argv[i];
        if (a === '--count') opts.count = Number(argv[++i]);
        else if (a === '--project') opts.project = argv[++i];
        else if (a === '--dir') opts.dir = argv[++i];
        else if (a === '--serve') opts.serve = true;
        else if (a === '--rm') opts.keep = false;
        else if (a === '-h' || a === '--help') {
            printHelp();
            process.exit(0);
        }
    }
    return opts;
}

function printHelp() {
    console.log(`Usage: node scripts/generate-test-tasks.mjs [options]

Options:
  --count <n>      number of tasks to create (default 40)
  --project <name> project prefix to seed (default DEMO)
  --dir <path>     target directory (default: a fresh temp dir)
  --serve          after seeding, run \`lotar serve\` against the dir (Ctrl-C to stop)
  --rm             remove the target dir first if it exists
  -h, --help       show this help`);
}

function pick(arr, rng) {
    return arr[Math.floor(rng() * arr.length)];
}

// Deterministic-ish PRNG so re-runs on the same seed look similar (optional override).
function makeRng(seed) {
    let s = seed >>> 0;
    return () => {
        s = (s * 1664525 + 1013904223) >>> 0;
        return s / 0xffffffff;
    };
}

const CONFIG_YAML = `default:
  project: DEMO
issue:
  states: [Todo, InProgress, NeedsReview, Blocked, Done]
  types: [Feature, Bug, Chore, Epic, Spike]
  priorities: [Low, Medium, High, Critical]
  custom_fields:
    component: '*'
    story_points: '*'
`;

const VERBS = ['Add', 'Fix', 'Refactor', 'Improve', 'Support', 'Document', 'Wire up', 'Tune', 'Extend', 'Rework'];
const NOUNS = [
    'search filters', 'board drag-and-drop', 'SSE reconnect', 'config validation', 'sprint burndown',
    'MCP task_list pagination', 'automation cooldown', 'sync report diff', 'scan re-anchoring',
    'custom field editor', 'tag color picker', 'dependency graph', 'keyboard shortcuts', 'dark mode polish',
    'effort rollup', 'activity drawer', 'project switcher', 'CLI aliases', 'OpenAPI examples', 'smoke harness',
];
const AREAS = ['cli', 'api', 'ui', 'storage', 'mcp', 'automation', 'sync', 'scanner'];
const ASSIGNEES = ['alice', 'bob', 'carol', 'dave', 'erin', ''];
const STATUSES = ['Todo', 'Todo', 'Todo', 'InProgress', 'InProgress', 'NeedsReview', 'Blocked', 'Done'];
const TYPES = ['Feature', 'Feature', 'Bug', 'Bug', 'Chore', 'Epic', 'Spike'];
const PRIORITIES = ['Low', 'Medium', 'Medium', 'High', 'Critical'];
const TAGS = ['frontend', 'backend', 'ux', 'perf', 'tech-debt', 'docs', 'agents'];
const EFFORTS = ['', '', '2h', '4h', '1d', '2d', '3d', '1w'];

function run(bin, cwd, args) {
    const res = spawnSync(bin, args, {
        cwd,
        env: { ...process.env, LOTAR_IGNORE_HOME_CONFIG: '1' },
        encoding: 'utf8',
    });
    return res;
}

function main() {
    const opts = parseArgs(process.argv.slice(2));
    const bin = resolveBinary();
    if (!existsSync(bin)) {
        console.error(`LoTaR binary not found at ${bin}. Run \`npm run build\` (or \`npm run build:smoke\`) first, or set LOTAR_BINARY_PATH.`);
        process.exit(1);
    }

    const dir = opts.dir || path.join(os.tmpdir(), `lotar-demo-${process.pid}`);
    if (opts.dir && !opts.keep && existsSync(dir)) rmSync(dir, { recursive: true, force: true });
    mkdirSync(path.join(dir, '.tasks'), { recursive: true });
    writeFileSync(path.join(dir, '.tasks', 'config.yml'), CONFIG_YAML.replace('project: DEMO', `project: ${opts.project}`));

    const rng = makeRng((opts.count * 2654435761) >>> 0);
    let created = 0;
    let lastId = '';

    for (let i = 0; i < opts.count; i += 1) {
        const title = `${pick(VERBS, rng)} ${pick(NOUNS, rng)}`;
        const res = run(bin, dir, [
            'add', title,
            '-p', opts.project,
            '--type', pick(TYPES, rng),
            '--priority', pick(PRIORITIES, rng),
        ]);
        const m = res.stdout && res.stdout.match(/Created task:\s*([A-Z0-9_-]+)/i);
        if (!m) {
            if (res.stderr) process.stderr.write(res.stderr);
            continue;
        }
        created += 1;
        lastId = m[1];
        const id = m[1];

        // Enrich ~70% of tasks with varied lifecycle fields.
        if (rng() < 0.7) {
            const status = pick(STATUSES, rng);
            if (status !== 'Todo') run(bin, dir, ['status', id, status]);
        }
        const assignee = pick(ASSIGNEES, rng);
        if (assignee) run(bin, dir, ['assignee', id, assignee]);
        const effort = pick(EFFORTS, rng);
        if (effort) run(bin, dir, ['effort', id, effort]);
        if (rng() < 0.4) {
            const due = new Date(Date.now() + Math.floor((rng() - 0.3) * 21) * 86400000).toISOString().slice(0, 10);
            run(bin, dir, ['due-date', id, due]);
        }
        // A couple of tags via the task update path (custom_fields + tags).
        if (rng() < 0.6) {
            const tag = pick(TAGS, rng);
            const component = pick(AREAS, rng);
            run(bin, dir, ['task', 'edit', id, '--tag', tag, '--field', `component=${component}`]);
        }
        if (rng() < 0.25) {
            run(bin, dir, ['comment', id, '-m', 'Draft ready for review; see linked branch.']);
        }
    }

    console.log(`\nSeeded ${created} task(s) into: ${dir}`);
    console.log(`Project: ${opts.project} (last id: ${lastId || 'n/a'})`);
    console.log('\nNext steps:');
    console.log(`  cd "${dir}" && lotar list -p ${opts.project} --format table`);
    console.log(`  cd "${dir}" && lotar serve --port 8080 --open`);

    if (opts.serve) {
        console.log(`\nStarting lotar serve against ${dir} ... (Ctrl-C to stop)`);
        spawnSync(bin, ['serve', '--port', '8080'], { cwd: dir, env: { ...process.env, LOTAR_IGNORE_HOME_CONFIG: '1' }, stdio: 'inherit' });
    }
}

main();
