#!/usr/bin/env node
// DEV-79 runtime Git selection runner for Rust tests.
//
// Invoked by `npm run test:rust` / `npm run test:rust:agent` (package.json).
// Cross-platform Node replacement for the previous bash wrapper: it probes the
// real Git capability with an isolated `git init` (same contract as
// tests/common/mod.rs `require_git`), and when the capability is unavailable
// selects the `gitless` nextest profile (.config/nextest.toml), whose
// default-filter excludes exactly the source-local `git_required` test
// modules. Excluded tests are REPORTED (never counted as passed): nextest
// prints its native "skipped via profile.gitless.default-filter" counts, and
// this runner additionally lists the excluded test names whenever the
// argument shape allows an exact `nextest list` mirror (no user filters).
// With LOTAR_REQUIRE_GIT=1 (CI) the runner aborts before compiling or
// selecting anything if the probe fails — the gitless profile can never
// silently weaken a designated full-coverage runner. There is no fallback to
// `cargo test`, and user-provided -E filtersets are passed through verbatim:
// the wrapper never appends its own -E (repeated -E flags are unioned by
// nextest, which would re-include the excluded tests).
//
// Usage:
//   node scripts/rust-test-runner.mjs [--agent] [--dry-run]
//          [--force-probe available|unavailable (dry-run only)]
//          [cargo-nextest run arguments...]
// --agent reproduces the old `test:rust:agent` contract inside this runner:
//   --cargo-profile ci --color never --failure-output immediate-final
//   --retries 0 plus child-scoped NO_COLOR/FORCE_COLOR/CARGO_TERM_COLOR.
// --dry-run prints the resolved probe, plan, and child command without
//   running tests (used by scripts/tests/rust-test-runner.test.mjs).

