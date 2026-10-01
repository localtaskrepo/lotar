#!/usr/bin/env node
// Dedicated performance-budget runner (DEV-78).
//
// Invoked by `npm run test:rust:perf` (plain `node` invocation, works in a
// clean shell on every platform). Runs the performance_test binary under
// cargo-nextest with the controlled serial `perf` profile
// (.config/nextest.toml) and --retries 0 so a blown budget cannot be
// retried away. This dedicated command IS the explicit perf opt-in: it
// enables budgets for its child via LOTAR_TEST_PERF_BUDGETS=1 (also set
// explicitly in .github/workflows/performance.yml). There is no silent
// fallback: an inherited non-opt-in flag value is rejected instead of
// overridden, cargo-nextest must be on PATH, and a run that produces no
// (or incomplete) per-budget lines is an error, never a pass. Thresholds
// live in tests/performance_test.rs and must not be raised to make a run
// pass; local results come from an uncontrolled runner - CI measurements
// come from .github/workflows/performance.yml. See
// docs/developers/performance-budgets.md for the mode rationale.
//
// Usage:
//   node scripts/perf-runner.mjs [--dry-run]
// --dry-run prints the resolved child environment and exact child command
// instead of running it (used by behavior checks for runner
// selection/profile/flags).

import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const ENV_FLAG = 'LOTAR_TEST_PERF_BUDGETS';
const JUNIT_PATH = 'target/nextest/perf/perf-junit.xml';
const RUN_LOG_PATH = 'target/nextest/perf-run.log';

// Canonical budgets emitted by tests/performance_test.rs (label -> budget ms).
// The runner treats a run that does not report each of these exactly once as
// incomplete, so budget lines and this list cannot drift apart silently.
const EXPECTED_BUDGETS = new Map([
  ['cli_help', 1_000],
  ['cli_config_show', 2_000],
  ['cli_task_create_x10', 10_000],
  ['cli_task_list', 2_000],
  ['scan_medium_project', 5_000],
  ['many_small_tasks_create_x50', 20_000],
  ['many_small_tasks_list', 3_000],
  ['complex_tasks_create_x20', 15_000],
  ['complex_tasks_list', 3_000],
  ['large_file_scan', 8_000],
  ['rapid_operation_sequence', 10_000],
  ['error_recovery', 5_000],
  ['full_workflow', 30_000],
]);

const CONTROL_PREFIX = 'control_';

function fail(message, code = 2) {
  console.error(`perf-runner: error: ${message}`);
  process.exit(code);
}

const args = process.argv.slice(2);
if (args.some((a) => !['--dry-run'].includes(a))) {
  fail(`unknown arguments: ${args.join(' ')} (only --dry-run is supported; the perf suite always runs in full, no filters)`);
}
const dryRun = args.includes('--dry-run');

// The dedicated perf command is the opt-in. A pre-existing flag value that
// is not an explicit opt-in signals a conflicting environment: reject it
// instead of silently overriding or inheriting a correctness-only run.
const inheritedFlag = process.env[ENV_FLAG];
if (inheritedFlag !== undefined && !['1', 'true'].includes(inheritedFlag)) {
  fail(
    `${ENV_FLAG} is already set to ${JSON.stringify(inheritedFlag)} (not an opt-in value); ` +
      'unset it or set it to 1 - this runner always enforces budgets for its child',
  );
}
const resolvedFlag = inheritedFlag ?? '1';

const nextestArgs = [
  'nextest',
  'run',
  '--cargo-profile',
  'ci',
  '--profile',
  'perf',
  '--retries',
  '0',
  '--filterset',
  'binary(performance_test)',
];

if (dryRun) {
  console.log(`LOTAR_TEST_PERF_BUDGETS=${resolvedFlag} cargo ${nextestArgs.join(' ')}`);
  process.exit(0);
}

const childEnv = {
  ...process.env,
  NO_COLOR: '1',
  FORCE_COLOR: '0',
  CARGO_TERM_COLOR: 'never',
  [ENV_FLAG]: resolvedFlag,
};

console.log('== LoTaR performance budget run (DEV-78) ==');
console.log(`budget enforcement: on (${ENV_FLAG}=${resolvedFlag}${inheritedFlag === undefined ? ', set by this dedicated runner' : ', inherited'}); execution: serial (profile perf, test-threads=1); retries: 0`);
console.log(`child: cargo ${nextestArgs.join(' ')}`);