import { spawn } from 'node:child_process';
import fs from 'node:fs/promises';
import { existsSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';

// Windows child processes require these bootstrap variables (crypto/SSL,
// console, temp resolution); they are passed through case-insensitively and
// never include any Git routing/config keys.
const WINDOWS_BOOTSTRAP_KEYS = [
  'SystemRoot',
  'windir',
  'TEMP',
  'TMP',
  'COMSPEC',
  'SYSTEMDRIVE',
];

export function buildProbeEnv(parentEnv, tmp, gitconfig, { windows = process.platform === 'win32' } = {}) {
  const env = {
    PATH: parentEnv.PATH ?? '',
    GIT_CONFIG_NOSYSTEM: '1',
    GIT_CONFIG_GLOBAL: gitconfig,
    HOME: tmp,
    USERPROFILE: tmp,
    XDG_CONFIG_HOME: path.join(tmp, '.config'),
  };
  if (windows) {
    for (const wanted of WINDOWS_BOOTSTRAP_KEYS) {
      const found = Object.keys(parentEnv).find(
        (key) => key.toLowerCase() === wanted.toLowerCase(),
      );
      if (found !== undefined && !/^git_/i.test(found)) {
        env[found] = parentEnv[found];
      }
    }
  }
  return env;
}

export const GITLESS_PROFILE = 'gitless';
export const GITLESS_TEST_EXPR = 'test(/(^|::)git_required::/)';
export const GITLESS_JUNIT_PATH = 'target/nextest/gitless/junit.xml';
const PROBE_TIMEOUT_MS = 5000;

// Flags safe to mirror onto `nextest list` for the exclusion inventory: they
// select what is built or which targets are included, and list accepts the
// exact same spellings. Anything else (user -E, substring filters, run-only
// flags) disables the detailed inventory; the native nextest skip counts in
// the run output remain the always-present report.
const INVENTORY_SAFE_VALUE_FLAGS = new Set([
  '--cargo-profile',
  '-F',
  '--features',
  '--target',
  '--manifest-path',
  '-p',
  '--package',
  '--exclude',
  '--test',
  '--bench',
  '--bin',
  '--example',
]);
const INVENTORY_SAFE_BARE_FLAGS = new Set([
  '--workspace',
  '--lib',
  '--bins',
  '--tests',
  '--benches',
  '--examples',
  '--all-targets',
  '--all-features',
  '--no-default-features',
  '-r',
  '--release',
]);

// Nextest flags that consume a separate value token. Their value must not be
// mistaken for a positional test-name filter when deciding whether a detailed
// exclusion inventory is safe.
const NEXTEST_VALUE_FLAGS = new Set([
  '--cargo-profile',
  '--build-jobs',
  '-p',
  '--package',
  '--exclude',
  '-F',
  '--features',
  '--target',
  '--manifest-path',
  '--config-file',
  '--test',
  '--bench',
  '--bin',
  '--example',
  '--test-threads',
  '--retries',
  '--slow-timeout',
  '--status-level',
  '--failure-output',
  '--success-output',
  '--final-status-level',
  '--color',
  '-T',
  '--message-format',
  '--run-ignored',
  '--cargo-message-format',
]);

export class RunnerError extends Error {
  constructor(message, code = 2) {
    super(message);
    this.code = code;
  }
}

export function parseWrapperArgs(argv) {
  const state = {
    agent: false,
    dryRun: false,
    forceProbe: null,
    passthrough: [],
    explicitProfile: null,
    hasFilterset: false,
    hasIgnoreDefaultFilter: false,
    hasPositionalFilter: false,
    sawSeparator: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (state.sawSeparator) {
      state.passthrough.push(token);
      continue;
    }
    if (token === '--') {
      state.sawSeparator = true;
      state.passthrough.push(token);
      continue;
    }
    if (token === '--agent') {
      state.agent = true;
      continue;
    }
    if (token === '--dry-run') {
      state.dryRun = true;
      continue;
    }
    if (token === '--force-probe') {
      const value = argv[i + 1];
      if (value !== 'available' && value !== 'unavailable') {
        throw new RunnerError('--force-probe requires available|unavailable');
      }
      state.forceProbe = value;
      i += 1;
      continue;
    }
    if (token.startsWith('--force-probe=')) {
      const value = token.slice('--force-probe='.length);
      if (value !== 'available' && value !== 'unavailable') {
        throw new RunnerError(`invalid --force-probe value: ${value}`);
      }
      state.forceProbe = value;
      continue;
    }
    if (token === '-P' || token === '--profile') {
      const value = argv[i + 1];
      if (value === undefined || value.startsWith('-')) {
        throw new RunnerError(`${token} requires a profile name`);
      }
      state.explicitProfile = value;
      i += 1;
      continue;
    }
    if (token.startsWith('--profile=')) {
      state.explicitProfile = token.slice('--profile='.length);
      continue;
    }
    if (/^-P.+/.test(token)) {
      state.explicitProfile = token.slice(2);
      continue;
    }
    if (token === '-E' || token === '--filterset') {
      state.hasFilterset = true;
    } else if (token === '--ignore-default-filter') {
      state.hasIgnoreDefaultFilter = true;
    }
    if (!token.startsWith('-')) {
      state.hasPositionalFilter = true;
    }
    state.passthrough.push(token);
    if (NEXTEST_VALUE_FLAGS.has(token)) {
      const value = argv[i + 1];
      if (value === undefined) {
        throw new RunnerError(`${token} requires a value`);
      }
      state.passthrough.push(value);
      i += 1;
    }
  }
  if (state.forceProbe !== null && !state.dryRun) {
    throw new RunnerError('--force-probe is only valid together with --dry-run');
  }
  return state;
}

export function validateRequireGitEnv(raw) {
  if (raw === undefined || raw === '') {
    return 'off';
  }
  if (raw === '1') {
    return 'required';
  }
  throw new RunnerError(
    `LOTAR_REQUIRE_GIT must be set to '1' or left unset; got ${JSON.stringify(raw)}`,
  );
}

export function resolveExplicitProfile(parsed, envProfile) {
  const fromEnv = typeof envProfile === 'string' && envProfile !== '' ? envProfile : null;
  // Command line wins over the environment, matching nextest's own
  // precedence (--profile beats NEXTEST_PROFILE).
  return parsed.explicitProfile ?? fromEnv;
}

export function resolvePlan({ probe, requireGit, explicitProfile, parsed }) {
  const gitlessSelected =
    explicitProfile === GITLESS_PROFILE ||
    (explicitProfile === null && !probe.available);
  if (requireGit === 'required' && !probe.available) {
    throw new RunnerError(
      `LOTAR_REQUIRE_GIT=1 requires real Git capability, but the isolated probe failed: ${probe.reason}. ` +
        'Refusing to build, run, or select the gitless profile on a designated full-coverage runner.',
    );
  }
  if (requireGit === 'required' && gitlessSelected) {
    throw new RunnerError(
      'LOTAR_REQUIRE_GIT=1 forbids the gitless exclusion profile: a designated full-coverage runner must execute every Git-dependent test.',
    );
  }
  if (parsed.hasIgnoreDefaultFilter && !probe.available && gitlessSelected) {
    throw new RunnerError(
      '--ignore-default-filter would disable the gitless default-filter while Git capability is unavailable; the excluded git_required tests would then fail closed test-by-test. Remove the flag or run on a Git-capable machine.',
    );
  }
  const warnings = [];
  if (explicitProfile !== null && explicitProfile !== GITLESS_PROFILE && !probe.available) {
    warnings.push(
      `explicit profile ${explicitProfile} respected without override, but Git capability is unavailable (${probe.reason}); ` +
        'git_required tests selected by that profile will FAIL CLOSED via require_git() instead of being excluded.',
    );
  }
  return {
    gitlessSelected,
    injectProfile: explicitProfile === null && !probe.available,
    warnings,
  };
}

export function buildRunArgs(parsed, plan) {
  const agentBase = parsed.agent
    ? [
        '--cargo-profile',
        'ci',
        '--color',
        'never',
        '--failure-output',
        'immediate-final',
        '--retries',
        '0',
      ]
    : [];
  const inject = plan.injectProfile ? ['--profile', GITLESS_PROFILE] : [];
  return [...inject, ...agentBase, ...parsed.passthrough];
}

export function inventoryAllowed(parsed) {
  return (
    !parsed.hasFilterset &&
    !parsed.hasPositionalFilter &&
    !parsed.sawSeparator &&
    !parsed.hasIgnoreDefaultFilter
  );
}

export function buildInventoryArgs(parsed) {
  const mirrored = [];
  const passthrough = parsed.passthrough;
  for (let i = 0; i < passthrough.length; i += 1) {
    const token = passthrough[i];
    if (INVENTORY_SAFE_VALUE_FLAGS.has(token)) {
      const value = passthrough[i + 1];
      if (value === undefined) {
        return null;
      }
      mirrored.push(token, value);
      i += 1;
      continue;
    }
    if (INVENTORY_SAFE_BARE_FLAGS.has(token)) {
      mirrored.push(token);
      continue;
    }
    if (
      token.startsWith('--cargo-profile=') ||
      token.startsWith('--features=') ||
      token.startsWith('--package=') ||
      token.startsWith('--exclude=') ||
      token.startsWith('--target=')
    ) {
      mirrored.push(token);
      continue;
    }
    return null;
  }
  return [
    'nextest',
    'list',
    ...mirrored,
    '--profile',
    'default',
    '-E',
    GITLESS_TEST_EXPR,
    '--message-format',
    'json',
  ];
}

// `cargo nextest list --message-format json` emits ONE JSON document (no
// trailing newline): { rust-build-meta, test-count, rust-suites: { <binary
// id>: { "binary-name": ..., testcases: { <case path>: ... } } } }. Case paths
// are relative to their binary; report canonical <binary>::<case> names.
export function parseListTestNames(stdoutText) {
  let document;
  try {
    document = JSON.parse(stdoutText);
  } catch {
    throw new RunnerError('nextest list emitted invalid JSON; cannot compute exclusions');
  }
  const names = [];
  const suites = document?.['rust-suites'];
  if (typeof suites !== 'object' || suites === null) {
    throw new RunnerError('nextest list JSON has no rust-suites; cannot compute exclusions');
  }
  for (const suite of Object.values(suites)) {
    const binary = suite?.['binary-name'];
    const cases = suite?.testcases;
    if (typeof binary !== 'string' || typeof cases !== 'object' || cases === null) {
      throw new RunnerError('nextest list JSON has an unexpected suite shape');
    }
    for (const [testCase, meta] of Object.entries(cases)) {
      // `nextest list` does not drop non-matching tests: it reports
      // filter-match=mismatch on them. Only counted matches are exclusions.
      if (meta?.['filter-match']?.status === 'matches') {
        names.push(`${binary}::${testCase}`);
      }
    }
  }
  names.sort();
  return names;
}

async function realGitLaunch(cwd, gitBinary, env, timeoutMs) {
  return new Promise((resolve, reject) => {
    const child = spawn(gitBinary, ['init', '--quiet'], {
      cwd,
      env,
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    let stderr = '';
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) {
        return;
      }
      settled = true;
      child.kill('SIGKILL');
      reject(new RunnerError(`git init probe timed out after ${timeoutMs}ms`));
    }, timeoutMs);
    child.on('error', (error) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      reject(error);
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('close', (code) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      resolve({ code, stderr: stderr.trim() });
    });
  });
}

// Probe Git the way tests/common/mod.rs does: real `git init` in an isolated
// temporary directory with system and global config disabled, then verify the
// .git artifact exists. The parent environment is never mutated; the
// isolation variables are child-scoped only.
export async function probeGitCapability({
  gitBinary = 'git',
  timeoutMs = PROBE_TIMEOUT_MS,
  launcher = realGitLaunch,
  artifactExists = (dir) => existsSync(path.join(dir, '.git')),
} = {}) {
  let tmp;
  try {
    tmp = await fs.mkdtemp(path.join(os.tmpdir(), 'lotar-git-probe-'));
  } catch (error) {
    return {
      available: false,
      reason: `cannot create probe directory: ${error.message}`,
    };
  }
  try {
    const gitconfig = path.join(tmp, 'gitconfig');
    await fs.writeFile(gitconfig, '');
    const env = buildProbeEnv(process.env, tmp, gitconfig);
    let outcome;
    try {
      outcome = await Promise.race([
        launcher(tmp, gitBinary, env, timeoutMs),
        new Promise((_, reject) => {
          const timer = setTimeout(
            () => reject(new RunnerError(`git init probe timed out after ${timeoutMs}ms`)),
            timeoutMs,
          );
          timer.unref?.();
        }),
      ]);
    } catch (error) {
      return {
        available: false,
        reason: `cannot run git: ${error.message}`,
      };
    }
    if (outcome.code !== 0) {
      return {
        available: false,
        reason: `git init failed with status ${outcome.code}: ${outcome.stderr}`,
      };
    }
    if (!artifactExists(tmp)) {
      return {
        available: false,
        reason: 'git init exited successfully but left no .git artifact',
      };
    }
    return { available: true, reason: '' };
  } finally {
    await fs.rm(tmp, { recursive: true, force: true }).catch(() => {});
  }
}