// cargo-nextest prints its UI (including captured test output with
// success-output=immediate) to stderr; tee both streams live and parse the
// combination so per-budget lines cannot hide in a stream we ignored.
const child = spawn('cargo', nextestArgs, {
  env: childEnv,
  stdio: ['ignore', 'pipe', 'pipe'],
});
let stdoutBuf = '';
let stderrBuf = '';
child.stdout.on('data', (chunk) => {
  stdoutBuf += chunk;
  process.stdout.write(chunk);
});
child.stderr.on('data', (chunk) => {
  stderrBuf += chunk;
  process.stderr.write(chunk);
});
const outcome = await new Promise((resolve) => {
  child.on('error', (error) => resolve({ error }));
  child.on('close', (code, signal) => resolve({ code, signal }));
});
if (outcome.error) {
  fail(`failed to spawn cargo: ${outcome.error.message} (cargo-nextest must be installed; no fallback to cargo test)`);
}
if (outcome.code === null) {
  fail(`cargo nextest terminated by signal ${outcome.signal ?? 'unknown'} - no perf result`);
}
const run = { status: outcome.code, stdout: stdoutBuf };

const stdout = `${stdoutBuf}\n${stderrBuf}`;
mkdirSync(path.dirname(RUN_LOG_PATH), { recursive: true });
writeFileSync(RUN_LOG_PATH, `# cargo nextest stdout\n${stdoutBuf}\n# cargo nextest stderr\n${stderrBuf}`);

const lineRe = /\[perf-budget\] (OK|SKIP|FAIL) label=(\S+) elapsed_ms=(\d+) budget_ms=(\d+)/;
const canonical = new Map();
const controls = [];
const unparsed = [];
for (const line of stdout.split('\n')) {
  const m = lineRe.exec(line);
  if (!m) {
    if (line.includes('[perf-budget]')) unparsed.push(line.trim());
    continue;
  }
  const entry = { status: m[1], label: m[2], elapsedMs: Number(m[3]), budgetMs: Number(m[4]) };
  if (entry.label.startsWith(CONTROL_PREFIX)) {
    controls.push(entry);
  } else if (canonical.has(entry.label)) {
    canonical.get(entry.label).push(entry);
  } else {
    canonical.set(entry.label, [entry]);
  }
}

const fmtMs = (ms) => `${String(ms).padEnd(8)}`;
console.log('\n== Performance budget summary ==');
console.log(`${'status'.padEnd(7)} ${'label'.padEnd(28)} ${'elapsed'.padEnd(9)} budget`);
const problems = [];
for (const [label, budgetMs] of EXPECTED_BUDGETS) {
  const entries = canonical.get(label) ?? [];
  if (entries.length === 0) {
    console.log(`${'MISSING'.padEnd(7)} ${label.padEnd(28)} ${fmtMs('-')} ${budgetMs}`);
    problems.push(`budget ${label} was not reported`);
    continue;
  }
  if (entries.length > 1) {
    problems.push(`budget ${label} reported ${entries.length} times (expected exactly once)`);
  }
  for (const e of entries) {
    console.log(`${e.status.padEnd(7)} ${e.label.padEnd(28)} ${fmtMs(`${e.elapsedMs}ms`)} ${e.budgetMs}ms`);
    if (e.status !== 'OK') {
      problems.push(`budget ${e.label} reported ${e.status} in enforced mode`);
    }
    if (e.budgetMs !== budgetMs) {
      problems.push(`budget ${e.label} reported budget_ms=${e.budgetMs}, runner expects ${budgetMs} (tests/performance_test.rs and scripts/perf-runner.mjs disagree)`);
    }
  }
  canonical.delete(label);
}
for (const [label, entries] of canonical) {
  for (const e of entries) {
    console.log(`${e.status.padEnd(7)} ${e.label.padEnd(28)} ${fmtMs(`${e.elapsedMs}ms`)} ${e.budgetMs}ms (not in expected set)`);
    problems.push(`unexpected budget label ${label}`);
  }
}
if (controls.length > 0) {
  console.log(`control checks: ${controls.map((c) => `${c.label}:${c.status}`).join(', ')}`);
}
if (unparsed.length > 0) {
  console.log(`unparsed [perf-budget] lines: ${unparsed.length}`);
  problems.push(`${unparsed.length} unparsed [perf-budget] line(s)`);
}

console.log(
  `\nnextest exit: ${run.status}; expected budgets: ${EXPECTED_BUDGETS.size}; ` +
    `problems: ${problems.length}; junit: ${JUNIT_PATH}; log: ${RUN_LOG_PATH}`,
);
console.log('note: local measurements come from an uncontrolled runner; compare against CI baseline artifacts.');

if (problems.length > 0) {
  for (const p of problems) console.error(`perf-runner: ${p}`);
  fail(`incomplete or inconsistent perf run (${problems.length} problem(s)) - refusing to report success`, 3);
}
process.exit(run.status);