export async function runInventoryList(listArgs, { spawnImpl = spawn } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawnImpl('cargo', listArgs, {
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
      reject(new RunnerError('exclusion inventory nextest list timed out'));
    }, PROBE_TIMEOUT_MS * 12);
    child.on('error', (error) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      reject(new RunnerError(`cannot run cargo nextest list: ${error.message}`));
    });
    child.stdout?.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr?.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('close', (code) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      if (code !== 0) {
        reject(
          new RunnerError(
            `exclusion inventory nextest list failed with status ${code}: ${stderr.trim()}`,
          ),
        );
        return;
      }
      resolve(stdout);
    });
  });
}

async function runTests(runArgs, childEnv) {
  return new Promise((resolve) => {
    const child = spawn('cargo', ['nextest', 'run', ...runArgs], {
      stdio: ['inherit', 'inherit', 'inherit'],
      env: childEnv,
    });
    const forward = (signal) => {
      child.kill(signal);
    };
    const signals = ['SIGINT', 'SIGTERM', 'SIGHUP'];
    for (const signal of signals) {
      process.on(signal, () => forward(signal));
    }
    child.on('error', (error) => {
      console.error(`rust-test-runner: error: cannot run cargo nextest: ${error.message}`);
      resolve({ exitCode: 2 });
    });
    child.on('close', (code, signal) => {
      if (code !== null) {
        resolve({ exitCode: code });
        return;
      }
      const numeric = typeof signal === 'string' ? os.constants.signals[signal] : undefined;
      resolve({ exitCode: numeric === undefined ? 1 : 128 + numeric });
    });
  });
}

export async function main({ argv = process.argv.slice(2), env = process.env, stdout = process.stdout, stderr = process.stderr } = {}) {
  const parsed = parseWrapperArgs(argv);
  const requireGit = validateRequireGitEnv(env.LOTAR_REQUIRE_GIT);
  const explicitProfile = resolveExplicitProfile(parsed, env.NEXTEST_PROFILE);
  const probe =
    parsed.forceProbe === 'available'
      ? { available: true, reason: 'forced available (dry-run only)' }
      : parsed.forceProbe === 'unavailable'
        ? { available: false, reason: 'forced unavailable (dry-run only)' }
        : await probeGitCapability();
  const plan = resolvePlan({ probe, requireGit, explicitProfile, parsed });
  const runArgs = buildRunArgs(parsed, plan);
  const childEnv = { ...env };
  if (parsed.agent) {
    childEnv.NO_COLOR = '1';
    childEnv.FORCE_COLOR = '0';
    childEnv.CARGO_TERM_COLOR = 'never';
  }

  const summary = {
    probe,
    requireGit,
    explicitProfile,
    plan,
    command: `cargo nextest run ${runArgs.join(' ')}`.trim(),
    envAdditions: parsed.agent ? ['NO_COLOR=1', 'FORCE_COLOR=0', 'CARGO_TERM_COLOR=never'] : [],
  };

  if (parsed.dryRun) {
    stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
    return 0;
  }

  for (const warning of plan.warnings) {
    stderr.write(`rust-test-runner: warning: ${warning}\n`);
  }

  let inventory = null;
  if (plan.gitlessSelected) {
    stderr.write(
      `rust-test-runner: Git capability unavailable (${probe.reason}); selecting profile ${GITLESS_PROFILE}.\n` +
        `git_required tests are EXCLUDED by the profile default-filter, not passed; nextest reports the skipped counts natively.\n`,
    );
    if (inventoryAllowed(parsed)) {
      const listArgs = buildInventoryArgs(parsed);
      if (listArgs !== null) {
        const listing = await runInventoryList(listArgs);
        const names = parseListTestNames(listing);
        inventory = names;
        stderr.write(
          `excluded git_required tests (${names.length}) across the selected targets:\n` +
            `${names.map((name) => `  ${name}`).join('\n')}\n`,
        );
      }
    } else {
      stderr.write(
        'detailed exclusion inventory skipped (user filters present); see the native skipped-count summary in the run output above.\n',
      );
    }
  }

  const outcome = await runTests(runArgs, childEnv);
  if (plan.gitlessSelected) {
    stderr.write(
      `JUnit report for this run: ${GITLESS_JUNIT_PATH} (gitless profile dir; git_required tests excluded, not passed).\n`,
    );
  }
  return outcome.exitCode;
}

const isMain =
  process.argv[1] !== undefined &&
  import.meta.url === new URL(`file://${path.resolve(process.argv[1])}`).href;
if (isMain) {
  main().then(
    (code) => process.exit(code),
    (error) => {
      const message = error instanceof RunnerError ? error.message : (error?.stack ?? String(error));
      console.error(`rust-test-runner: error: ${message}`);
      process.exit(error instanceof RunnerError ? error.code : 2);
    },
  );
}
